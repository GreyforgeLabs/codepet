//! The desktop pet scene: a time-explicit port of qml/shared/DesktopPet.qml.
//!
//! Presentation only. It draws the parsed bar-state.json it is given and never talks to
//! CodePet itself: clicks become [`Effect`]s that the host turns into requests. All time is
//! passed in (`now`, ms since the epoch), so the scene is deterministic under test, and
//! [`Scene::next_deadline`] tells the host when anything can change next: while the pet
//! rests the process sleeps until then instead of ticking.

use crate::model::catalog::{self, Species, StateMeta};
use crate::model::face::{self, Celebrate, Face, Seen};
use crate::model::work;
use serde_json::Value;
use std::path::PathBuf;

pub const PET_MAX: f64 = 112.0;
const HOP_MS: u64 = 400;
const WIGGLE_MS: u64 = 730;
const HEART_MS: i64 = 1600;
const FEEDBACK_MS: u64 = 6000;
const HIDE_LINGER_MS: u64 = 1500;
pub const WHEEL_W: f64 = 252.0;
pub const WHEEL_H: f64 = 304.0;
pub const PILL_H: f64 = 26.0;

/// What a click or key asks the host to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    ToggleCard,
    Care(String),
    SavePlacement { x: f64, y: f64, pinned: bool },
    Hide,
}

/// Text measurement supplied by the renderer (width in px of `text` at `size` px).
pub trait Measure {
    fn width(&self, text: &str, size: f32) -> f32;
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn contains(&self, px: f64, py: f64) -> bool {
        self.w > 0.0
            && self.h > 0.0
            && px >= self.x
            && py >= self.y
            && px < self.x + self.w
            && py < self.y + self.h
    }
    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }
    pub fn union(&self, o: &Rect) -> Rect {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Rect::new(
            x,
            y,
            (self.x + self.w).max(o.x + o.w) - x,
            (self.y + self.h).max(o.y + o.h) - y,
        )
    }
}

/// One care-wheel control as drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct WheelButton {
    pub action: &'static str,
    pub label: String,
    /// An icon under assets/icons, or "" for a vector glyph drawn by the renderer ("pin").
    pub icon: String,
    pub rect: Rect,
    pub enabled: bool,
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
}

/// Everything the renderer draws, in scene coordinates.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    pub active: bool,
    /// Frame path relative to the species dir, mirrored, top-left, size, rotation (deg), opacity.
    pub sprite: Option<(String, String, bool, Rect, f64, f64)>,
    pub pinned_tag: Option<Rect>,
    pub bubble: Option<(Rect, Vec<String>, String)>,
    pub feedback: Option<(Rect, String)>,
    pub badge: Option<Rect>,
    pub hide_pill: Option<(Rect, bool, bool)>,
    pub wheel: Option<(Rect, Vec<WheelButton>, Rect, bool, String)>,
    pub pet_hit: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    None,
    Pet,
    Pill,
    Wheel(usize),
    WheelClose,
    WheelPanel,
}

pub struct Scene {
    pets_dir: PathBuf,
    pub width: f64,
    pub height: f64,
    pub bar_state: Option<Value>,
    pub work_state: Option<Value>,
    pub card_open: bool,
    pub surface_active: bool,
    pub reduced_motion: bool,
    pub now: u64,
    rng: u64,
    species: Option<Species>,
    pet_id: String,
    stage: f64,
    life: Option<Value>,
    pub face: Face,
    seen: Seen,
    grew_seq: f64,
    seeded: bool,
    played_celebration: String,
    celebration_bubble: Option<Value>,
    initialized: bool,
    was_active: bool,
    pub pet_x: f64,
    pub pet_y: f64,
    pub pinned: bool,
    pub dragging: bool,
    drag_moved: bool,
    press_point: (f64, f64),
    press_position: (f64, f64),
    facing_left: bool,
    destination: f64,
    idle_ticks: i64,
    walking: bool,
    swim_ticks_left: i64,
    resting: bool,
    base_elapsed_ms: u64,
    transient: Option<(StateMeta, u64, u64)>,
    queued: Option<(String, u64)>,
    cosmetic_expression: bool,
    sleep_observed: bool,
    last_sleeping: bool,
    heart_ms_left: i64,
    hop_start: Option<u64>,
    wiggle_start: Option<u64>,
    pub click_count: u64,
    pub wheel_open: bool,
    wheel_focus: usize,
    care_busy: bool,
    care_resuming: bool,
    pub care_message: String,
    feedback_until: Option<u64>,
    hover: Target,
    pressed: Target,
    hide_linger_until: Option<u64>,
    timer_interval: u64,
    tick_due: Option<u64>,
    tick_interval_set: u64,
    wake_due: Option<u64>,
    effects: Vec<Effect>,
    pill_w: f64,
}

impl Scene {
    pub fn new(pets_dir: PathBuf, seed: u64) -> Scene {
        Scene {
            pets_dir,
            width: 0.0,
            height: 0.0,
            bar_state: None,
            work_state: None,
            card_open: false,
            surface_active: true,
            reduced_motion: false,
            now: 0,
            rng: seed | 1,
            species: None,
            pet_id: String::new(),
            stage: 1.0,
            life: None,
            face: Face::default(),
            seen: Seen::default(),
            grew_seq: -1.0,
            seeded: false,
            played_celebration: String::new(),
            celebration_bubble: None,
            initialized: false,
            was_active: false,
            pet_x: 40.0,
            pet_y: -1.0,
            pinned: false,
            dragging: false,
            drag_moved: false,
            press_point: (0.0, 0.0),
            press_position: (0.0, 0.0),
            facing_left: false,
            destination: 40.0,
            idle_ticks: 60,
            walking: false,
            swim_ticks_left: 0,
            resting: false,
            base_elapsed_ms: 0,
            transient: None,
            queued: None,
            cosmetic_expression: false,
            sleep_observed: false,
            last_sleeping: false,
            heart_ms_left: 0,
            hop_start: None,
            wiggle_start: None,
            click_count: 0,
            wheel_open: false,
            wheel_focus: 0,
            care_busy: false,
            care_resuming: false,
            care_message: String::new(),
            feedback_until: None,
            hover: Target::None,
            pressed: Target::None,
            hide_linger_until: None,
            timer_interval: 200,
            tick_due: None,
            tick_interval_set: 0,
            wake_due: None,
            effects: Vec::new(),
            pill_w: 64.0,
        }
    }

