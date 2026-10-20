//! The native desktop pet process: Wayland layer-shell surfaces, input, file watching, the
//! control socket and the explicit requests. Single-threaded and event-driven (calloop):
//! nothing runs between events except the one timer the scene asks for.
//!
//! Surfaces: one full-output, click-through layer surface whose buffer is a single
//! transparent pixel stretched by wp_viewporter, and one small content subsurface around
//! the pet (pet, bubbles, HIDE pill, care wheel). Only the content surface takes input, and
//! only over the pet, the pill and the wheel.

use crate::jobs::{self, JobResult};
use crate::model::{barstate, settings, work};
use crate::paths::Paths;
use crate::render::{self, Renderer};
use crate::scene::{Effect, Key, Rect, Scene, View};
use calloop::channel::{self, Channel, Sender};
use calloop::generic::Generic;
use calloop::timer::{TimeoutAction, Timer};
use calloop::{EventLoop, Interest, LoopHandle, Mode, PostAction, RegistrationToken};
use calloop_wayland_source::WaylandSource;
use serde_json::Value;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::dispatch2::Dispatch2;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::pointer::cursor_shape::CursorShapeManager;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::subcompositor::SubcompositorState;
use smithay_client_toolkit::{delegate_dispatch2, delegate_registry, registry_handlers};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_subsurface, wl_surface};
use wayland_client::{Connection, QueueHandle, WEnum};
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{Shape, WpCursorShapeDeviceV1};
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const HOME_COOLDOWN_MS: u64 = 1000;
const CARE_RETRY_MS: u64 = 120000;

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

struct Content {
    surface: wl_surface::WlSurface,
    sub: wl_subsurface::WlSubsurface,
    viewport: Option<WpViewport>,
    origin: (f64, f64),
    mapped: bool,
}

pub struct App {
    pub exit: Option<String>,
    paths: Paths,
    conn: Connection,
    qh: QueueHandle<App>,
    handle: LoopHandle<'static, App>,
    registry: RegistryState,
    seat: SeatState,
    outputs: OutputState,
    compositor: CompositorState,
    subcompositor: SubcompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    pool: Option<SlotPool>,
    viewporter: Option<WpViewporter>,
    cursor_shapes: Option<CursorShapeManager>,
    layer: Option<LayerSurface>,
    layer_viewport: Option<WpViewport>,
    parent_buffer: Option<Buffer>,
    content: Option<Content>,
    configured: bool,
    output: Option<wl_output::WlOutput>,
    pub preferred_screen: String,
    scale: i32,
    size: (u32, u32),
    keyboard_on_demand: bool,
    pointer: Option<wl_pointer::WlPointer>,
    shape_device: Option<WpCursorShapeDeviceV1>,
    enter_serial: u32,
    cursor: Option<Shape>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    shift: bool,
    pub scene: Scene,
    renderer: Renderer,
    last_view: Option<View>,
    watcher: barstate::Watcher,
    pub desktop_pet: Option<bool>,
    calm_setting: Option<bool>,
    calm_override: Option<bool>,
    placement: Option<settings::Placement>,
    saw_on: bool,
    settle_until: Option<u64>,
    pub hidden: bool,
    pub locked: bool,
    pub suspended: bool,
    pub fullscreen: bool,
    jobs_tx: Sender<JobResult>,
    care_busy: bool,
    care_retries: HashMap<String, (String, u64)>,
    seq: u64,
    startup_read: bool,
    home_requests: u64,
    last_home: u64,
    placement_busy: bool,
    placement_pending: Option<(f64, f64, bool)>,
    work_busy: bool,
    attempted_scout: String,
    client_seen: String,
    scout_due: Option<u64>,
    timer: Option<RegistrationToken>,
    timer_at: Option<u64>,
}

// ---- user data for protocol objects this file dispatches itself ----

pub struct ViewporterData;
pub struct ViewportData;
pub struct KeyboardData;

impl Dispatch2<WpViewporter, App> for ViewporterData {
    fn event(&self, _: &mut App, _: &WpViewporter, _: <WpViewporter as wayland_client::Proxy>::Event, _: &Connection, _: &QueueHandle<App>) {}
}
impl Dispatch2<WpViewport, App> for ViewportData {
    fn event(&self, _: &mut App, _: &WpViewport, _: <WpViewport as wayland_client::Proxy>::Event, _: &Connection, _: &QueueHandle<App>) {}
}
impl Dispatch2<wl_keyboard::WlKeyboard, App> for KeyboardData {
    fn event(&self, app: &mut App, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event, _: &Connection, _: &QueueHandle<App>) {
        match event {
            wl_keyboard::Event::Keymap { fd, .. } => drop(fd),
            wl_keyboard::Event::Modifiers { mods_depressed, .. } => app.shift = mods_depressed & 1 != 0,
            wl_keyboard::Event::Key { key, state: WEnum::Value(wl_keyboard::KeyState::Pressed), .. } => {
                // evdev codes: Esc 1, Tab 15, Enter 28, Space 57, KP Enter 96, arrows 103/105/106/108.
                let k = match key {
                    1 => Some(Key::Escape),
                    15 => Some(if app.shift { Key::Prev } else { Key::Next }),
                    28 | 57 | 96 => Some(Key::Activate),
                    105 | 103 => Some(Key::Prev),
                    106 | 108 => Some(Key::Next),
                    _ => None,
                };
                if let Some(k) = k {
                    app.scene.key(k, now_ms());
                }
            }
            _ => {}
        }
    }
}

