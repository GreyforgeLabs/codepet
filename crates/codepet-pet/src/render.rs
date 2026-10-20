//! Software rendering of the scene's [`View`] into a small premultiplied RGBA pixmap.
//!
//! Only the current species' frames are decoded, pre-scaled once to the drawn size and kept
//! in a small bounded cache; icons likewise. Text uses the system sans faces (ab_glyph, parsed lazily) with a
//! glyph cache. Colors and geometry follow the QML scene this replaces.

use crate::scene::{Measure, Rect, View};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tiny_skia::{
    Color, FillRule, FilterQuality, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform,
};

const SPRITE_CACHE: usize = 24;
const ICON_CACHE: usize = 32;

fn argb(hex: u32) -> Color {
    let a = ((hex >> 24) & 0xff) as u8;
    let r = ((hex >> 16) & 0xff) as u8;
    let g = ((hex >> 8) & 0xff) as u8;
    let b = (hex & 0xff) as u8;
    Color::from_rgba8(r, g, b, a)
}

/// Decodes a WebP file to a premultiplied pixmap (None for anything unreadable).
pub fn decode_webp(path: &Path) -> Option<Pixmap> {
    let data = std::fs::read(path).ok()?;
    if data.len() > 4 << 20 {
        return None;
    }
    let mut dec = image_webp::WebPDecoder::new(std::io::Cursor::new(data)).ok()?;
    dec.set_memory_limit(16 << 20);
    let (w, h) = dec.dimensions();
    if w == 0 || h == 0 || w > 1024 || h > 1024 {
        return None;
    }
    let mut buf = vec![0u8; dec.output_buffer_size()?];
    dec.read_image(&mut buf).ok()?;
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    if dec.has_alpha() {
        for px in buf.chunks_exact(4) {
            let a = px[3] as u32;
            rgba.extend_from_slice(&[
                ((px[0] as u32 * a + 127) / 255) as u8,
                ((px[1] as u32 * a + 127) / 255) as u8,
                ((px[2] as u32 * a + 127) / 255) as u8,
                px[3],
            ]);
        }
    } else {
        for px in buf.chunks_exact(3) {
            rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
    }
    Pixmap::from_vec(rgba, tiny_skia::IntSize::from_wh(w, h)?)
}

/// Scales a pixmap to `size`×`size` px (aspect fit, high-quality filter).
fn scaled(src: &Pixmap, size: u32) -> Option<Pixmap> {
    let size = size.max(1);
    let mut out = Pixmap::new(size, size)?;
    let s = (size as f32 / src.width() as f32).min(size as f32 / src.height() as f32);
    let (w, h) = (src.width() as f32 * s, src.height() as f32 * s);
    let t = Transform::from_scale(s, s)
        .post_translate((size as f32 - w) / 2.0, (size as f32 - h) / 2.0);
    let paint = PixmapPaint {
        quality: FilterQuality::Bicubic,
        ..PixmapPaint::default()
    };
    out.draw_pixmap(0, 0, src.as_ref(), &paint, t, None);
    Some(out)
}

struct Lru {
    cap: usize,
    tick: u64,
    map: HashMap<(String, u32), (u64, Option<Pixmap>)>,
}

impl Lru {
    fn new(cap: usize) -> Lru {
        Lru {
            cap,
            tick: 0,
            map: HashMap::new(),
        }
    }
    fn get_or(
        &mut self,
        key: (String, u32),
        load: impl FnOnce() -> Option<Pixmap>,
    ) -> Option<&Pixmap> {
        self.tick += 1;
        let tick = self.tick;
        if !self.map.contains_key(&key) {
            if self.map.len() >= self.cap {
                if let Some(old) = self
                    .map
                    .iter()
                    .min_by_key(|(_, v)| v.0)
                    .map(|(k, _)| k.clone())
                {
                    self.map.remove(&old);
                }
            }
            self.map.insert(key.clone(), (tick, load()));
        }
        let e = self.map.get_mut(&key)?;
        e.0 = tick;
        e.1.as_ref()
    }
    fn clear(&mut self) {
        self.map.clear();
    }
}

/// A rasterized glyph: pixel offset from the pen at the baseline, size, advance, coverage.
pub struct Glyph {
    left: i32,
    top: i32,
    w: usize,
    h: usize,
    advance: f32,
    coverage: Vec<u8>,
}

/// Two system faces (regular, bold), parsed lazily by ab_glyph on first use. Glyphs are
/// rasterized only when drawn and kept in a small bounded cache.
pub struct Fonts {
    regular: Option<ab_glyph::FontVec>,
    bold: Option<ab_glyph::FontVec>,
    loaded: bool,
    glyphs: HashMap<(char, u32, bool), Glyph>,
}

fn font_file(pattern: &str) -> Option<PathBuf> {
    let out = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", pattern])
        .output()
        .ok()?;
    let p = PathBuf::from(String::from_utf8(out.stdout).ok()?.trim());
    p.is_file().then_some(p)
}

fn load_font(pattern: &str) -> Option<ab_glyph::FontVec> {
    let path = font_file(pattern)?;
    let data = std::fs::read(path).ok().filter(|d| d.len() <= 32 << 20)?;
    ab_glyph::FontVec::try_from_vec(data).ok()
}

impl Fonts {
    pub fn new() -> Fonts {
        Fonts {
            regular: None,
            bold: None,
            loaded: false,
            glyphs: HashMap::new(),
        }
    }
    fn ensure(&mut self) {
        if !self.loaded {
            self.loaded = true;
            self.regular = load_font("sans-serif");
            self.bold = load_font("sans-serif:bold");
        }
    }
    fn font(&self, bold: bool) -> Option<&ab_glyph::FontVec> {
        if bold {
            self.bold.as_ref().or(self.regular.as_ref())
        } else {
            self.regular.as_ref()
        }
    }
    pub fn width(&mut self, text: &str, size: f32, bold: bool) -> f32 {
        use ab_glyph::{Font, ScaleFont};
        self.ensure();
        let Some(f) = self.font(bold) else {
            return text.chars().count() as f32 * size * 0.55;
        };
        let sf = f.as_scaled(ab_glyph::PxScale::from(size * px_to_scale(f)));
        text.chars().map(|c| sf.h_advance(f.glyph_id(c))).sum()
    }
    /// (ascent, descent) in px; descent is negative.
    fn vmetrics(&mut self, size: f32, bold: bool) -> (f32, f32) {
        use ab_glyph::{Font, ScaleFont};
        self.ensure();
        match self.font(bold) {
            Some(f) => {
                let sf = f.as_scaled(ab_glyph::PxScale::from(size * px_to_scale(f)));
                (sf.ascent(), sf.descent())
            }
            None => (size * 0.93, -size * 0.24),
        }
    }
    fn glyph(&mut self, c: char, size: f32, bold: bool) -> Option<&Glyph> {
        use ab_glyph::{Font, ScaleFont};
        self.ensure();
        let key = (c, (size * 4.0).round() as u32, bold);
        if !self.glyphs.contains_key(&key) {
            if self.glyphs.len() > 256 {
                self.glyphs.clear();
            }
            let f = self.font(bold)?;
            let scale = ab_glyph::PxScale::from(size * px_to_scale(f));
            let id = f.glyph_id(c);
            let advance = f.as_scaled(scale).h_advance(id);
            let g = match f
                .outline_glyph(id.with_scale_and_position(scale, ab_glyph::point(0.0, 0.0)))
            {
                Some(o) => {
                    let b = o.px_bounds();
                    let (w, h) = (b.width().max(0.0) as usize, b.height().max(0.0) as usize);
                    let mut coverage = vec![0u8; w * h];
                    o.draw(|x, y, v| {
                        let i = y as usize * w + x as usize;
                        if i < coverage.len() {
                            coverage[i] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                        }
                    });
                    Glyph {
                        left: b.min.x as i32,
                        top: b.min.y as i32,
                        w,
                        h,
                        advance,
                        coverage,
                    }
                }
                None => Glyph {
                    left: 0,
                    top: 0,
                    w: 0,
                    h: 0,
                    advance,
                    coverage: Vec::new(),
                },
            };
            self.glyphs.insert(key, g);
        }
        self.glyphs.get(&key)
    }
}

/// ab_glyph's PxScale is the font's height (ascent - descent); this converts a CSS-like
/// pixel size (em) to it, so sizes match the QML text they replace.
fn px_to_scale(f: &ab_glyph::FontVec) -> f32 {
    use ab_glyph::Font;
    let upm = f.units_per_em().unwrap_or(1000.0);
    let h = f.height_unscaled();
    if upm > 0.0 && h > 0.0 {
        h / upm
    } else {
        1.0
    }
}

pub struct Renderer {
    pets: PathBuf,
    icons: PathBuf,
    sprites: Lru,
    icon_cache: Lru,
    pub fonts: RefCell<Fonts>,
}

impl Measure for Renderer {
    fn width(&self, text: &str, size: f32) -> f32 {
        self.fonts.borrow_mut().width(text, size, false)
    }
}

impl Renderer {
    pub fn new(assets: &Path) -> Renderer {
        Renderer {
            pets: assets.join("pets"),
            icons: assets.join("icons"),
            sprites: Lru::new(SPRITE_CACHE),
            icon_cache: Lru::new(ICON_CACHE),
            fonts: RefCell::new(Fonts::new()),
        }
    }

    /// Drops decoded images (the pet changed species, or the scene went idle for long).
    pub fn trim(&mut self) {
        self.sprites.clear();
        self.icon_cache.clear();
        self.fonts.borrow_mut().glyphs.clear();
    }

    /// Draws `view` into a new pixmap covering `area` (scene px) at integer `scale`.
    pub fn draw(&mut self, view: &View, area: Rect, scale: f64) -> Option<Pixmap> {
        let w = ((area.w * scale).ceil() as u32).max(1);
        let h = ((area.h * scale).ceil() as u32).max(1);
        let mut pm = Pixmap::new(w, h)?;
        let base = Transform::from_scale(scale as f32, scale as f32)
            .pre_translate(-area.x as f32, -area.y as f32);
        let s = scale as f32;
        if let Some((species, path, mirror, r, rotation, opacity)) = &view.sprite {
            let px = (r.w * scale).round() as u32;
            let file = self.pets.join(species).join(path);
            let key = (file.to_string_lossy().into_owned(), px);
            if let Some(img) = self
                .sprites
                .get_or(key, || decode_webp(&file).and_then(|p| scaled(&p, px)))
            {
                let mut t = base
                    .pre_translate(r.x as f32, r.y as f32)
                    .pre_scale(1.0 / s, 1.0 / s);
                let half = px as f32 / 2.0;
                if *rotation != 0.0 {
                    t = t
                        .pre_translate(half, half)
                        .pre_rotate(*rotation as f32)
                        .pre_translate(-half, -half);
                }
                if *mirror {
                    t = t.pre_translate(px as f32, 0.0).pre_scale(-1.0, 1.0);
                }
                let paint = PixmapPaint {
                    opacity: *opacity as f32,
                    quality: FilterQuality::Bilinear,
                    ..PixmapPaint::default()
                };
                pm.draw_pixmap(0, 0, img.as_ref(), &paint, t, None);
            }
        }
        if let Some(r) = view.pinned_tag {
            rounded(&mut pm, base, r, 8.0, argb(0xee22303b), None);
            self.text_centered(&mut pm, base, s, r, "Pinned", 12.0, false, argb(0xffffffff));
        }
        if let Some((r, icons, label)) = &view.bubble {
            rounded(
                &mut pm,
                base,
                *r,
                r.h / 2.0,
                argb(0xf2ffffff),
                Some(argb(0x33000000)),
            );
            let icon = if label.is_empty() { 40.0 } else { 24.0 };
            let mut x = r.x + 8.0;
            for i in icons {
                self.icon(
                    &mut pm,
                    base,
                    s,
                    i,
                    Rect::new(x, r.y + (r.h - icon) / 2.0, icon, icon),
                    1.0,
                );
                x += icon + 4.0;
            }
            if !label.is_empty() {
                let max = (r.x + r.w - 8.0) - (x + 2.0);
                let text = elide(&mut self.fonts.borrow_mut(), label, 12.0, max as f32);
                self.text(
                    &mut pm,
                    base,
                    s,
                    x + 2.0,
                    r.y + r.h / 2.0,
                    &text,
                    12.0,
                    false,
                    argb(0xff24303c),
                );
            }
        }
        if let Some((r, msg)) = &view.feedback {
            rounded(
                &mut pm,
                base,
                *r,
                12.0,
                argb(0xf2232b36),
                Some(argb(0x897d8b9b)),
            );
            let lines = crate::scene::wrap(self, msg, 12.0, (r.w - 20.0) as f32);
            for (i, line) in lines.iter().enumerate() {
                let lr = Rect::new(r.x + 10.0, r.y + 8.0 + i as f64 * 16.0, r.w - 20.0, 16.0);
                self.text_centered(&mut pm, base, s, lr, line, 12.0, false, argb(0xfff0f4fa));
            }
        }
        if let Some(r) = view.badge {
            circle(
                &mut pm,
                base,
                r,
                argb(0xffeab276),
                Some((argb(0xff2b303a), 2.0)),
            );
            self.text_centered(&mut pm, base, s, r, "!", 12.0, true, argb(0xff302719));
        }
        if let Some((r, hovered, pressed)) = view.hide_pill {
            let fill = if pressed {
                0xff667c91
            } else if hovered {
                0xff49596c
            } else {
                0xf2232b36
            };
            rounded(
                &mut pm,
                base,
                r,
                r.h / 2.0,
                argb(fill),
                Some(argb(0x897d8b9b)),
            );
            let label_w = self.fonts.borrow_mut().width("HIDE", 11.0, true) as f64 + 2.4;
            let total = 16.0 + 5.0 + label_w;
            let x0 = r.x + (r.w - total) / 2.0;
            eye_slash(&mut pm, base, x0, r.y + r.h / 2.0 - 6.0, argb(0xfff0f4fa));
            self.text_spaced(
                &mut pm,
                base,
                s,
                x0 + 21.0,
                r.y + r.h / 2.0,
                "HIDE",
                11.0,
                true,
                0.8,
                argb(0xfff0f4fa),
            );
        }
        if let Some((r, buttons, close, close_hover, message)) = &view.wheel {
            rounded(
                &mut pm,
                base,
                *r,
                24.0,
                argb(0xf2232b36),
                Some(argb(0x897d8b9b)),
            );
            for b in buttons {
                let fill = if b.pressed {
                    0xff667c91
                } else if b.hovered {
                    0xff49596c
                } else {
                    0xff333f50
                };
                let border = if b.focused { 0xffa8dacc } else { 0xff586679 };
                let alpha = if b.enabled { 1.0 } else { 0.45 };
                rounded(
                    &mut pm,
                    base,
                    b.rect,
                    18.0 * b.rect.w / 64.0,
                    with_alpha(argb(fill), alpha),
                    Some(with_alpha(argb(border), alpha)),
                );
                let k = b.rect.w / 64.0;
                let ir = Rect::new(
                    b.rect.x + (b.rect.w - 36.0 * k) / 2.0,
                    b.rect.y + 3.0 * k,
                    36.0 * k,
                    36.0 * k,
                );
                self.icon(
                    &mut pm,
                    base,
                    s,
                    &format!("{}.webp", b.icon),
                    ir,
                    alpha as f32,
                );
                let lr = Rect::new(b.rect.x, b.rect.y + 41.0 * k, b.rect.w, 16.0 * k);
                self.text_centered(
                    &mut pm,
                    base,
                    s,
                    lr,
                    &b.label,
                    (12.0 * k) as f32,
                    false,
                    with_alpha(argb(0xfff0f4fa), alpha),
                );
            }
            let fill = if *close_hover { 0xff49596c } else { 0xff333f50 };
            let focused = buttons.iter().all(|b| !b.focused);
            rounded(
                &mut pm,
                base,
                *close,
                16.0,
                argb(fill),
                Some(argb(if focused { 0xffa8dacc } else { 0xff586679 })),
            );
            self.text_centered(&mut pm, base, s, *close, "×", 24.0, false, argb(0xffffffff));
            let k = r.w / crate::scene::WHEEL_W;
            let mr = Rect::new(r.x + 77.0 * k, r.y + 148.0 * k, 98.0 * k, 48.0 * k);
            let lines: Vec<String> = crate::scene::wrap(self, message, 11.0, mr.w as f32);
            let top = mr.y + (mr.h - lines.len() as f64 * 14.0) / 2.0;
            for (i, line) in lines.iter().enumerate() {
                let lr = Rect::new(mr.x, top + i as f64 * 14.0, mr.w, 14.0);
                self.text_centered(&mut pm, base, s, lr, line, 11.0, false, argb(0xffcedce9));
            }
        }
        Some(pm)
    }

    fn icon(&mut self, pm: &mut Pixmap, base: Transform, s: f32, rel: &str, r: Rect, opacity: f32) {
        let px = (r.w * s as f64).round() as u32;
        let file = self.icons.join(rel);
        let key = (file.to_string_lossy().into_owned(), px);
        if let Some(img) = self
            .icon_cache
            .get_or(key, || decode_webp(&file).and_then(|p| scaled(&p, px)))
        {
            let t = base
                .pre_translate(r.x as f32, r.y as f32)
                .pre_scale(1.0 / s, 1.0 / s);
            let paint = PixmapPaint {
                opacity,
                quality: FilterQuality::Bilinear,
                ..PixmapPaint::default()
            };
            pm.draw_pixmap(0, 0, img.as_ref(), &paint, t, None);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn text_centered(
        &mut self,
        pm: &mut Pixmap,
        base: Transform,
        s: f32,
        r: Rect,
        text: &str,
        size: f32,
        bold: bool,
        color: Color,
    ) {
        let w = self.fonts.borrow_mut().width(text, size, bold) as f64;
        self.text(
            pm,
            base,
            s,
            r.x + (r.w - w) / 2.0,
            r.y + r.h / 2.0,
            text,
            size,
            bold,
            color,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        pm: &mut Pixmap,
        base: Transform,
        s: f32,
        x: f64,
        mid_y: f64,
        text: &str,
        size: f32,
        bold: bool,
        color: Color,
    ) {
        self.text_spaced(pm, base, s, x, mid_y, text, size, bold, 0.0, color);
    }

    /// Draws `text` with its vertical centre at `mid_y` (scene px), rasterized at device size.
    #[allow(clippy::too_many_arguments)]
    fn text_spaced(
        &mut self,
        pm: &mut Pixmap,
        base: Transform,
        s: f32,
        x: f64,
        mid_y: f64,
        text: &str,
        size: f32,
        bold: bool,
        spacing: f32,
        color: Color,
    ) {
        let dev = size * s;
        let mut fonts = self.fonts.borrow_mut();
        let (ascent, descent) = fonts.vmetrics(dev, bold);
        let (px, py) = map(base, x, mid_y);
        let mut pen = px;
        let baseline = (py + (ascent + descent) / 2.0).round();
        for c in text.chars() {
            let Some(g) = fonts.glyph(c, dev, bold) else {
                continue;
            };
            let gx = (pen + g.left as f32).round() as i32;
            let gy = (baseline + g.top as f32).round() as i32;
            blend_mask(pm, gx, gy, g.w, g.h, &g.coverage, color);
            pen += g.advance + spacing * s;
        }
    }
}

fn map(t: Transform, x: f64, y: f64) -> (f32, f32) {
    let mut p = [tiny_skia::Point::from_xy(x as f32, y as f32)];
    t.map_points(&mut p);
    (p[0].x, p[0].y)
}

fn with_alpha(c: Color, a: f64) -> Color {
    let mut c = c;
    c.apply_opacity(a as f32);
    c
}

/// Source-over of a coverage mask in a solid color (premultiplied RGBA target).
fn blend_mask(pm: &mut Pixmap, x0: i32, y0: i32, w: usize, h: usize, cov: &[u8], color: Color) {
    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
    let c = color.premultiply().to_color_u8();
    let data = pm.data_mut();
    for gy in 0..h as i32 {
        let y = y0 + gy;
        if y < 0 || y >= ph {
            continue;
        }
        for gx in 0..w as i32 {
            let x = x0 + gx;
            if x < 0 || x >= pw {
                continue;
            }
            let a = cov[(gy as usize) * w + gx as usize] as u32;
            if a == 0 {
                continue;
            }
            let i = ((y * pw + x) * 4) as usize;
            let src = [
                c.red() as u32 * a / 255,
                c.green() as u32 * a / 255,
                c.blue() as u32 * a / 255,
                c.alpha() as u32 * a / 255,
            ];
            let inv = 255 - src[3];
            for k in 0..4 {
                data[i + k] = (src[k] + data[i + k] as u32 * inv / 255).min(255) as u8;
            }
        }
    }
}

fn elide(f: &mut Fonts, text: &str, size: f32, max: f32) -> String {
    if f.width(text, size, false) <= max {
        return text.to_string();
    }
    let mut out = String::new();
    for c in text.chars() {
        let next = format!("{out}{c}…");
        if f.width(&next, size, false) > max {
            break;
        }
        out.push(c);
    }
    format!("{out}…")
}

fn rounded_path(r: Rect, radius: f64) -> Option<tiny_skia::Path> {
    let (x, y, w, h) = (r.x as f32, r.y as f32, r.w as f32, r.h as f32);
    let rr = (radius as f32).min(w / 2.0).min(h / 2.0).max(0.0);
    let k = 0.552_284_8 * rr;
    let mut pb = PathBuilder::new();
    pb.move_to(x + rr, y);
    pb.line_to(x + w - rr, y);
    pb.cubic_to(x + w - rr + k, y, x + w, y + rr - k, x + w, y + rr);
    pb.line_to(x + w, y + h - rr);
    pb.cubic_to(
        x + w,
        y + h - rr + k,
        x + w - rr + k,
        y + h,
        x + w - rr,
        y + h,
    );
    pb.line_to(x + rr, y + h);
    pb.cubic_to(x + rr - k, y + h, x, y + h - rr + k, x, y + h - rr);
    pb.line_to(x, y + rr);
    pb.cubic_to(x, y + rr - k, x + rr - k, y, x + rr, y);
    pb.close();
    pb.finish()
}

fn paint(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c);
    p.anti_alias = true;
    p
}

fn rounded(
    pm: &mut Pixmap,
    t: Transform,
    r: Rect,
    radius: f64,
    fill: Color,
    border: Option<Color>,
) {
    let Some(path) = rounded_path(r, radius) else {
        return;
    };
    pm.fill_path(&path, &paint(fill), FillRule::Winding, t, None);
    if let Some(b) = border {
        let inset = Rect::new(r.x + 0.5, r.y + 0.5, r.w - 1.0, r.h - 1.0);
        if let Some(p) = rounded_path(inset, (radius - 0.5).max(0.0)) {
            pm.stroke_path(
                &p,
                &paint(b),
                &Stroke {
                    width: 1.0,
                    ..Stroke::default()
                },
                t,
                None,
            );
        }
    }
}

fn circle(pm: &mut Pixmap, t: Transform, r: Rect, fill: Color, border: Option<(Color, f32)>) {
    let Some(path) = PathBuilder::from_circle(
        (r.x + r.w / 2.0) as f32,
        (r.y + r.h / 2.0) as f32,
        (r.w / 2.0) as f32,
    ) else {
        return;
    };
    pm.fill_path(&path, &paint(fill), FillRule::Winding, t, None);
    if let Some((c, w)) = border {
        if let Some(p) = PathBuilder::from_circle(
            (r.x + r.w / 2.0) as f32,
            (r.y + r.h / 2.0) as f32,
            (r.w / 2.0) as f32 - w / 2.0,
        ) {
            pm.stroke_path(
                &p,
                &paint(c),
                &Stroke {
                    width: w,
                    ..Stroke::default()
                },
                t,
                None,
            );
        }
    }
}

/// The HIDE pill's glyph: an almond eye, a pupil and a slash (16×12 px at `x`, `y`).
fn eye_slash(pm: &mut Pixmap, t: Transform, x: f64, y: f64, color: Color) {
    let (x, y) = (x as f32, y as f32);
    let stroke = Stroke {
        width: 1.6,
        line_cap: tiny_skia::LineCap::Round,
        ..Stroke::default()
    };
    let mut pb = PathBuilder::new();
    pb.move_to(x + 1.0, y + 6.0);
    pb.quad_to(x + 8.0, y - 1.5, x + 15.0, y + 6.0);
    pb.quad_to(x + 8.0, y + 13.5, x + 1.0, y + 6.0);
    pb.close();
    if let Some(p) = pb.finish() {
        pm.stroke_path(&p, &paint(color), &stroke, t, None);
    }
    if let Some(p) = PathBuilder::from_circle(x + 8.0, y + 6.0, 2.2) {
        pm.fill_path(&p, &paint(color), FillRule::Winding, t, None);
    }
    let mut pb = PathBuilder::new();
    pb.move_to(x + 2.5, y + 11.0);
    pb.line_to(x + 13.5, y + 1.0);
    if let Some(p) = pb.finish() {
        pm.stroke_path(&p, &paint(color), &stroke, t, None);
    }
}

/// Copies a premultiplied RGBA pixmap into a little-endian ARGB8888 buffer (B, G, R, A bytes).
pub fn to_argb8888(pm: &Pixmap, out: &mut [u8]) {
    for (src, dst) in pm.data().chunks_exact(4).zip(out.chunks_exact_mut(4)) {
        dst[0] = src[2];
        dst[1] = src[1];
        dst[2] = src[0];
        dst[3] = src[3];
    }
}