    pub fn set_measure(&mut self, m: &dyn Measure) {
        self.pill_w = (m.width("HIDE", 11.0) as f64 + 16.0 + 21.0).ceil();
    }

    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    fn random(&mut self) -> f64 {
        // xorshift64*: cosmetic randomness only.
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    // ---- derived values (QML bindings) ----

    pub fn pet_size(&self) -> f64 {
        PET_MAX.min(self.width).min(self.height).max(0.0)
    }
    fn floor_y(&self) -> f64 {
        (self.height - self.pet_size() - 12.0).max(0.0)
    }
    fn shown(&self) -> bool {
        self.face.overlay != "none" && self.species.is_some()
    }
    pub fn active(&self) -> bool {
        self.surface_active && self.width > 0.0 && self.height > 0.0 && self.shown()
    }
    fn anim(&self) -> (&'static str, bool, bool) {
        face::animation(&self.face.overlay)
    }
    fn sleeping(&self) -> bool {
        self.anim().0 == "sleep"
    }
    fn care_sleeping(&self) -> bool {
        self.bar_state
            .as_ref()
            .and_then(|b| b.get("care"))
            .and_then(|c| c.get("sleeping"))
            == Some(&Value::Bool(true))
    }
    fn life_flag(&self, key: &str) -> bool {
        self.life.as_ref().and_then(|l| l.get(key)) == Some(&Value::Bool(true))
    }
    fn roams_species(&self) -> bool {
        self.species.as_ref().is_some_and(Species::roams)
    }
    fn can_roam(&self) -> bool {
        self.anim().1 && !self.pinned && !self.dragging && !self.card_open && !self.wheel_open
    }
    fn meta(&self, state: &str) -> StateMeta {
        match &self.species {
            Some(s) => s.state_meta(state),
            None => Species::default().state_meta(state),
        }
    }
    fn base_state(&self) -> String {
        if self.walking && self.can_roam() {
            return "locomotion".into();
        }
        let a = self.anim().0;
        if a.is_empty() {
            "idle".into()
        } else {
            a.into()
        }
    }
    fn display(&self) -> (StateMeta, bool) {
        let base = self.meta(&self.base_state());
        match &self.transient {
            Some((t, _, _)) if catalog::display_transient(&base, Some(t)) => (t.clone(), true),
            _ => (base, false),
        }
    }
    fn frame(&self) -> i64 {
        let (meta, is_transient) = self.display();
        let elapsed = if is_transient {
            self.transient.as_ref().map(|t| t.1).unwrap_or(0)
        } else {
            self.base_elapsed_ms
        };
        catalog::frame_at(&meta, elapsed / 200, self.reduced_motion)
    }
    fn bob_offset(&self) -> f64 {
        let (meta, _) = self.display();
        if self.anim().2 && !self.reduced_motion && meta.name == "waiting" && self.frame() % 2 == 1
        {
            -3.0
        } else {
            0.0
        }
    }
    fn motion_running(&self) -> bool {
        self.active() && !self.reduced_motion && !self.sleeping()
    }
    fn scheduler_running(&self) -> bool {
        self.active()
            && (self.transient.is_some() || self.motion_running() || self.heart_ms_left > 0)
    }
    fn hop_offset(&self) -> f64 {
        let Some(start) = self.hop_start else {
            return 0.0;
        };
        let t = self.now.saturating_sub(start);
        if t >= HOP_MS {
            return 0.0;
        }
        if t < 150 {
            let p = t as f64 / 150.0;
            -18.0 * (1.0 - (1.0 - p) * (1.0 - p))
        } else {
            let p = (t - 150) as f64 / 250.0;
            -18.0 * (1.0 - out_bounce(p))
        }
    }
    fn wiggle_angle(&self) -> f64 {
        let Some(start) = self.wiggle_start else {
            return 0.0;
        };
        let t = self.now.saturating_sub(start) as f64;
        let keys = [
            (0.0, 0.0),
            (150.0, -6.0),
            (370.0, 6.0),
            (550.0, -3.0),
            (730.0, 0.0),
        ];
        for w in keys.windows(2) {
            let ((t0, a0), (t1, a1)) = (w[0], w[1]);
            if t <= t1 {
                let p = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
                let e = 0.5 - 0.5 * (std::f64::consts::PI * p).cos();
                return a0 + (a1 - a0) * e;
            }
        }
        0.0
    }
    fn pet_top(&self) -> f64 {
        (if self.pet_y < 0.0 {
            self.floor_y()
        } else {
            self.pet_y
        }) + self.hop_offset()
            + self.bob_offset()
    }
    fn mirrored(&self) -> bool {
        let (meta, _) = self.display();
        self.facing_left && meta.mirror
    }
    fn hit_box(&self) -> [f64; 4] {
        match &self.species {
            Some(s) => s.hit_rect(self.pet_size(), self.mirrored(), self.stage),
            None => [0.0, 0.0, self.pet_size(), self.pet_size()],
        }
    }
    pub fn pet_hit(&self) -> Rect {
        if !self.active() {
            return Rect::default();
        }
        let h = self.hit_box();
        Rect::new(self.pet_x + h[0], self.pet_top() + h[1], h[2], h[3])
    }
    fn hide_offered(&self) -> bool {
        self.active()
            && !self.dragging
            && !self.card_open
            && !self.wheel_open
            && (self.hover == Target::Pet
                || self.hover == Target::Pill
                || self.hide_linger_until.is_some_and(|u| self.now < u))
    }
    pub fn pill_rect(&self) -> Rect {
        if !self.hide_offered() {
            return Rect::default();
        }
        let h = self.hit_box();
        let w = self.pill_w.min(self.width);
        let x = (self.pet_x + h[0] - 6.0).min(self.width - w).max(0.0);
        let y = (self.pet_top() + h[1] - PILL_H / 2.0)
            .min(self.height - PILL_H)
            .max(0.0);
        Rect::new(x, y, w, PILL_H)
    }
    pub fn wheel_rect(&self) -> Rect {
        if !(self.active() && self.wheel_open) {
            return Rect::default();
        }
        let w = WHEEL_W.min(self.width);
        let h = WHEEL_H.min(self.height);
        let top = self.pet_top();
        let size = self.pet_size();
        let x = (self.pet_x + size / 2.0 - w / 2.0)
            .min(self.width - w)
            .max(0.0);
        let y = (if top >= h + 8.0 {
            top - h - 8.0
        } else {
            top + size + 8.0
        })
        .min(self.height - h)
        .max(0.0);
        Rect::new(x, y, w, h)
    }

    fn wheel_buttons(&self) -> Vec<WheelButton> {
        let r = self.wheel_rect();
        let scale = (r.w / WHEEL_W).min(r.h / WHEEL_H);
        let ox = r.x + (r.w - WHEEL_W * scale) / 2.0;
        let oy = r.y + (r.h - WHEEL_H * scale) / 2.0;
        let set = self
            .species
            .as_ref()
            .map(|s| s.care_set().to_string())
            .unwrap_or_else(|| "floor".into());
        let (sleeping, paused, fainted) = (
            self.care_sleeping(),
            self.life_flag("paused"),
            self.life_flag("fainted"),
        );
        let defs: [(&'static str, String, String, f64, f64); 7] = [
            ("pet", "Pet".into(), "care-floor/hand".into(), 94.0, 8.0),
            (
                "food",
                "Snack".into(),
                format!("care-{set}/food"),
                176.0,
                68.0,
            ),
            (
                "water",
                "Drink".into(),
                format!("care-{set}/water"),
                176.0,
                154.0,
            ),
            (
                "wash",
                "Bath".into(),
                format!("care-{set}/wash"),
                132.0,
                234.0,
            ),
            (
                "play",
                "Play".into(),
                format!("care-{set}/play"),
                56.0,
                234.0,
            ),
            (
                "sleep",
                if paused {
                    "Resume"
                } else if sleeping {
                    "Wake"
                } else {
                    "Nap"
                }
                .into(),
                if sleeping || paused {
                    "care-floor/sun_wake"
                } else {
                    "care-floor/bed"
                }
                .into(),
                12.0,
                154.0,
            ),
            (
                "pin",
                if self.pinned { "Roam" } else { "Pin" }.into(),
                "care-floor/pin".into(),
                12.0,
                68.0,
            ),
        ];
        defs.into_iter()
            .enumerate()
            .map(|(i, (action, label, icon, x, y))| WheelButton {
                action,
                label,
                icon,
                rect: Rect::new(ox + x * scale, oy + y * scale, 64.0 * scale, 62.0 * scale),
                enabled: action != "sleep" || !fainted || sleeping || paused,
                hovered: self.hover == Target::Wheel(i),
                pressed: self.pressed == Target::Wheel(i),
                focused: self.wheel_focus == i,
            })
            .collect()
    }
    fn wheel_close(&self) -> Rect {
        let r = self.wheel_rect();
        let scale = (r.w / WHEEL_W).min(r.h / WHEEL_H);
        let ox = r.x + (r.w - WHEEL_W * scale) / 2.0;
        let oy = r.y + (r.h - WHEEL_H * scale) / 2.0;
        Rect::new(
            ox + 102.0 * scale,
            oy + 102.0 * scale,
            48.0 * scale,
            40.0 * scale,
        )
    }
    fn wheel_message(&self) -> String {
        if !self.care_message.is_empty() {
            return self.care_message.clone();
        }
        if self.life_flag("fainted") {
            "Snack + Drink\nto recover".into()
        } else if self.life_flag("paused") {
            "Paused".into()
        } else if self.care_sleeping() {
            "Sleeping".into()
        } else if self.pinned {
            "Pinned".into()
        } else {
            "Roaming".into()
        }
    }

    /// The bubble's picture icons: the heart after a click, a celebration, else the face's.
    fn bubble_icons(&self) -> Vec<String> {
        if !self.active() {
            return Vec::new();
        }
        let set = self
            .species
            .as_ref()
            .map(|s| s.care_set())
            .unwrap_or("floor");
        if self.heart_ms_left > 0 {
            return vec!["care-floor/heart.webp".into()];
        }
        let celebrating = self
            .transient
            .as_ref()
            .is_some_and(|t| t.0.name == "celebrate");
        if celebrating && self.celebration_bubble.is_some() {
            return face::bubble_icons(self.celebration_bubble.as_ref(), set, false);
        }
        face::bubble_icons(
            self.face.bubble.as_ref(),
            set,
            self.face.state() == "waiting",
        )
    }
    fn observer_label(&self) -> String {
        let celebrating = self
            .transient
            .as_ref()
            .is_some_and(|t| t.0.name == "celebrate");
        if self.heart_ms_left > 0 || celebrating {
            return String::new();
        }
        face::observer_label(&self.face)
    }

    /// The full drawing for the current moment.
    pub fn view(&self, m: &dyn Measure) -> View {
        let mut v = View {
            active: self.active(),
            ..View::default()
        };
        if !v.active {
            return v;
        }
        let size = self.pet_size();
        let top = self.pet_top();
        let (meta, is_t) = self.display();
        let frame = if self.cosmetic_expression && is_t && meta.name == "celebrate" {
            1
        } else {
            self.frame()
        };
        if let Some(s) = &self.species {
            let (path, may_mirror) = s.resolve(self.stage, &meta.name, frame, self.reduced_motion);
            let opacity = if self.face.state() == "paused" {
                0.85
            } else {
                1.0
            };
            v.sprite = Some((
                s.id.clone(),
                path,
                self.facing_left && may_mirror,
                Rect::new(self.pet_x, top, size, size),
                self.wiggle_angle(),
                opacity,
            ));
        }
        v.pet_hit = self.pet_hit();
        let hb = self.hit_box();
        if self.pinned {
            let w = (m.width("Pinned", 12.0) as f64 + 14.0).ceil();
            let x = (self.pet_x + (size - w) / 2.0).min(self.width - w).max(0.0);
            let y = (top + size - 6.0).min(self.height - 22.0);
            v.pinned_tag = Some(Rect::new(x, y, w, 22.0));
        }
        let feedback_visible = !self.card_open && !self.wheel_open && !self.care_message.is_empty();
        let icons = self.bubble_icons();
        if !self.card_open && !feedback_visible && !icons.is_empty() {
            let label = self.observer_label();
            let icon = if label.is_empty() { 40.0 } else { 24.0 };
            let icons_w = icons.len() as f64 * icon + (icons.len().saturating_sub(1)) as f64 * 4.0;
            let label_w = if label.is_empty() {
                0.0
            } else {
                (m.width(&label, 12.0) as f64)
                    .min((260.0f64.min(self.width) - icons_w - 22.0).max(0.0))
                    + 6.0
            };
            let w = (icons_w + label_w + 16.0).min(self.width);
            let h = icon + 12.0;
            let x = (self.pet_x + hb[0] + hb[2] - w / 3.0)
                .min(self.width - w)
                .max(0.0);
            let y = if !label.is_empty() {
                if top >= h + 6.0 {
                    top - h - 6.0
                } else {
                    (top + size + 6.0).min(self.height - h)
                }
            } else {
                (top + hb[1] - h - 6.0).max(0.0)
            };
            v.bubble = Some((Rect::new(x, y.max(0.0), w, h), icons, label));
        }
        if feedback_visible {
            let w = self.width.min(230.0);
            let lines = wrap(m, &self.care_message, 12.0, (w - 20.0) as f32)
                .len()
                .max(1) as f64;
            let h = lines * 16.0 + 16.0;
            let x = (self.pet_x + (size - w) / 2.0).min(self.width - w).max(0.0);
            let y = (if top >= h + 8.0 {
                top - h - 8.0
            } else {
                top + size + 8.0
            })
            .min(self.height - h)
            .max(0.0);
            v.feedback = Some((Rect::new(x, y, w, h), self.care_message.clone()));
        }
        if !self.card_open && work::badge(self.work_state.as_ref()) {
            let x = (self.pet_x + hb[0] + hb[2] - 18.0)
                .min(self.width - 18.0)
                .max(0.0);
            v.badge = Some(Rect::new(x, (top + hb[1]).max(0.0), 18.0, 18.0));
        }
        let pill = self.pill_rect();
        if !pill.is_empty() {
            v.hide_pill = Some((
                pill,
                self.hover == Target::Pill,
                self.pressed == Target::Pill,
            ));
        }
        if self.wheel_open {
            v.wheel = Some((
                self.wheel_rect(),
                self.wheel_buttons(),
                self.wheel_close(),
                self.hover == Target::WheelClose,
                self.wheel_message(),
            ));
        }
        v
    }

    /// The input islands: pet, HIDE pill and wheel (decorations stay click-through).
    pub fn input_rects(&self) -> Vec<Rect> {
        [self.pet_hit(), self.pill_rect(), self.wheel_rect()]
            .into_iter()
            .filter(|r| !r.is_empty())
            .collect()
    }

    // ---- inputs from the host ----

    pub fn set_size(&mut self, w: f64, h: f64, now: u64) {
        self.now = now;
        self.width = w;
        self.height = h;
        if self.initialized {
            self.clamp_position();
        }
        self.after(now);
    }

    pub fn set_bar_state(&mut self, doc: Option<Value>, now: u64) {
        self.bar_state = doc;
        self.refresh_clock(now);
    }

    pub fn set_work_state(&mut self, doc: Option<Value>, now: u64) {
        self.work_state = doc;
        self.refresh_clock(now);
    }

    pub fn set_surface_active(&mut self, active: bool, now: u64) {
        self.surface_active = active;
        self.now = now;
        self.after(now);
    }

    pub fn set_reduced_motion(&mut self, on: bool, now: u64) {
        if self.reduced_motion == on {
            return;
        }
        self.reduced_motion = on;
        self.now = now;
        if on {
            self.stop_moving();
            self.hop_start = None;
            self.wiggle_start = None;
        }
        self.update_interval();
        self.after(now);
    }

    pub fn start(&mut self, now: u64) {
        self.initialized = true;
        self.clamp_position();
        self.refresh_clock(now);
    }

    /// The host calls this whenever `next_deadline` has passed.
    pub fn advance(&mut self, now: u64) {
        self.now = now;
        if self.hop_start.is_some_and(|s| now >= s + HOP_MS) {
            self.hop_start = None;
        }
        if self.wiggle_start.is_some_and(|s| now >= s + WIGGLE_MS) {
            self.wiggle_start = None;
        }
        if self.feedback_until.is_some_and(|u| now >= u) {
            self.feedback_until = None;
            self.care_message.clear();
        }
        if self.hide_linger_until.is_some_and(|u| now >= u) {
            self.hide_linger_until = None;
        }
        if self.wake_due.is_some_and(|w| now >= w) {
            self.wake_due = None;
            self.refresh_clock(now);
        }
        if self.tick_due.is_some_and(|t| now >= t) {
            self.tick_due = None;
            self.cosmetic_tick();
            // A Qt repeating timer re-arms at the interval in force after the tick.
            if self.scheduler_running() {
                self.tick_due = Some(now + self.timer_interval);
                self.tick_interval_set = self.timer_interval;
            }
        }
        self.after(now);
    }

    /// When the scene can next change on its own (None: only an input can change it).
    pub fn next_deadline(&self) -> Option<u64> {
        let mut d: Vec<u64> = [
            self.tick_due,
            self.wake_due,
            self.feedback_until,
            self.hide_linger_until,
        ]
        .into_iter()
        .flatten()
        .collect();
        if self.hop_start.is_some() || self.wiggle_start.is_some() {
            d.push(self.now + 16);
        }
        d.into_iter().min()
    }

    fn target_at(&self, x: f64, y: f64) -> Target {
        if self.wheel_open {
            if self.wheel_close().contains(x, y) {
                return Target::WheelClose;
            }
            for (i, b) in self.wheel_buttons().iter().enumerate() {
                if b.rect.contains(x, y) {
                    return Target::Wheel(i);
                }
            }
            if self.wheel_rect().contains(x, y) {
                return Target::WheelPanel;
            }
        }
        if self.pill_rect().contains(x, y) {
            return Target::Pill;
        }
        if self.pet_hit().contains(x, y) {
            return Target::Pet;
        }
        Target::None
    }

    pub fn pointer_motion(&mut self, x: f64, y: f64, now: u64) {
        self.now = now;
        if self.dragging {
            self.move_drag(x, y);
            self.after(now);
            return;
        }
        let t = self.target_at(x, y);
        if t != self.hover {
            let was = self.hover;
            self.hover = t;
            if (was == Target::Pet || was == Target::Pill)
                && t != Target::Pet
                && t != Target::Pill
                && self.active()
            {
                self.hide_linger_until = Some(now + HIDE_LINGER_MS);
            }
        }
        self.after(now);
    }

    pub fn pointer_leave(&mut self, now: u64) {
        self.now = now;
        if (self.hover == Target::Pet || self.hover == Target::Pill) && self.active() {
            self.hide_linger_until = Some(now + HIDE_LINGER_MS);
        }
        self.hover = Target::None;
        self.after(now);
    }

    /// Left (true) or right (false) button press.
    pub fn pointer_press(&mut self, x: f64, y: f64, left: bool, now: u64) {
        self.now = now;
        let t = self.target_at(x, y);
        self.pressed = t;
        if left && t == Target::Pet {
            self.begin_drag(x, y);
        }
        self.after(now);
    }

    pub fn pointer_release(&mut self, x: f64, y: f64, left: bool, now: u64) {
        self.now = now;
        let pressed = std::mem::replace(&mut self.pressed, Target::None);
        if left && self.dragging {
            let moved = self.drag_moved;
            self.end_drag();
            if !moved && self.target_at(x, y) == Target::Pet {
                self.pet_clicked();
            }
            self.after(now);
            return;
        }
        let t = self.target_at(x, y);
        if t != pressed {
            self.after(now);
            return;
        }
        match t {
            Target::Pet if !left => self.toggle_wheel(),
            Target::Pill if left => self.request_hide(),
            Target::Wheel(i) if left => {
                let b = &self.wheel_buttons()[i];
                if b.enabled {
                    let a = b.action;
                    self.wheel_focus = i;
                    self.choose_care(a);
                }
            }
            Target::WheelClose if left => self.wheel_open = false,
            _ => {}
        }
        self.after(now);
    }

    /// Keyboard while the wheel holds focus: Tab / Shift+Tab, Enter or Space, Escape.
    pub fn key(&mut self, key: Key, now: u64) {
        self.now = now;
        if !self.wheel_open {
            return;
        }
        let count = 8; // seven care buttons plus close
        match key {
            Key::Escape => self.wheel_open = false,
            Key::Next => self.wheel_focus = (self.wheel_focus + 1) % count,
            Key::Prev => self.wheel_focus = (self.wheel_focus + count - 1) % count,
            Key::Activate => {
                if self.wheel_focus == 7 {
                    self.wheel_open = false;
                } else {
                    let b = &self.wheel_buttons()[self.wheel_focus];
                    if b.enabled {
                        let a = b.action;
                        self.choose_care(a);
                    }
                }
            }
        }
        self.after(now);
    }

    pub fn wants_keyboard(&self) -> bool {
        self.active() && self.wheel_open
    }

    /// The host's answer to an `Effect::Care` request.
    pub fn care_answered(
        &mut self,
        action: &str,
        ok: bool,
        changed: bool,
        message: Option<String>,
        now: u64,
    ) {
        self.now = now;
        self.care_busy = false;
        let resumed = std::mem::replace(&mut self.care_resuming, false);
        if !self.active() {
            self.set_message(String::new());
            self.after(now);
            return;
        }
        if !ok {
            let msg = message
                .map(|m| m.chars().take(180).collect())
                .unwrap_or_else(|| "Try again from Care".into());
            self.set_message(msg);
            self.after(now);
            return;
        }
        if ["food", "water", "play", "wash"].contains(&action) {
            let msg = if changed {
                match action {
                    "food" => "Yum!",
                    "water" => "Refreshing!",
                    "play" => "Wheee!",
                    _ => "All clean!",
                }
            } else {
                "Already feeling good"
            };
            self.set_message(msg.into());
            if changed || action == "play" {
                if self.transient.as_ref().is_some_and(|t| t.0.name == action) {
                    self.clear_transients();
                }
                self.start_transient(action, 0);
            }
        } else {
            self.set_message(
                if action == "wake" {
                    if resumed {
                        "Resumed"
                    } else {
                        "Awake"
                    }
                } else {
                    "Nap time"
                }
                .into(),
            );
        }
        self.after(now);
    }

    // ---- behavior (DesktopPet.qml functions) ----

    fn set_message(&mut self, msg: String) {
        self.care_message = msg;
        self.feedback_until = if self.care_message.is_empty() {
            None
        } else {
            Some(self.now + FEEDBACK_MS)
        };
    }

    fn save_placement(&mut self) {
        let size = self.pet_size();
        let y = if self.pet_y < 0.0 {
            self.floor_y()
        } else {
            self.pet_y
        };
        self.effects.push(Effect::SavePlacement {
            x: self.pet_x / (self.width - size).max(1.0),
            y: y / (self.height - size).max(1.0),
            pinned: self.pinned,
        });
    }

    /// A saved placement (fractions of the free area) and pin state.
    pub fn restore_placement(&mut self, x: Option<f64>, y: Option<f64>, pinned: bool, now: u64) {
        self.now = now;
        if self.dragging {
            return;
        }
        self.pinned = pinned;
        if let (Some(x), Some(y)) = (x, y) {
            let size = self.pet_size();
            self.pet_x = x * (self.width - size).max(0.0);
            self.pet_y = y * (self.height - size).max(0.0);
            self.stop_moving();
            self.clamp_position();
        }
        self.after(now);
    }

    fn toggle_pinned(&mut self) {
        self.pinned = !self.pinned;
        self.stop_moving();
        self.idle_ticks = 60;
        self.save_placement();
    }

    fn toggle_wheel(&mut self) {
        if !self.active() {
            return;
        }
        self.wheel_open = !self.wheel_open;
        self.wheel_focus = 0;
        if self.wheel_open && !self.care_busy {
            self.set_message(String::new());
        }
        if self.wheel_open {
            self.stop_moving();
        }
    }

    fn express(&mut self, petted: bool) {
        if !self.active() || self.sleeping() {
            return;
        }
        self.clear_transients();
        self.start_transient("celebrate", 1600);
        self.cosmetic_expression = true;
        if petted {
            self.heart_ms_left = HEART_MS;
        }
        if !self.reduced_motion && self.roams_species() {
            self.wiggle_start = Some(self.now);
            if petted {
                self.hop_start = Some(self.now);
            }
        }
        self.update_interval();
    }

    fn choose_care(&mut self, action: &str) {
        if !self.active() {
            return;
        }
        self.wheel_open = false;
        if action == "pin" {
            self.toggle_pinned();
            self.set_message(String::new());
            return;
        }
        if action == "pet" {
            // Petting is cosmetic: a happy reaction, no request (DesktopPet.qml).
            self.express(true);
            return;
        }
        if self.care_busy {
            self.set_message("One moment…".into());
            return;
        }
        let sleeping = self.care_sleeping();
        let paused = self.life_flag("paused");
        if action == "sleep" && self.life_flag("fainted") && !sleeping && !paused {
            self.set_message("Give Snack and Drink to recover".into());
            return;
        }
        self.care_busy = true;
        self.care_resuming = action == "sleep" && paused;
        self.set_message("One moment…".into());
        let sent = if action == "sleep" && (sleeping || paused) {
            "wake"
        } else {
            action
        };
        self.effects.push(Effect::Care(sent.into()));
    }

    fn request_hide(&mut self) {
        if !self.active() {
            return;
        }
        self.hide_linger_until = None;
        self.hover = Target::None;
        self.effects.push(Effect::Hide);
    }

    fn begin_drag(&mut self, x: f64, y: f64) {
        self.wheel_open = false;
        self.dragging = true;
        self.drag_moved = false;
        self.stop_moving();
        self.hop_start = None;
        self.press_point = (x, y);
        self.press_position = (
            self.pet_x,
            if self.pet_y < 0.0 {
                self.floor_y()
            } else {
                self.pet_y
            },
        );
    }

    fn move_drag(&mut self, x: f64, y: f64) {
        if !self.dragging {
            return;
        }
        let (dx, dy) = (x - self.press_point.0, y - self.press_point.1);
        if !self.drag_moved && dx * dx + dy * dy < 64.0 {
            return;
        }
        self.drag_moved = true;
        self.pet_x = self.press_position.0 + dx;
        self.pet_y = (self.press_position.1 + dy).max(0.0);
        self.clamp_position();
    }

    fn end_drag(&mut self) {
        self.dragging = false;
        self.idle_ticks = 60;
        if self.drag_moved {
            self.save_placement();
        }
    }

    fn pet_clicked(&mut self) {
        if !self.active() {
            return;
        }
        self.wheel_open = false;
        self.click_count += 1;
        self.stop_moving();
        self.resting = false;
        self.idle_ticks = 60;
        self.heart_ms_left = HEART_MS;
        if !self.reduced_motion && self.roams_species() {
            self.hop_start = Some(self.now);
        }
        self.update_interval();
        self.effects.push(Effect::ToggleCard);
    }

    pub fn refresh_clock(&mut self, now: u64) {
        self.now = now;
        self.update_face();
        self.after(now);
    }

    fn update_face(&mut self) {
        if !self.initialized {
            return;
        }
        let bar = self.bar_state.clone().unwrap_or(Value::Null);
        if !self.seeded && self.bar_state.is_some() {
            let (seen, grew) = face::seen_at_start(&bar);
            self.seen = seen;
            self.grew_seq = grew;
            self.seeded = true;
        }
        let life = bar.get("life").filter(|l| l.is_object()).cloned();
        let pet_id = life
            .as_ref()
            .and_then(|l| l.get("appearance"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        self.stage = life
            .as_ref()
            .and_then(|l| l.get("stage"))
            .and_then(Value::as_f64)
            .unwrap_or(1.0);
        self.life = life;
        if pet_id != self.pet_id {
            self.pet_id = pet_id;
            self.species = Species::load(&self.pets_dir, &self.pet_id);
            if !self.roams_species() {
                self.stop_moving();
            }
            self.hop_start = None;
            self.wiggle_start = None;
        }
        let doc = face::without_growth(&bar, self.grew_seq);
        let base = face::current_priority(&doc, self.now as f64, self.seen);
        self.face = face::with_work(&base, self.work_state.as_ref());
        self.observe_face();
    }

    fn observe_face(&mut self) {
        if !self.initialized {
            return;
        }
        let sleeping = self.sleeping();
        if self.sleep_observed && sleeping != self.last_sleeping && self.active() {
            self.start_transient(
                if sleeping {
                    "sleep_enter"
                } else {
                    "sleep_exit"
                },
                0,
            );
        }
        if sleeping {
            self.stop_moving();
        }
        self.last_sleeping = sleeping;
        self.sleep_observed = self.shown();
        let key = face::celebration_key(self.face.celebrate.as_ref());
        if self.face.state() == "celebrate"
            && !key.is_empty()
            && key != self.played_celebration
            && self.active()
        {
            self.played_celebration = key;
            self.cosmetic_expression = false;
            self.celebration_bubble = self.face.bubble.clone();
            let c: Option<Celebrate> = self.face.celebrate.clone();
            self.start_transient("celebrate", 0);
            if let Some(c) = &c {
                if c.kind == "grew" {
                    self.grew_seq = c.seq.unwrap_or(self.grew_seq);
                } else {
                    self.seen = face::mark_seen(self.seen, Some(c));
                }
            }
            self.update_face();
            return;
        }
        self.update_interval();
        self.arm_wake();
    }

    fn arm_wake(&mut self) {
        self.wake_due = match self.face.wake_at_ms {
            Some(w) if self.active() => Some((w.ceil() as u64).max(self.now) + 5),
            _ => None,
        };
    }

    fn clamp_position(&mut self) {
        if self.width <= 0.0 || self.height <= 0.0 {
            return;
        }
        let size = self.pet_size();
        self.pet_x = self.pet_x.min((self.width - size).max(0.0)).max(0.0);
        if self.pet_y >= 0.0 {
            self.pet_y = self.pet_y.min((self.height - size).max(0.0)).max(0.0);
        }
        self.destination = self.destination.min((self.width - size).max(0.0)).max(0.0);
    }

    fn stop_moving(&mut self) {
        if self.walking {
            self.walking = false;
            self.base_elapsed_ms = 0;
        }
        self.swim_ticks_left = 0;
    }

    fn random_pause(&mut self) -> i64 {
        let (lo, hi) = self
            .species
            .as_ref()
            .map(|s| s.locomotion().1)
            .unwrap_or((40, 110));
        lo as i64 + (self.random() * (hi - lo + 1) as f64).floor() as i64
    }

    fn clear_transients(&mut self) {
        self.cosmetic_expression = false;
        self.transient = None;
        self.queued = None;
    }

    fn start_transient(&mut self, state: &str, override_ms: u64) -> bool {
        if !self.active() {
            return false;
        }
        let meta = self.meta(state);
        let duration = if override_ms > 0 {
            override_ms
        } else {
            catalog::duration_ms(&meta).unwrap_or_else(|| catalog::cycle_ms(&meta))
        };
        if let Some((cur, _, _)) = &self.transient {
            if !catalog::admit_replace(Some(cur), &meta) {
                self.queued = Some((meta.name.clone(), duration));
                return true;
            }
        }
        self.stop_moving();
        self.resting = false;
        self.transient = Some((meta, 0, duration));
        self.update_interval();
        true
    }

    fn finish_transient(&mut self) {
        let next = self.queued.take();
        self.clear_transients();
        self.celebration_bubble = None;
        self.base_elapsed_ms = 0;
        self.resting = false;
        if let Some((state, ms)) = next {
            self.start_transient(&state, ms);
        }
    }

    fn start_moving(&mut self) {
        if !self.can_roam() {
            return;
        }
        if self.roams_species() {
            let size = self.pet_size();
            let span = 70.0 + self.random() * 150.0;
            let dir = if self.random() < 0.5 { -1.0 } else { 1.0 };
            self.destination = (self.pet_x + dir * span).min(self.width - size).max(0.0);
            self.facing_left = self.destination < self.pet_x;
            self.walking = (self.destination - self.pet_x).abs() > 2.0;
        } else {
            self.swim_ticks_left = 8;
            self.walking = true;
        }
        self.base_elapsed_ms = 0;
        if !self.walking {
            self.idle_ticks = self.random_pause();
        }
    }

    fn step_movement(&mut self) {
        let meta = self.meta("locomotion");
        let f = catalog::frame_at(&meta, self.base_elapsed_ms / 200, false);
        self.base_elapsed_ms += 200;
        if !self.roams_species() {
            self.swim_ticks_left -= 1;
            if self.swim_ticks_left <= 0 {
                self.walking = false;
                self.base_elapsed_ms = 0;
                self.idle_ticks = self.random_pause();
            }
            return;
        }
        let (px, _, air) =
            self.species
                .as_ref()
                .map(Species::locomotion)
                .unwrap_or((5.0, (40, 110), Vec::new()));
        let step = if !air.is_empty() && !air.contains(&f) {
            0.0
        } else {
            px.max(1.0)
        };
        let distance = self.destination - self.pet_x;
        self.pet_x += distance.clamp(-step, step);
        self.clamp_position();
        if (self.destination - self.pet_x).abs() < 1.0 {
            self.walking = false;
            self.base_elapsed_ms = 0;
            self.idle_ticks = self.random_pause();
        }
    }

    fn cosmetic_tick(&mut self) {
        if !self.active() {
            return;
        }
        let dt = self.timer_interval.max(200);
        if self.heart_ms_left > 0 {
            self.heart_ms_left = (self.heart_ms_left - dt as i64).max(0);
        }
        if let Some(t) = self.transient.as_mut() {
            t.1 += dt;
            if t.1 >= t.2 {
                self.finish_transient();
            }
            self.update_interval();
            return;
        }
        if !self.motion_running() {
            self.update_interval();
            return;
        }
        if self.walking {
            self.step_movement();
            self.update_interval();
            return;
        }
        let hovered = self.hover == Target::Pet;
        if self.resting {
            self.base_elapsed_ms = 0;
            self.idle_ticks -= (dt as f64 / 200.0).round() as i64;
            if self.idle_ticks <= 0 {
                if self.pinned
                    && self.face.state() == "idle"
                    && !self.dragging
                    && !self.card_open
                    && !self.wheel_open
                    && !hovered
                {
                    self.idle_ticks = self.random_pause();
                    self.express(false);
                    return;
                }
                if hovered || !self.can_roam() {
                    self.idle_ticks = self.random_pause();
                } else {
                    self.resting = false;
                    self.start_moving();
                    self.update_interval();
                    return;
                }
            }
            let (meta, _) = self.display();
            self.resting = catalog::frame_count(&meta) <= 1;
            self.update_interval();
            return;
        }
        self.base_elapsed_ms += dt;
        let (meta, _) = self.display();
        if self.base_elapsed_ms >= catalog::cycle_ms(&meta) || catalog::frame_count(&meta) <= 1 {
            self.base_elapsed_ms = 0;
            self.resting = !self.anim().2;
        }
        self.update_interval();
    }

    fn update_interval(&mut self) {
        let ms = if let Some((meta, el, dur)) = &self.transient {
            if self.reduced_motion {
                dur.saturating_sub(*el)
            } else {
                catalog::frame_ms_of(meta)
            }
        } else if self.heart_ms_left > 0 && !self.motion_running() {
            self.heart_ms_left as u64
        } else if self.walking {
            200
        } else if self.resting {
            let mut ms = (self.idle_ticks.max(1) as u64 * 200).clamp(200, 60000);
            if self.heart_ms_left > 0 {
                ms = ms.min(self.heart_ms_left as u64);
            }
            ms
        } else {
            catalog::frame_ms_of(&self.display().0)
        };
        self.timer_interval = catalog::snap_interval(ms);
    }

    /// After any change: track `active` transitions and keep the single scheduler in step.
    fn after(&mut self, now: u64) {
        let active = self.active();
        if active != self.was_active {
            self.was_active = active;
            if !active {
                self.wheel_open = false;
                self.set_message(String::new());
                self.stop_moving();
                self.clear_transients();
                self.heart_ms_left = 0;
                self.hop_start = None;
                self.wiggle_start = None;
                self.hide_linger_until = None;
                self.hover = Target::None;
                self.update_interval();
            } else {
                self.update_face();
            }
            self.arm_wake();
        }
        if !self.can_roam() && self.walking {
            self.stop_moving();
        }
        if !self.scheduler_running() {
            self.tick_due = None;
        } else if self.tick_due.is_none() || self.tick_interval_set != self.timer_interval {
            self.tick_due = Some(now + self.timer_interval);
            self.tick_interval_set = self.timer_interval;
        }
    }

    /// Read-only facts for the `status` verb.
    pub fn status(&self) -> Value {
        serde_json::json!({
            "face": self.face.state(),
            "overlay": self.face.overlay,
            "drawn": self.active(),
            "x": self.pet_x,
            "y": if self.pet_y < 0.0 { self.floor_y() } else { self.pet_y },
            "pinned": self.pinned,
            "dragging": self.dragging,
            "wheel_open": self.wheel_open,
            "card_open": self.card_open,
            "appearance": if self.pet_id.is_empty() { Value::Null } else { Value::String(self.pet_id.clone()) },
            "stage": self.life.as_ref().and_then(|l| l.get("stage")).cloned().unwrap_or(Value::Null),
            "calm": self.reduced_motion,
            "clicks": self.click_count,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    Next,
    Prev,
    Activate,
    Escape,
}

fn out_bounce(p: f64) -> f64 {
    let (n1, d1) = (7.5625, 2.75);
    if p < 1.0 / d1 {
        n1 * p * p
    } else if p < 2.0 / d1 {
        let p = p - 1.5 / d1;
        n1 * p * p + 0.75
    } else if p < 2.5 / d1 {
        let p = p - 2.25 / d1;
        n1 * p * p + 0.9375
    } else {
        let p = p - 2.625 / d1;
        n1 * p * p + 0.984375
    }
}

/// Greedy word wrap with the renderer's measure (explicit newlines kept).
pub fn wrap(m: &dyn Measure, text: &str, size: f32, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if m.width(&candidate, size) <= max_w || line.is_empty() {
                line = candidate;
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            }
        }
        lines.push(line);
    }
    lines
}