impl App {
    pub fn surface_active(&self) -> bool {
        !self.hidden && !self.locked && !self.suspended && !self.fullscreen && self.output.is_some() && self.exit_reason().is_empty()
    }

    pub fn exit_reason(&self) -> &'static str {
        settings::exit_reason(self.scene.bar_state.as_ref(), self.desktop_pet)
    }

    fn calm(&self) -> bool {
        settings::reduced_motion(self.calm_override, self.calm_setting, std::env::var("CODEPET_REDUCED_MOTION").ok().as_deref())
    }

    fn new_id(&mut self, tag: &str) -> String {
        self.seq += 1;
        format!("{tag}-{:x}-{}", now_ms(), self.seq)
    }

    // ---- files ----

    pub fn read_bar_state(&mut self) {
        let doc = std::fs::read(self.paths.bar_state()).ok().and_then(|t| barstate::parse(&t));
        match doc {
            Some(doc) => {
                if self.watcher.accept(&doc) {
                    self.scene.set_bar_state(Some(doc), now_ms());
                }
            }
            None => self.startup_state_get(),
        }
    }

    pub fn read_work_state(&mut self) {
        let doc = std::fs::read(self.paths.work_state())
            .ok()
            .filter(|t| t.len() <= 1 << 20)
            .and_then(|t| serde_json::from_slice::<Value>(&t).ok());
        if let Some(d) = &doc {
            self.observe_client(d);
        }
        self.scene.set_work_state(doc, now_ms());
    }

    pub fn read_settings(&mut self) {
        let text = std::fs::read(self.paths.settings()).unwrap_or_default();
        let s = settings::read(&text);
        self.desktop_pet = s.desktop_pet;
        if s.desktop_pet == Some(true) {
            self.saw_on = true;
        }
        if s.calm != self.calm_setting {
            self.calm_setting = s.calm;
            self.calm_override = None;
        }
        if let Some(next) = s.placement {
            if Some(&next) != self.placement.as_ref() {
                let prev = self.placement.clone();
                self.placement = Some(next.clone());
                // A pin switch from More keeps the current position (it may have wandered).
                if let Some(p) = prev {
                    if p.pinned != next.pinned && p.x == next.x && p.y == next.y && p.screen == next.screen {
                        self.scene.pinned = next.pinned;
                        let (x, y) = self.placement_fraction();
                        self.save_placement(x, y, next.pinned);
                        return;
                    }
                }
                if !next.screen.is_empty() {
                    self.preferred_screen = next.screen.clone();
                }
                self.scene.restore_placement(next.x, next.y, next.pinned, now_ms());
            }
        }
    }

    fn placement_fraction(&self) -> (f64, f64) {
        let s = &self.scene;
        let size = s.pet_size();
        let y = if s.pet_y < 0.0 { (s.height - size - 12.0).max(0.0) } else { s.pet_y };
        (s.pet_x / (s.width - size).max(1.0), y / (s.height - size).max(1.0))
    }

    fn startup_state_get(&mut self) {
        if self.startup_read || self.scene.bar_state.is_some() {
            return;
        }
        self.startup_read = true;
        self.state_get();
    }

    pub fn state_get(&mut self) {
        if self.care_busy {
            return;
        }
        self.care_busy = true;
        let id = self.new_id("pet");
        jobs::rpc(&self.paths.codepet_bin, &id, "state.get", serde_json::json!({}), "state.get".into(), self.jobs_tx.clone());
    }

    // ---- effects from the scene ----

    fn run_effects(&mut self) {
        for e in self.scene.take_effects() {
            match e {
                Effect::Care(action) => self.care(&action),
                Effect::ToggleCard => self.toggle_card(),
                Effect::OpenHome => self.open_home(),
                Effect::Hide => {
                    if !self.hidden && self.surface_active() {
                        self.hidden = true;
                        jobs::detached("toast", settings::hidden_toast_argv(), None);
                    }
                }
                Effect::SavePlacement { x, y, pinned } => self.save_placement(x, y, pinned),
            }
        }
    }

    fn care(&mut self, action: &str) {
        if !self.surface_active() || !["food", "water", "play", "wash", "sleep", "wake"].contains(&action) {
            return;
        }
        if self.care_busy {
            self.scene.care_answered(action, false, false, Some("One moment…".into()), now_ms());
            return;
        }
        let life = self.scene.bar_state.as_ref().and_then(|b| b.get("life")).cloned().unwrap_or(Value::Null);
        let (mut method, mut params) = match action {
            "sleep" => ("pet.sleep", serde_json::json!({})),
            "wake" => ("pet.wake", serde_json::json!({})),
            _ => ("pet.care", serde_json::json!({"action": action})),
        };
        if action == "wake" && life.get("paused") == Some(&Value::Bool(true)) {
            method = "pet.pause";
            params = serde_json::json!({"paused": false});
        }
        let signature = format!("{}:{action}", life.get("life_id").and_then(Value::as_str).unwrap_or(""));
        let now = now_ms();
        let id = match self.care_retries.get(&signature) {
            Some((id, until)) if *until > now => id.clone(),
            _ => self.new_id("pet"),
        };
        self.care_retries.insert(signature.clone(), (id.clone(), now + CARE_RETRY_MS));
        self.care_busy = true;
        jobs::rpc(&self.paths.codepet_bin, &id, method, params, format!("{action}|{signature}"), self.jobs_tx.clone());
    }

    fn toggle_card(&mut self) {
        let qml = self.paths.work_qml.to_string_lossy().into_owned();
        let launcher = self.paths.work_launcher.clone();
        let bin = self.paths.codepet_bin.clone();
        let tx = self.jobs_tx.clone();
        std::thread::spawn(move || {
            let status = jobs::output(&["qs", "ipc", "-p", &qml, "call", "--", "codepet-work", "status"], Duration::from_secs(2));
            let visible = status.as_deref().is_some_and(|s| s.contains("\"visible\":true") && s.contains("\"compact\":true"));
            if visible {
                let _ = jobs::output(&["qs", "ipc", "-p", &qml, "call", "--", "codepet-work", "close"], Duration::from_secs(2));
            } else {
                let _ = std::process::Command::new("/usr/bin/env")
                    .arg(format!("CODEPET_BIN={bin}"))
                    .arg(&launcher)
                    .arg("compact")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
            let _ = tx.send(JobResult::Done("card"));
        });
    }

    pub fn open_home(&mut self) {
        if !self.exit_reason().is_empty() {
            return;
        }
        let now = now_ms();
        if self.last_home > 0 && now >= self.last_home && now - self.last_home < HOME_COOLDOWN_MS {
            return;
        }
        self.last_home = now;
        self.home_requests += 1;
        jobs::detached("home", settings::open_home_argv(&self.paths.omarchy_path), None);
    }

    fn save_placement(&mut self, x: f64, y: f64, pinned: bool) {
        if self.placement_busy {
            self.placement_pending = Some((x, y, pinned));
            return;
        }
        let screen = self.output.as_ref().and_then(|o| self.outputs.info(o)).and_then(|i| i.name).unwrap_or_default();
        self.placement = Some(settings::Placement { x: Some(x), y: Some(y), pinned, screen: screen.clone() });
        self.placement_busy = true;
        jobs::detached("placement", settings::placement_argv(&self.paths.omarchy_path, x, y, pinned, &screen), Some(self.jobs_tx.clone()));
    }

    fn on_job(&mut self, r: JobResult) {
        let now = now_ms();
        match r {
            JobResult::Care(tag, resp) => {
                self.care_busy = false;
                if tag == "state.get" {
                    self.read_bar_state();
                    return;
                }
                let (action, signature) = tag.split_once('|').unwrap_or((tag.as_str(), ""));
                let ok = resp.get("ok") == Some(&Value::Bool(true));
                if ok {
                    self.care_retries.remove(signature);
                }
                let changed = resp.get("result").and_then(|r| r.get("changed")) == Some(&Value::Bool(true));
                let msg = resp.get("error").and_then(|e| e.get("message")).and_then(Value::as_str).map(str::to_string);
                self.scene.care_answered(action, ok, changed, msg, now);
                self.read_bar_state();
            }
            JobResult::Work(_, _) => {
                self.work_busy = false;
                self.read_work_state();
            }
            JobResult::Done("placement") => {
                self.placement_busy = false;
                if let Some((x, y, p)) = self.placement_pending.take() {
                    self.save_placement(x, y, p);
                }
            }
            JobResult::Done(_) => {}
        }
    }

    // ---- automatic scouting (the scheduling half of WorkController.qml) ----

    fn automatic(&self) -> bool {
        let bar = self.scene.bar_state.as_ref();
        self.surface_active()
            && bar.and_then(|b| b.get("enabled")) == Some(&Value::Bool(true))
            && bar.and_then(|b| b.get("life")).and_then(|l| l.get("armed")) == Some(&Value::Bool(true))
    }

    fn observe_client(&mut self, summary: &Value) {
        let id = summary.get("provider").and_then(|p| p.get("id")).and_then(Value::as_str).unwrap_or("codex").to_string();
        if !self.client_seen.is_empty() && id != self.client_seen {
            // A client choice must not turn a due check into a launch.
            if work::next_due_ms(summary).is_some_and(|d| d <= now_ms() as f64) {
                self.attempted_scout = work::scout_key(summary);
            }
        }
        self.client_seen = id;
    }

    fn arm_scout(&mut self) {
        self.scout_due = None;
        let Some(s) = self.scene.work_state.as_ref() else { return };
        if !self.automatic() || self.work_busy || !work::scout_eligible(s) || work::scout_key(s) == self.attempted_scout {
            return;
        }
        let due = work::next_due_ms(s).unwrap_or(0.0).max(0.0) as u64;
        self.scout_due = Some(due.max(now_ms() + 20));
    }

    fn scout(&mut self) {
        let Some(s) = self.scene.work_state.clone() else { return };
        if !self.automatic() || self.work_busy || !work::scout_eligible(&s) {
            return;
        }
        self.attempted_scout = work::scout_key(&s);
        self.work_busy = true;
        jobs::work_rpc(&self.paths.codepet_bin, "scout", serde_json::json!({}), self.jobs_tx.clone());
    }

    // ---- surfaces ----

    fn pick_output(&self) -> Option<wl_output::WlOutput> {
        let outs: Vec<_> = self.outputs.outputs().collect();
        outs.iter()
            .find(|o| self.outputs.info(o).and_then(|i| i.name).is_some_and(|n| n == self.preferred_screen))
            .or(outs.first())
            .cloned()
    }

    fn sync_surfaces(&mut self) {
        let want = self.pick_output();
        if want != self.output {
            self.destroy_surfaces();
            self.output = want;
        }
        let active = self.surface_active();
        if active && self.layer.is_none() {
            self.create_surfaces();
        } else if !active && self.layer.is_some() {
            self.destroy_surfaces();
        }
        self.scene.set_surface_active(active && self.configured, now_ms());
    }

    fn create_surfaces(&mut self) {
        let Some(output) = self.output.clone() else { return };
        self.scale = self.outputs.info(&output).map(|i| i.scale_factor).unwrap_or(1).max(1);
        let surface = self.compositor.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(&self.qh, surface, Layer::Top, Some("codepet-overlay"), Some(&output));
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_size(0, 0);
        layer.set_exclusive_zone(0);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        self.keyboard_on_demand = false;
        // Click-through everywhere: the parent's input region is empty.
        if let Ok(empty) = Region::new(&self.compositor) {
            layer.wl_surface().set_input_region(Some(empty.wl_region()));
        }
        layer.commit();
        self.layer = Some(layer);
        self.configured = false;
    }

    fn destroy_surfaces(&mut self) {
        if let Some(c) = self.content.take() {
            if let Some(v) = c.viewport {
                v.destroy();
            }
            c.sub.destroy();
            c.surface.destroy();
        }
        if let Some(v) = self.layer_viewport.take() {
            v.destroy();
        }
        self.layer = None;
        self.parent_buffer = None;
        self.configured = false;
        self.last_view = None;
        // Hidden or suppressed: give back the buffers and decoded images.
        self.pool = None;
        self.renderer.trim();
    }

    fn pool(&mut self) -> Option<&mut SlotPool> {
        if self.pool.is_none() {
            self.pool = SlotPool::new(64 * 1024, &self.shm).ok();
        }
        self.pool.as_mut()
    }

    fn on_configure(&mut self, w: u32, h: u32) {
        let (w, h) = (w.max(1), h.max(1));
        self.size = (w, h);
        let first = !self.configured;
        self.configured = true;
        if first {
            let Some(layer) = self.layer.clone() else { return };
            let parent = layer.wl_surface().clone();
            // The parent shows nothing: one transparent pixel stretched over the output.
            let (bw, bh) = if self.viewporter.is_some() { (1, 1) } else { (w as i32, h as i32) };
            let made = self.pool().and_then(|p| p.create_buffer(bw, bh, bw * 4, wl_shm::Format::Argb8888).ok());
            if let Some((buf, canvas)) = made {
                canvas.fill(0);
                let _ = buf.attach_to(&parent);
                self.parent_buffer = Some(buf);
            }
            if let Some(vp) = &self.viewporter {
                let v = vp.get_viewport(&parent, &self.qh, ViewportData);
                v.set_destination(w as i32, h as i32);
                self.layer_viewport = Some(v);
            }
            parent.damage_buffer(0, 0, i32::MAX, i32::MAX);
            let (sub, surface) = self.subcompositor.create_subsurface(parent.clone(), &self.qh);
            sub.set_desync();
            if let Ok(empty) = Region::new(&self.compositor) {
                surface.set_input_region(Some(empty.wl_region()));
            }
            let viewport = self.viewporter.as_ref().map(|vp| vp.get_viewport(&surface, &self.qh, ViewportData));
            self.content = Some(Content { surface, sub, viewport, origin: (0.0, 0.0), mapped: false });
            layer.commit();
        } else if let Some(v) = &self.layer_viewport {
            v.set_destination(w as i32, h as i32);
            if let Some(l) = &self.layer {
                l.commit();
            }
        }
        self.last_view = None;
        self.scene.set_size(w as f64, h as f64, now_ms());
        self.scene.set_surface_active(self.surface_active(), now_ms());
    }

    /// Draws the content surface when the view changed.
    fn redraw(&mut self) {
        if !self.configured || self.content.is_none() {
            return;
        }
        let view = self.scene.view(&self.renderer);
        if self.last_view.as_ref() == Some(&view) {
            return;
        }
        let mut area = Rect::default();
        if let Some(s) = &view.sprite {
            // Room for the wiggle rotation and the hop.
            area = area.union(&Rect::new(s.3.x - 12.0, s.3.y - 20.0, s.3.w + 24.0, s.3.h + 28.0));
        }
        for r in [view.pinned_tag, view.badge, view.bubble.as_ref().map(|b| b.0), view.feedback.as_ref().map(|f| f.0),
                  view.hide_pill.map(|p| p.0), view.wheel.as_ref().map(|w| w.0)].into_iter().flatten() {
            area = area.union(&r);
        }
        let (sw, sh) = (self.size.0 as f64, self.size.1 as f64);
        let x0 = area.x.max(0.0).floor();
        let y0 = area.y.max(0.0).floor();
        let x1 = (area.x + area.w).min(sw).ceil();
        let y1 = (area.y + area.h).min(sh).ceil();
        let area = Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0));
        let scale = self.scale;
        let content_surface = self.content.as_ref().map(|c| c.surface.clone());
        let Some(surface) = content_surface else { return };
        if !view.active || area.is_empty() {
            surface.attach(None, 0, 0);
            surface.commit();
            if let Some(c) = self.content.as_mut() {
                c.mapped = false;
            }
            self.last_view = Some(view);
            return;
        }
        let Some(pm) = self.renderer.draw(&view, area, scale as f64) else { return };
        let (bw, bh) = (pm.width() as i32, pm.height() as i32);
        let Some((buf, canvas)) = self.pool().and_then(|p| p.create_buffer(bw, bh, bw * 4, wl_shm::Format::Argb8888).ok()) else { return };
        render::to_argb8888(&pm, canvas);
        drop(pm);
        if let Some(c) = self.content.as_mut() {
            if let Some(v) = &c.viewport {
                v.set_destination(area.w as i32, area.h as i32);
            } else {
                surface.set_buffer_scale(scale);
            }
            let _ = buf.attach_to(&surface);
            surface.damage_buffer(0, 0, bw, bh);
            if let Ok(region) = Region::new(&self.compositor) {
                for r in self.scene.input_rects() {
                    let (rx, ry) = ((r.x - area.x).floor() as i32, (r.y - area.y).floor() as i32);
                    region.add(rx, ry, r.w.ceil() as i32 + 1, r.h.ceil() as i32 + 1);
                }
                surface.set_input_region(Some(region.wl_region()));
            }
            c.sub.set_position(area.x as i32, area.y as i32);
            c.origin = (area.x, area.y);
            c.mapped = true;
            surface.commit();
        }
        if let Some(l) = &self.layer {
            l.commit();
        }
        self.last_view = Some(view);
    }

    fn update_keyboard_mode(&mut self) {
        let want = self.scene.wants_keyboard();
        if want != self.keyboard_on_demand {
            if let Some(l) = &self.layer {
                l.set_keyboard_interactivity(if want { KeyboardInteractivity::OnDemand } else { KeyboardInteractivity::None });
                l.commit();
            }
            self.keyboard_on_demand = want;
        }
    }

    fn update_cursor(&mut self) {
        let want = if self.scene.dragging { Shape::Grabbing } else { Shape::Grab };
        if self.cursor != Some(want) {
            if let Some(d) = &self.shape_device {
                d.set_shape(self.enter_serial, want);
            }
            self.cursor = Some(want);
        }
    }

    /// After every batch of events: apply exits, surfaces, effects, drawing and the timer.
    pub fn flush(&mut self) {
        let now = now_ms();
        let reason = self.exit_reason();
        if !reason.is_empty() {
            if settings::exits_at_once(reason, self.saw_on) {
                self.exit = Some(reason.to_string());
                return;
            }
            if self.settle_until.is_none() {
                self.settle_until = Some(now + settings::SETTLE_MS);
            }
            if self.settle_until.is_some_and(|u| now >= u) {
                self.exit = Some(reason.to_string());
                return;
            }
        } else {
            self.settle_until = None;
        }
        self.scene.set_reduced_motion(self.calm(), now);
        self.sync_surfaces();
        self.run_effects();
        self.sync_surfaces();
        self.arm_scout();
        self.update_keyboard_mode();
        self.redraw();
        self.reschedule();
        let _ = self.conn.flush();
    }

    fn reschedule(&mut self) {
        let next = [self.scene.next_deadline(), self.scout_due, self.settle_until].into_iter().flatten().min();
        if next == self.timer_at && self.timer.is_some() {
            return;
        }
        if let Some(t) = self.timer.take() {
            self.handle.remove(t);
        }
        self.timer_at = next;
        let Some(at) = next else { return };
        let delay = Duration::from_millis(at.saturating_sub(now_ms()));
        let token = self.handle.insert_source(Timer::from_duration(delay), |_, _, app: &mut App| {
            app.timer = None;
            app.timer_at = None;
            let now = now_ms();
            if app.scout_due.is_some_and(|d| now >= d) {
                app.scout_due = None;
                app.scout();
            }
            app.scene.advance(now);
            TimeoutAction::Drop
        });
        self.timer = token.ok();
    }

    // ---- control socket ----

    pub fn handle_verb(&mut self, line: &str) -> String {
        let mut parts = line.split_whitespace();
        let verb = parts.next().unwrap_or("");
        let arg = parts.collect::<Vec<_>>();
        let now = now_ms();
        let flag = |v: Option<&&str>| v.is_some_and(|s| *s == "true" || *s == "1");
        match verb {
            "ping" => format!("codepet-overlay {}", if self.hidden { "hidden" } else { "shown" }),
            "status" => self.status().to_string(),
            "show" => {
                self.hidden = false;
                "ok".into()
            }
            "hide" => {
                self.hidden = true;
                "ok".into()
            }
            "toggle" => {
                self.hidden = !self.hidden;
                "ok".into()
            }
            "refresh" => {
                self.read_bar_state();
                self.read_work_state();
                self.read_settings();
                self.state_get();
                "ok".into()
            }
            "openHome" => {
                self.open_home();
                "ok".into()
            }
            "openCard" => {
                if self.surface_active() {
                    self.toggle_card();
                }
                "ok".into()
            }
            "setReducedMotion" => {
                self.calm_override = Some(flag(arg.first()));
                "ok".into()
            }
            "selectScreen" => {
                self.preferred_screen = arg.first().unwrap_or(&"").to_string();
                "ok".into()
            }
            "setSessionState" => {
                self.locked = flag(arg.first());
                self.suspended = flag(arg.get(1));
                "ok".into()
            }
            "setFullscreen" => {
                self.fullscreen = flag(arg.first());
                "ok".into()
            }
            "quit" => {
                self.exit = Some("quit".into());
                "ok".into()
            }
            _ => {
                let _ = now;
                "error: unknown verb".into()
            }
        }
    }

    fn status(&self) -> Value {
        let s = self.scene.bar_state.as_ref();
        let life = s.and_then(|s| s.get("life"));
        let mut v = serde_json::json!({
            "engine": "native",
            "pid": std::process::id(),
            "pet": if self.hidden { "hidden" } else if self.surface_active() { "shown" } else { "suppressed" },
            "busy": self.care_busy,
            "supervisor": false,
            "enabled": s.map(|s| s.get("enabled") != Some(&Value::Bool(false))),
            "desktop_pet": self.desktop_pet,
            "exit_reason": self.exit_reason(),
            "calm": self.calm(),
            "home_requests": self.home_requests,
            "work_busy": self.work_busy,
            "state_version": s.and_then(|s| s.get("state_version")).cloned(),
            "screen": self.output.as_ref().and_then(|o| self.outputs.info(o)).and_then(|i| i.name),
            "scout_due_ms": self.scout_due,
        });
        if let (Some(m), Some(sc)) = (v.as_object_mut(), self.scene.status().as_object()) {
            for (k, val) in sc {
                m.insert(k.clone(), val.clone());
            }
            m.insert("stage".into(), life.and_then(|l| l.get("stage")).cloned().unwrap_or(Value::Null));
        }
        v
    }
}

fn serve(app: &mut App, listener: &UnixListener) {
    loop {
        match listener.accept() {
            Ok((stream, _)) => answer(app, stream),
            Err(_) => break,
        }
    }
}

fn answer(app: &mut App, stream: UnixStream) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    let mut line = String::new();
    let mut reader = BufReader::new(std::io::Read::take(&stream, 4096));
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let reply = app.handle_verb(line.trim());
    let mut w = &stream;
    let _ = w.write_all(reply.as_bytes());
    let _ = w.write_all(b"\n");
}

// ---- SCTK handlers ----

impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, factor: i32) {
        if factor.max(1) != self.scale {
            self.scale = factor.max(1);
            self.last_view = None;
        }
    }
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        if Some(&output) == self.output.as_ref() {
            let scale = self.outputs.info(&output).map(|i| i.scale_factor).unwrap_or(1).max(1);
            if scale != self.scale {
                self.scale = scale;
                self.last_view = None;
            }
        }
    }
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        if Some(&output) == self.output.as_ref() {
            self.destroy_surfaces();
            self.output = None;
        }
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.destroy_surfaces();
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface, c: LayerSurfaceConfigure, _: u32) {
        self.on_configure(c.new_size.0, c.new_size.1);
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Pointer && self.pointer.is_none() {
            if let Ok(p) = self.seat.get_pointer(qh, &seat) {
                self.shape_device = self.cursor_shapes.as_ref().map(|m| m.get_shape_device(&p, qh));
                self.pointer = Some(p);
            }
        }
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = Some(seat.get_keyboard(qh, KeyboardData));
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Pointer {
            if let Some(d) = self.shape_device.take() {
                d.destroy();
            }
            if let Some(p) = self.pointer.take() {
                p.release();
            }
        }
        if cap == Capability::Keyboard {
            if let Some(k) = self.keyboard.take() {
                k.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for App {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        let Some(c) = self.content.as_ref() else { return };
        let (surface, origin) = (c.surface.clone(), c.origin);
        let now = now_ms();
        for e in events {
            if e.surface != surface {
                continue;
            }
            let (x, y) = (origin.0 + e.position.0, origin.1 + e.position.1);
            match e.kind {
                PointerEventKind::Enter { serial } => {
                    self.enter_serial = serial;
                    self.cursor = None;
                    self.scene.pointer_motion(x, y, now);
                }
                PointerEventKind::Leave { .. } => {
                    if !self.scene.dragging {
                        self.scene.pointer_leave(now);
                    }
                }
                PointerEventKind::Motion { .. } => self.scene.pointer_motion(x, y, now),
                PointerEventKind::Press { button, .. } if button == BTN_LEFT || button == BTN_RIGHT => {
                    self.scene.pointer_press(x, y, button == BTN_LEFT, now)
                }
                PointerEventKind::Release { button, .. } if button == BTN_LEFT || button == BTN_RIGHT => {
                    self.scene.pointer_release(x, y, button == BTN_LEFT, now)
                }
                _ => {}
            }
        }
        self.update_cursor();
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_registry!(App);
delegate_dispatch2!(App);

fn watch(fd: &rustix::fd::OwnedFd, dir: &std::path::Path) -> Option<i32> {
    use rustix::fs::inotify::WatchFlags;
    rustix::fs::inotify::add_watch(
        fd,
        dir,
        WatchFlags::CLOSE_WRITE | WatchFlags::MOVED_TO | WatchFlags::CREATE | WatchFlags::DELETE,
    )
    .ok()
}

/// Runs the pet until it is told to quit or CodePet / "Pet on my desktop" turns off.
pub fn run(paths: Paths, listener: UnixListener) -> Result<String, String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland display: {e}"))?;
    let (globals, queue) = registry_queue_init::<App>(&conn).map_err(|e| format!("registry: {e}"))?;
    let qh = queue.handle();
    let mut event_loop: EventLoop<'static, App> = EventLoop::try_new().map_err(|e| e.to_string())?;
    let handle = event_loop.handle();
    WaylandSource::new(conn.clone(), queue).insert(handle.clone()).map_err(|e| e.to_string())?;

    let compositor = CompositorState::bind(&globals, &qh).map_err(|e| format!("wl_compositor: {e}"))?;
    let subcompositor = SubcompositorState::bind(compositor.wl_compositor().clone(), &globals, &qh)
        .map_err(|e| format!("wl_subcompositor: {e}"))?;
    let layer_shell = LayerShell::bind(&globals, &qh).map_err(|e| format!("layer shell: {e}"))?;
    let shm = Shm::bind(&globals, &qh).map_err(|e| format!("wl_shm: {e}"))?;
    let viewporter = globals.bind::<WpViewporter, _, _>(&qh, 1..=1, ViewporterData).ok();
    let cursor_shapes = CursorShapeManager::bind(&globals, &qh).ok();

    let (jobs_tx, jobs_rx): (Sender<JobResult>, Channel<JobResult>) = channel::channel();
    handle
        .insert_source(jobs_rx, |ev, _, app: &mut App| {
            if let channel::Event::Msg(r) = ev {
                app.on_job(r);
            }
        })
        .map_err(|e| e.to_string())?;

    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    handle
        .insert_source(Generic::new(listener, Interest::READ, Mode::Level), |_, l, app: &mut App| {
            serve(app, l);
            Ok(PostAction::Continue)
        })
        .map_err(|e| e.to_string())?;

    // File watching: the state dir (bar-state.json, work-state.json) and ~/.config/omarchy.
    let inotify = rustix::fs::inotify::init(rustix::fs::inotify::CreateFlags::CLOEXEC | rustix::fs::inotify::CreateFlags::NONBLOCK)
        .map_err(|e| format!("inotify: {e}"))?;
    let state_wd = watch(&inotify, &paths.state);
    let config_wd = watch(&inotify, &paths.omarchy_config);
    let parent_wd = if state_wd.is_none() { paths.state.parent().and_then(|p| watch(&inotify, p)) } else { None };
    let state_dir = paths.state.clone();
    let mut state_wd_live = state_wd;
    handle
        .insert_source(Generic::new(inotify, Interest::READ, Mode::Level), move |_, fd, app: &mut App| {
            let mut buf = [std::mem::MaybeUninit::<u8>::uninit(); 4096];
            let (mut bar, mut work, mut settings, mut created) = (false, false, false, false);
            {
                let mut reader = rustix::fs::inotify::Reader::new(&**fd, &mut buf);
                while let Ok(ev) = reader.next() {
                    let name = ev.file_name().map(|c| c.to_string_lossy().into_owned()).unwrap_or_default();
                    if Some(ev.wd()) == state_wd_live {
                        bar |= name == "bar-state.json";
                        work |= name == "work-state.json";
                    } else if Some(ev.wd()) == config_wd {
                        settings |= name == "shell.json";
                    } else if Some(ev.wd()) == parent_wd && name == "codepet" {
                        created = true;
                    }
                }
            }
            if created && state_wd_live.is_none() {
                state_wd_live = watch(&**fd, &state_dir);
                bar = true;
                work = true;
            }
            if bar {
                app.read_bar_state();
            }
            if work {
                app.read_work_state();
            }
            if settings {
                app.read_settings();
            }
            Ok(PostAction::Continue)
        })
        .map_err(|e| e.to_string())?;

    let seed = now_ms() ^ (std::process::id() as u64).rotate_left(32);
    let mut renderer = Renderer::new(&paths.assets);
    let mut scene = Scene::new(paths.assets.join("pets"), seed);
    scene.set_measure(&renderer);
    let _ = &mut renderer;
    let mut app = App {
        exit: None,
        preferred_screen: std::env::var("CODEPET_SCREEN").unwrap_or_default(),
        paths,
        conn: conn.clone(),
        qh: qh.clone(),
        handle,
        registry: RegistryState::new(&globals),
        seat: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        compositor,
        subcompositor,
        layer_shell,
        shm,
        pool: None,
        viewporter,
        cursor_shapes,
        layer: None,
        layer_viewport: None,
        parent_buffer: None,
        content: None,
        configured: false,
        output: None,
        scale: 1,
        size: (0, 0),
        keyboard_on_demand: false,
        pointer: None,
        shape_device: None,
        enter_serial: 0,
        cursor: None,
        keyboard: None,
        shift: false,
        scene,
        renderer,
        last_view: None,
        watcher: barstate::Watcher::default(),
        desktop_pet: None,
        calm_setting: None,
        calm_override: None,
        placement: None,
        saw_on: false,
        settle_until: None,
        hidden: false,
        locked: false,
        suspended: false,
        fullscreen: false,
        jobs_tx,
        care_busy: false,
        care_retries: HashMap::new(),
        seq: 0,
        startup_read: false,
        home_requests: 0,
        last_home: 0,
        placement_busy: false,
        placement_pending: None,
        work_busy: false,
        attempted_scout: String::new(),
        client_seen: String::new(),
        scout_due: None,
        timer: None,
        timer_at: None,
    };
    // Outputs arrive with the first roundtrips.
    event_loop.dispatch(Duration::from_millis(50), &mut app).map_err(|e| e.to_string())?;
    app.scene.start(now_ms());
    app.read_settings();
    app.read_bar_state();
    app.read_work_state();
    let started = Instant::now();
    loop {
        app.flush();
        if let Some(reason) = app.exit.take() {
            app.destroy_surfaces();
            let _ = app.conn.flush();
            let _ = started;
            return Ok(reason);
        }
        event_loop.dispatch(None, &mut app).map_err(|e| e.to_string())?;
    }
}
