//! Per-render-frame client logic for the local player: mouse look, key bindings, sending one
//! [`FpsInput`] per logic frame, the camera (with RTS <-> first-person transitions), and HUD timers.
//!
//! Nothing here affects the lockstep simulation except through the input messages it emits.

use crate::input::{buttons, key_name, pack_move, FpsInput, Key, KeySet, MAX_PITCH};
use crate::math::{dir_from_angles, lerp_angle, smoothstep, wrap_angle, Vec3};
use crate::sim::{EventKind, Mode, Sim};
use crate::sounds::{self, Cues};
use crate::units::LOGIC_DT;
use crate::viewmodel::{self, Viewmodel};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bindings {
    pub forward: Key,
    pub back: Key,
    pub left: Key,
    pub right: Key,
    pub fire: Key,
    pub ads: Key,
    pub sprint: Key,
    pub crouch: Key,
    pub prone: Key,
    pub jump: Key,
    pub reload: Key,
    pub swap: Key,
    pub melee: Key,
    pub field_computer: Key,
    /// Throw lethal / tactical (hold to cook).
    pub lethal: Key,
    pub tactical: Key,
    /// Held: the whole map (BO2's scoreboard key).
    pub map: Key,
    /// BO2's action slot 3: the weapon's alt mode (select fire, grenade launcher, second optic).
    pub alt_mode: Key,
    /// The builder's hammer (the use key builds at a site).
    pub hammer: Key,
}

impl Default for Bindings {
    /// BO2 PC defaults, with the scorestreak/"use" key repurposed for the field computer.
    fn default() -> Self {
        Self {
            forward: Key::W,
            back: Key::S,
            left: Key::A,
            right: Key::D,
            fire: Key::MouseLeft,
            ads: Key::MouseRight,
            sprint: Key::Shift,
            crouch: Key::C,
            prone: Key::Control,
            jump: Key::Space,
            reload: Key::R,
            swap: Key::Key1,
            melee: Key::V,
            field_computer: Key::F,
            lethal: Key::G,
            tactical: Key::Q,
            map: Key::Tab,
            alt_mode: Key::Key3,
            hammer: Key::B,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClientConfig {
    /// Radians of view rotation per mouse count at 1.0x zoom.
    pub sensitivity: f32,
    /// BO2 `cg_fov`: horizontal field of view at 4:3, in degrees.
    pub cg_fov: f32,
    pub invert_y: bool,
    pub bindings: Bindings,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self { sensitivity: 0.0022, cg_fov: 65.0, invert_y: false, bindings: Bindings::default() }
    }
}

/// Camera transitions: entering swings round behind the General, then moves in to the eyes; leaving
/// is the same move played backwards ([`transition_pose`]). The swing takes the first
/// `TRANSITION_SPLIT` of it.
pub const ENTER_TIME: f32 = 1.5;
/// How long a word from the hammer stays up, and the time between hammer blows while building.
pub const BUILD_NOTE_TIME: f32 = 2.5;
pub const HAMMER_BLOW_TIME: f32 = crate::viewmodel::HAMMER_SWING_TIME;
pub const LEAVE_TIME: f32 = ENTER_TIME;
const TRANSITION_SPLIT: f32 = 0.45;
pub const HITMARKER_TIME: f32 = 0.25;
pub const DEATH_BANNER_TIME: f32 = 3.0;
/// BO2's "+100" kill popup stays up this long; kills inside the window add up.
pub const SCORE_POPUP_TIME: f32 = 1.4;
pub const KILL_POINTS: u32 = 100;
/// Fraction of weapon recoil that permanently moves the aim (the rest recovers).
pub const RECOIL_PERMANENT: f32 = 0.35;

/// Horizontal FOV for a BO2 `cg_fov` (defined at 4:3) at the given aspect ratio (Hor+).
pub fn horizontal_fov(cg_fov_deg: f32, aspect: f32) -> f32 {
    let half_h43 = (cg_fov_deg.to_radians() * 0.5).tan();
    let half_v = half_h43 / (4.0 / 3.0);
    2.0 * (half_v * aspect).atan()
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ClientIn {
    /// Seconds since the previous rendered frame.
    pub dt: f32,
    pub local_player: i32,
    pub logic_frame: u32,
    /// Platform-neutral key-down set (see [`KeySet`]).
    pub keys: KeySet,
    pub mouse_dx: f32,
    pub mouse_dy: f32,
    /// Absolute mouse cursor position (screen pixels), for menus (the lobby class pick).
    pub mouse_x: f32,
    pub mouse_y: f32,
    /// The mouse buttons' held state (see [`KeySet`]).
    pub mouse_buttons: u32,
    /// Wheel notches since last frame.
    pub wheel: f32,
    /// Game window focused and no menu/dialog is up.
    pub input_allowed: i32,
    pub screen_w: f32,
    pub screen_h: f32,
    /// The RTS camera, for transitions.
    pub rts_cam_pos: Vec3,
    pub rts_cam_dir: Vec3,
    /// The RTS camera's horizontal field of view (radians).
    pub rts_hfov: f32,
    /// The point the RTS camera pivots on (on the ground; the camera sits at an offset from it).
    pub rts_pivot: Vec3,
    /// One-shot from the host: the lobby's Create-a-Class button was pressed, so the pick screen
    /// opens (or re-opens) even when a class is already chosen. The bridge latches the shell
    /// button until the first client frame consumes it.
    pub class_pick_force: i32,
}

impl Default for ClientIn {
    fn default() -> Self {
        Self {
            dt: 1.0 / 60.0,
            local_player: 0,
            logic_frame: 0,
            keys: KeySet::default(),
            mouse_dx: 0.0,
            mouse_dy: 0.0,
            mouse_x: 640.0,
            mouse_y: 360.0,
            mouse_buttons: 0,
            wheel: 0.0,
            input_allowed: 1,
            screen_w: 1280.0,
            screen_h: 720.0,
            rts_cam_pos: Vec3::new(0.0, -200.0, 300.0),
            rts_cam_dir: Vec3::new(0.0, 0.6, -0.8),
            rts_hfov: 0.87,
            rts_pivot: Vec3::new(0.0, 0.0, 0.0),
            class_pick_force: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClientOut {
    /// Replace the tactical view's camera with `eye`/`dir`/`hfov` this frame.
    pub camera_override: i32,
    pub eye: Vec3,
    pub dir: Vec3,
    pub hfov: f32,
    /// Lock and hide the cursor and feed relative mouse deltas.
    pub capture_mouse: i32,
    /// Hide the RTS control bar and in-game UI.
    pub hide_rts_ui: i32,
    /// Object whose drawable must be hidden (the local General in first person), 0 = none.
    pub hide_object: u32,
    /// Send `MSG_ZHBO_FPS_INPUT` with `input` for `input_general` this frame.
    pub send_input: i32,
    pub input_general: u32,
    pub input: FpsInput,
    /// One-shot: centre the RTS camera on `rts_look_at` (end of the field computer transition).
    pub rts_look_at_valid: i32,
    pub rts_look_at: Vec3,
    /// First-person viewmodel clip (a BO2 slot, `viewmodel::slot`, or a tablet clip), or
    /// `viewmodel::NONE` to hide it.
    pub viewmodel: i32,
    /// Which converted viewmodel plays it (the weapon's `WeaponDef::viewmodel`).
    pub viewmodel_weapon: i32,
    /// The gun fired this frame (muzzle flash and brass at the viewmodel's tags).
    pub viewmodel_fired: i32,
    /// Seconds into the viewmodel clip.
    pub viewmodel_time: f32,
    /// 0..1: how far the weapon is raised to aim down the sights.
    pub viewmodel_ads: f32,
    /// Sway, bob and recoil: offset from the eye (forward, left, up) and yaw/pitch/roll (radians).
    pub viewmodel_offset: Vec3,
    pub viewmodel_angles: Vec3,
    /// One-shot: the player picked class `choose_class` (an index into `classes.rs`' list, or 0
    /// = none) for `choose_class_general`. When a custom class is picked, `choose_class` is
    /// `i32::MIN` (the custom marker) and `choose_class_payload` carries the wire words
    /// (`classes.rs`' `CustomClass::to_wire_words`); the bridge sends them alongside the index.
    pub choose_class: i32,
    pub choose_class_general: u32,
    /// The custom class's wire words when `choose_class` is the custom marker; unused otherwise.
    pub choose_class_payload: [u32; 64],
    /// How many `choose_class_payload` words are valid (0 for a shipped class pick).
    pub choose_class_payload_len: i32,
}

// Manual Default: `[u32; 64]` has none (arrays above 32 don't), and the rest are plain.
impl Default for ClientOut {
    fn default() -> ClientOut {
        ClientOut {
            camera_override: 0,
            eye: Vec3::default(),
            dir: Vec3::default(),
            hfov: 0.0,
            capture_mouse: 0,
            hide_rts_ui: 0,
            hide_object: 0,
            send_input: 0,
            input_general: 0,
            input: FpsInput::default(),
            rts_look_at_valid: 0,
            rts_look_at: Vec3::default(),
            viewmodel: 0,
            viewmodel_weapon: 0,
            viewmodel_fired: 0,
            viewmodel_time: 0.0,
            viewmodel_ads: 0.0,
            viewmodel_offset: Vec3::default(),
            viewmodel_angles: Vec3::default(),
            choose_class: 0,
            choose_class_general: 0,
            choose_class_payload: [0; 64],
            choose_class_payload_len: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewMode {
    Rts,
    Entering { t: f32, from_pos: Vec3, from_dir: Vec3, from_hfov: f32 },
    Fps,
    /// `to_*`: the overhead camera's pose once it is centred on `target`.
    Leaving { t: f32, from_pos: Vec3, from_dir: Vec3, from_hfov: f32, target: Vec3, to_pos: Vec3, to_dir: Vec3, to_hfov: f32 },
}

/// The in-game test harness's scripted input; release builds can strip it.
#[cfg(feature = "autopilot")]
pub mod autopilot;
#[cfg(feature = "autopilot")]
pub use autopilot::{Autopilot, Scripted};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HudTimers {
    pub hitmarker: f32,
    pub hitmarker_kill: bool,
    pub hitmarker_head: bool,
    pub damage_flash: f32,
    pub death_banner: f32,
    pub view_kick_pitch: f32,
    pub view_kick_yaw: f32,
    pub muzzle: f32,
    pub score_popup: f32,
    pub score_points: u32,
    /// The map key is held: Zero Hour's radar over the view.
    pub map_open: bool,
    /// Black Hat progress (0..1) while hacking, and how long it stays up without news.
    pub hack_progress: f32,
    pub hack_shown: f32,
    /// Building at a site: its percent, how long the bar stays up without news, and the next
    /// hammer blow's sound.
    pub build_percent: f32,
    pub build_shown: f32,
    pub build_blow: f32,
    /// A word from the hammer ("Not enough money") and how long it stays.
    pub build_note: &'static str,
    pub build_note_time: f32,
    /// The class pick screen has made its entrance this life: its open sound plays once.
    pub class_pick_seen: bool,
}

/// A camera circling a point at a fixed distance and height (the overhead camera's, around the
/// General).
#[derive(Clone, Copy, Debug)]
struct Orbit {
    focus: Vec3,
    distance: f32,
    height: f32,
    azimuth: f32,
}

impl Orbit {
    /// The orbit through `pos` around `focus`.
    fn around(focus: Vec3, pos: Vec3) -> Orbit {
        let d = pos - focus;
        Orbit { focus, distance: d.length_xy().max(20.0), height: d.z.max(20.0), azimuth: d.y.atan2(d.x) }
    }

    fn at(&self, azimuth: f32) -> Vec3 {
        self.focus + Vec3::new(azimuth.cos() * self.distance, azimuth.sin() * self.distance, self.height)
    }
}

/// A camera pose: position, view direction, horizontal field of view.
type Pose = (Vec3, Vec3, f32);

/// The camera `k` of the way from the overhead view (0) to the General's eyes (1): swing round the
/// eyes, at the overhead camera's distance and height, to behind the General's view (`behind` is that
/// azimuth), turning to look at them; then move in, turning to the view. Entering plays this forwards
/// and leaving backwards, so the two are the same move.
fn transition_pose(k: f32, overhead: Pose, eyes: Pose, behind: f32) -> Pose {
    let (over_pos, over_dir, over_hfov) = overhead;
    let (eye, view_dir, eye_hfov) = eyes;
    let orbit = Orbit::around(eye, over_pos);
    let (pos, dir) = if k < TRANSITION_SPLIT {
        let s = smoothstep(k / TRANSITION_SPLIT);
        let pos = orbit.at(lerp_angle(orbit.azimuth, behind, s));
        (pos, over_dir.lerp((eye - pos).normalized_or_zero(), s))
    } else {
        let s = smoothstep((k - TRANSITION_SPLIT) / (1.0 - TRANSITION_SPLIT));
        let start = orbit.at(behind);
        (start.lerp(eye, s), (eye - start).normalized_or_zero().lerp(view_dir, s))
    };
    (pos, dir.normalized_or_zero(), over_hfov + (eye_hfov - over_hfov) * smoothstep(k))
}

pub struct Client {
    pub cfg: ClientConfig,
    #[cfg(feature = "autopilot")]
    pub autopilot: Option<autopilot::Autopilot>,
    /// Stripped with the `autopilot` feature: a no-op so the client struct's shape is stable.
    #[cfg(not(feature = "autopilot"))]
    pub autopilot: Option<()>,
    pub view: ViewMode,
    pub yaw: f32,
    pub pitch: f32,
    pub general: u32,
    pub hud: HudTimers,
    last_sent_frame: Option<u32>,
    latch: u32,
    prev_held: u32,
    /// The previous frame's raw key set: edge detection for keys outside the button bindings
    /// (the class pick's number keys).
    prev_keys: KeySet,
    eye_prev: Vec3,
    eye_curr: Vec3,
    seen_frame: u32,
    since_logic: f32,
    pub last_hfov: f32,
    /// The overhead camera as first person found it: its offset from its pivot, its direction and
    /// field of view. Leaving flies back to it, centred on the General (while first person owns the
    /// camera, the view reports the first-person camera, not the overhead one).
    overhead: Option<(Vec3, Vec3, f32)>,
    /// The first-person camera this frame (for HUD marks over the world).
    pub view_eye: Vec3,
    pub view_dir: Vec3,
    /// First-person viewmodel animation, and this frame's weapon events for it.
    vm: Viewmodel,
    vm_motion: viewmodel::Motion,
    vm_events: viewmodel::Events,
    /// BO2 sound cues queued this frame (drained by the FFI layer into the host).
    pub cues: Cues,
    /// The open create-a-class editor (BO2's Pick-10 screen), or `None` when it is closed. Opened
    /// from the class pick screen's CREATE A CLASS row; on save its class becomes the pick, on
    /// cancel the pick screen returns.
    pub class_editor: Option<crate::classmenu::Editor>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new(ClientConfig::default())
    }
}

impl Client {
    pub fn new(cfg: ClientConfig) -> Self {
        Self {
            cfg,
            #[cfg(feature = "autopilot")]
            autopilot: None,
            #[cfg(not(feature = "autopilot"))]
            autopilot: None,
            view: ViewMode::Rts,
            yaw: 0.0,
            pitch: 0.0,
            general: 0,
            hud: HudTimers::default(),
            last_sent_frame: None,
            latch: 0,
            prev_held: 0,
            prev_keys: KeySet::default(),
            eye_prev: Vec3::ZERO,
            eye_curr: Vec3::ZERO,
            seen_frame: 0,
            since_logic: 0.0,
            last_hfov: 1.0,
            overhead: None,
            view_eye: Vec3::ZERO,
            view_dir: Vec3::new(1.0, 0.0, 0.0),
            vm: Viewmodel::default(),
            vm_motion: viewmodel::Motion::default(),
            vm_events: viewmodel::Events::default(),
            cues: Cues::default(),
            class_editor: None,
        }
    }

    pub fn reset(&mut self) {
        #[cfg(feature = "autopilot")]
        let autopilot =
            self.autopilot.as_ref().map(|a| Autopilot { equipment: a.equipment, build: a.build, classpick: a.classpick, ..Default::default() });
        *self = Client::new(self.cfg);
        #[cfg(feature = "autopilot")]
        {
            self.autopilot = autopilot;
        }
    }

    fn read_buttons(&self, inp: &ClientIn) -> (u32, f32, f32) {
        let b = &self.cfg.bindings;
        let down = |k: Key| inp.keys.is_down(k);
        let mut held = 0;
        for (key, bit) in [
            (b.fire, buttons::FIRE),
            (b.ads, buttons::ADS),
            (b.sprint, buttons::SPRINT),
            (b.crouch, buttons::CROUCH),
            (b.prone, buttons::PRONE),
            (b.jump, buttons::JUMP),
            (b.reload, buttons::RELOAD),
            (b.swap, buttons::SWAP_WEAPON),
            (b.melee, buttons::MELEE),
            (b.field_computer, buttons::FIELD_COMPUTER),
            (b.lethal, buttons::LETHAL),
            (b.tactical, buttons::TACTICAL),
            (b.alt_mode, buttons::ALT_MODE),
            (b.hammer, buttons::HAMMER),
        ] {
            if down(key) {
                held |= bit;
            }
        }
        let fwd = (down(b.forward) as i32 - down(b.back) as i32) as f32;
        let right = (down(b.right) as i32 - down(b.left) as i32) as f32;
        (held, fwd, right)
    }

/// The class pick's keys: a fresh press of 1-5 picks that entry of the class list (the list is
/// shared, so the index is deterministic on every client; the five basics fill the keys); a fresh
/// press of 6 opens the create-a-class editor (BO2's row below the presets). One-shot: the value
/// stays 0 otherwise, and the bridge turns it into `MSG_ZHBO_CHOOSE_CLASS`.
fn class_pick_input(inp: &ClientIn, prev: &KeySet, out: &mut ClientOut, general: u32) -> bool {
    const PICK_KEYS: [Key; 5] = [Key::Key1, Key::Key2, Key::Key3, Key::Key4, Key::Key5];
    for (i, key) in PICK_KEYS.iter().enumerate() {
        if inp.keys.is_down(*key) && !prev.is_down(*key) {
            out.choose_class = i as i32 + 1;
            out.choose_class_general = general;
            return false;
        }
    }
    // Entry 6 is the editor: it opens instead of picking.
    inp.keys.is_down(Key::Key6) && !prev.is_down(Key::Key6)
}

/// `choose_class`'s marker for a class built in the create-a-class editor: the payload words carry
/// its spec instead of a registry index.
const CHOOSE_CLASS_CUSTOM: i32 = i32::MIN;

/// Fill a custom-class pick into the outgoing one-shot: the marker in `choose_class`, the class's
/// wire words in the payload. The bridge turns it into `MSG_ZHBO_CHOOSE_CLASS` and the sim
/// reassembles the spec ([`crate::classes::CustomClass::from_wire_words`]).
fn pick_custom_class(out: &mut ClientOut, general: u32, class: &crate::classes::CustomClass) {
    out.choose_class = Self::CHOOSE_CLASS_CUSTOM;
    out.choose_class_general = general;
    let words = class.to_wire_words();
    let n = words.len().min(out.choose_class_payload.len());
    out.choose_class_payload[..n].copy_from_slice(&words[..n]);
    out.choose_class_payload_len = n as i32;
}

/// Run one rendered frame.
pub fn frame(&mut self, sim: &mut Sim, inp: &ClientIn) -> ClientOut {
        let dt = inp.dt.clamp(0.0, 0.25);
        #[allow(unused_mut)]
        let mut scripted = ClientIn::default();
        let inp = match self.autopilot.as_mut() {
            #[cfg(feature = "autopilot")]
            Some(ap) if matches!(self.view, ViewMode::Fps) => {
                // The scripts' time windows are tuned in logic ticks (the sim's), so read the
                // clock from the logic frame, not the render dt: hitches and frame-rate
                // differences would drift every window relative to the state it aims at.
                ap.t = (inp.logic_frame - ap.base_frame) as f32 * LOGIC_DT;
                let script = ap.input_at(&self.cfg.bindings);
                scripted = *inp;
                scripted.keys = script.keys;
                scripted.mouse_dx = script.dx;
                scripted.mouse_dy = script.dy;
                scripted.wheel = script.wheel;
                scripted.input_allowed = 1;
                &scripted
            }
            #[cfg(feature = "autopilot")]
            Some(ap) => {
                ap.t = 0.0;
                ap.base_frame = inp.logic_frame;
                inp
            }
            #[cfg(not(feature = "autopilot"))]
            Some(_) => inp,
            None => inp,
        };
        let mut out = ClientOut { hfov: self.last_hfov, viewmodel: viewmodel::NONE, ..Default::default() };
        self.hud.map_open = false;
        let player = sim.player(inp.local_player).cloned();

        // Track logic frames for eye interpolation.
        if let Some(p) = &player {
            if inp.logic_frame != self.seen_frame {
                self.seen_frame = inp.logic_frame;
                self.since_logic = 0.0;
                self.eye_prev = self.eye_curr;
                self.eye_curr = p.body.eye();
            } else {
                self.since_logic += dt;
            }
        }
        let alpha = (self.since_logic / LOGIC_DT).clamp(0.0, 1.0);
        let eye = self.eye_prev.lerp(self.eye_curr, alpha);

        self.consume_events(sim, inp, player.as_ref().map(|p| p.body.pos));
        self.tick_hud(dt);

        // The class pick's entrance: its UI sound once per life while the class is unset (the
        // screen draws over the view every frame, so the latch lives here with the other timers).
        if let Some(p) = &player {
            if p.class.is_none() {
                if !self.hud.class_pick_seen {
                    self.hud.class_pick_seen = true;
                    self.cues.play(sounds::UI_SELECT);
                }
            } else {
                self.hud.class_pick_seen = false;
            }
        }

        let aspect = if inp.screen_h > 0.0 { inp.screen_w / inp.screen_h } else { 16.0 / 9.0 };
        let base_fov = horizontal_fov(self.cfg.cg_fov, aspect);
        let ads = player.as_ref().map_or(0.0, |p| p.arsenal.ads);
        let ads_scale = player.as_ref().map_or(1.0, |p| p.arsenal.ads_fov_scale());
        let hfov = base_fov * (1.0 + (ads_scale - 1.0) * ads);
        self.last_hfov = hfov;

        let first_person = player.as_ref().is_some_and(|p| p.is_first_person());
        let allowed = inp.input_allowed != 0;

        match self.view {
            ViewMode::Rts => {
                if first_person {
                    let p = player.as_ref().unwrap();
                    self.start_entering(inp, p.general, p.yaw, p.body.eye());
                }
            }
            ViewMode::Entering { t, from_pos, from_dir, from_hfov } => {
                let t = t + dt;
                let u = (t / ENTER_TIME).min(1.0);
                let behind = self.yaw + std::f32::consts::PI;
                let eyes = (eye, dir_from_angles(self.yaw, self.pitch), hfov);
                let (pos, dir, fov) = transition_pose(u, (from_pos, from_dir, from_hfov), eyes, behind);
                out.eye = pos;
                out.dir = dir;
                out.camera_override = 1;
                out.hfov = fov;
                out.hide_rts_ui = 1;
                if u > 0.9 {
                    out.hide_object = self.general;
                }
                self.view = if t >= ENTER_TIME { ViewMode::Fps } else { ViewMode::Entering { t, from_pos, from_dir, from_hfov } };
            }
            ViewMode::Fps => {
                if !first_person {
                    // Released without a ReturnedToRts event (e.g. death handled in consume_events).
                    self.view = ViewMode::Rts;
                } else {
                    let p = player.as_ref().unwrap();
                    let in_fc = matches!(p.mode, Mode::FieldComputer { .. });
                    if allowed && !in_fc {
                        let zoom = hfov / base_fov;
                        let sens = self.cfg.sensitivity * zoom;
                        let inv = if self.cfg.invert_y { -1.0 } else { 1.0 };
                        self.yaw = wrap_angle(self.yaw - inp.mouse_dx * sens);
                        self.pitch = (self.pitch - inp.mouse_dy * sens * inv).clamp(-MAX_PITCH, MAX_PITCH);
                    }
                    out.camera_override = 1;
                    out.eye = eye;
                    let kick_pitch = self.hud.view_kick_pitch.to_radians();
                    let kick_yaw = self.hud.view_kick_yaw.to_radians();
                    out.dir = dir_from_angles(self.yaw + kick_yaw, (self.pitch + kick_pitch).clamp(-MAX_PITCH, MAX_PITCH));
                    out.hfov = hfov;
                    out.hide_rts_ui = 1;
                    out.hide_object = self.general;
                    out.capture_mouse = allowed as i32;
                    self.view_eye = out.eye;
                    self.view_dir = out.dir;
                    self.hud.map_open = allowed && !in_fc && inp.keys.is_down(self.cfg.bindings.map);
                    let events = std::mem::take(&mut self.vm_events);
                    let pose = self.vm_motion.update(dt, self.yaw, self.pitch, p, events.shot);
                    out.viewmodel_offset = pose.offset;
                    out.viewmodel_angles = pose.angles;
                    self.cues.update(dt, p, p.input.held & buttons::FIRE != 0);
                    if let Mode::FieldComputer { t } = p.mode {
                        // A scorestreak: the tablet comes up before the view leaves.
                        let (clip, time) = viewmodel::tablet_up(t);
                        out.viewmodel = clip;
                        out.viewmodel_time = time;
                    } else {
                        out.viewmodel_fired = events.shot as i32;
                        let f = self.vm.update(dt, p, events);
                        out.viewmodel = f.clip;
                        out.viewmodel_weapon = f.model;
                        out.viewmodel_time = f.t;
                        out.viewmodel_ads = p.arsenal.ads;
                    }
                    self.build_input(inp, &mut out, allowed && !in_fc, p.hammer.is_some(), p.class.is_none() || inp.class_pick_force != 0);
                }
            }
            ViewMode::Leaving { t, from_pos, from_dir, from_hfov, target, to_pos, to_dir, to_hfov } => {
                let t = t + dt;
                let u = (t / LEAVE_TIME).min(1.0);
                // Entering backwards: from the eyes (where first person left off) to the overhead
                // camera's own pose, centred on the General.
                let behind = self.yaw + std::f32::consts::PI;
                let (pos, dir, fov) = transition_pose(1.0 - u, (to_pos, to_dir, to_hfov), (from_pos, from_dir, from_hfov), behind);
                out.eye = pos;
                out.dir = dir;
                out.camera_override = 1;
                out.hfov = fov;
                // The General stays hidden while the camera is still inside the head (as entering).
                if u < 0.1 {
                    out.hide_object = self.general;
                }
                if t >= LEAVE_TIME {
                    self.view = ViewMode::Rts;
                    out.camera_override = 0;
                    out.rts_look_at_valid = 1;
                    out.rts_look_at = target;
                } else {
                    self.view = ViewMode::Leaving { t, from_pos, from_dir, from_hfov, target, to_pos, to_dir, to_hfov };
                }
            }
        }
        // The fresh-press detectors (the class pick) compare against this frame's keys.
        self.prev_keys = inp.keys;
        out
    }

    fn start_entering(&mut self, inp: &ClientIn, general: u32, yaw: f32, eye: Vec3) {
        self.general = general;
        self.vm = Viewmodel::default(); // the weapon comes up as first person starts
        self.vm_motion = viewmodel::Motion::default();
        self.cues.reset();
        self.yaw = yaw;
        self.pitch = 0.0;
        self.eye_prev = eye;
        self.eye_curr = eye;
        self.latch = 0;
        self.prev_held = u32::MAX; // ignore keys already held when entering (e.g. the click itself)
        let from_dir = if inp.rts_cam_dir.length() > 0.5 { inp.rts_cam_dir } else { Vec3::new(0.0, 0.6, -0.8) };
        let from_hfov = if inp.rts_hfov > 0.05 { inp.rts_hfov } else { self.last_hfov };
        self.overhead = Some((inp.rts_cam_pos - inp.rts_pivot, from_dir.normalized_or_zero(), from_hfov));
        self.view = ViewMode::Entering { t: 0.0, from_pos: inp.rts_cam_pos, from_dir: from_dir.normalized_or_zero(), from_hfov };
    }

    fn build_input(&mut self, inp: &ClientIn, out: &mut ClientOut, allowed: bool, hammer: bool, class_pick: bool) {
        let (mut held, mut fwd, mut right) = if allowed { self.read_buttons(inp) } else { (0, 0.0, 0.0) };
        if !allowed {
            fwd = 0.0;
            right = 0.0;
        }
        // The create-a-class editor, when open, owns the input entirely: the arrows move its
        // highlight, Enter confirms and Escape backs out. While it is up nothing else reads the
        // keys this frame.
        if self.class_editor.is_some() {
            let edges = crate::classmenu::Edges::between(&self.prev_keys, &inp.keys);
            let mut editor = self.class_editor.take().expect("editor is open");
            let outcome = editor.step(edges);
            match outcome {
                crate::classmenu::Outcome::Continue => {
                    self.class_editor = Some(editor);
                }
                crate::classmenu::Outcome::Save if editor.draft.valid() => {
                    let class = editor.draft.to_custom();
                    // Persist it into the profile first (the pick screen lists the profile's classes,
                    // so the just-built one shows next time), then pick it: the bridge sends the
                    // choose message and writes the profile file. The profile assigns a default
                    // name to a blank one, so read the saved entry back and send that: the pick and
                    // the profile then carry the same named class.
                    crate::profile::save(class);
                    let class = crate::profile::classes().last().cloned().expect("just saved");
                    Self::pick_custom_class(out, self.general, &class);
                    self.cues.play(sounds::PLANT);
                    self.prev_held = u32::MAX;
                }
                crate::classmenu::Outcome::Save => {
                    // An over-spent class cannot be saved; the editor stays open.
                    self.class_editor = Some(editor);
                }
                crate::classmenu::Outcome::Cancel => {
                    self.cues.play(sounds::UI_SELECT);
                    self.prev_held = u32::MAX;
                }
            }
            return;
        }
        // The pick menu takes the number keys while it is up, even when the game has locked
        // input (it must not eat them).
        if class_pick {
            let open_editor = Self::class_pick_input(inp, &self.prev_keys, out, self.general);
            if open_editor {
                // Open on the most recently saved custom class when there is one (so the player can
                // re-edit it), otherwise on BO2's default class. BO2 opens a fresh slot; ours keeps
                // the last class so a player without naming gets their kit back.
                let draft = crate::profile::classes()
                    .last()
                    .map(crate::classmenu::Draft::from_custom)
                    .unwrap_or_default();
                self.class_editor = Some(crate::classmenu::Editor::new(draft));
                self.cues.play(sounds::UI_SELECT);
                // The opening key's edge must not reach the editor on the same frame.
                self.prev_keys = inp.keys;
                return;
            }
        }
        if allowed && inp.wheel.abs() >= 1.0 {
            // With the hammer up the wheel turns the structure being placed (wheel up: to the left).
            self.latch |= match (hammer, inp.wheel > 0.0) {
                (false, _) => buttons::SWAP_WEAPON,
                (true, true) => buttons::ROTATE_LEFT,
                (true, false) => buttons::ROTATE_RIGHT,
            };
        }
        if self.prev_held == u32::MAX {
            // Swallow keys that were already down on entry until they are released.
            self.prev_held = held;
            held = 0;
        }
        self.latch |= held & !self.prev_held;
        self.prev_held = held;
        if self.last_sent_frame != Some(inp.logic_frame) {
            self.last_sent_frame = Some(inp.logic_frame);
            out.send_input = 1;
            out.input_general = self.general;
            out.input = FpsInput {
                held,
                pressed: self.latch,
                move_packed: pack_move(fwd, right),
                yaw: self.yaw,
                pitch: self.pitch,
            };
            self.latch = 0;
        }
    }

    fn consume_events(&mut self, sim: &mut Sim, inp: &ClientIn, body_pos: Option<Vec3>) {
        let me = inp.local_player;
        let mine: Vec<EventKind> = sim.events.iter().filter(|e| e.player == me).map(|e| e.kind).collect();
        sim.events.clear();
        // A shot's impact is flesh if the same frame scored a hit.
        let hit = mine.iter().any(|k| matches!(k, EventKind::HitMarker { .. }));
        let fire = sim.player(me).map_or(sounds::FIRE, |p| sounds::fire_for(p.arsenal.current().def));
        for kind in &mine {
            if let EventKind::Shot { end, .. } = kind {
                self.cues.play(fire);
                self.cues.play_at(if hit { sounds::IMPACT_FLESH } else { sounds::IMPACT }, *end);
            }
        }
        for kind in mine {
            match kind {
                EventKind::EnteredFps { general } => {
                    if let Some(p) = sim.player(me) {
                        self.start_entering(inp, general, p.yaw, p.body.eye());
                    }
                }
                EventKind::ReturnedToRts { general } => {
                    self.general = general;
                    self.begin_leaving(inp, body_pos.unwrap_or(self.eye_curr));
                }
                EventKind::GeneralDied { .. } => {
                    self.hud.death_banner = DEATH_BANNER_TIME;
                    if !matches!(self.view, ViewMode::Rts) {
                        self.begin_leaving(inp, self.eye_curr - Vec3::new(0.0, 0.0, 10.0));
                    }
                }
                EventKind::HitMarker { headshot, kill } => {
                    self.cues.play(sounds::HIT_MARKER);
                    self.hud.hitmarker = HITMARKER_TIME;
                    self.hud.hitmarker_kill = kill;
                    self.hud.hitmarker_head = headshot;
                    if kill {
                        if self.hud.score_popup <= 0.0 {
                            self.hud.score_points = 0;
                        }
                        self.hud.score_points += KILL_POINTS;
                        self.hud.score_popup = SCORE_POPUP_TIME;
                    }
                }
                EventKind::Recoil { pitch_deg, yaw_deg } => {
                    // BO2 assault rifles mostly recover: part of each kick moves the real aim, the
                    // rest is a visual kick that settles back.
                    self.pitch = (self.pitch + (pitch_deg * RECOIL_PERMANENT).to_radians()).clamp(-MAX_PITCH, MAX_PITCH);
                    self.yaw = wrap_angle(self.yaw + (yaw_deg * RECOIL_PERMANENT).to_radians());
                    self.hud.view_kick_pitch += pitch_deg * (1.0 - RECOIL_PERMANENT);
                    self.hud.view_kick_yaw += yaw_deg * (1.0 - RECOIL_PERMANENT);
                    self.hud.muzzle = 0.05;
                }
                EventKind::Damaged { amount } => {
                    self.cues.play(sounds::HURT);
                    self.hud.damage_flash = (self.hud.damage_flash + amount / 60.0).min(1.0);
                }
                EventKind::Shot { .. } => self.vm_events.shot = true,
                EventKind::ReloadStarted => self.vm_events.reload_started = true,
                EventKind::Swapped => self.vm_events.swapped = true,
                EventKind::FieldComputerOpened => {}
                EventKind::ClassChosen { .. } => self.cues.play(sounds::PLANT),
                EventKind::Melee => self.cues.play(sounds::MELEE_SWING),
                EventKind::OffhandReady { .. } => self.cues.play(sounds::PULL_PIN),
                EventKind::Thrown { .. } => self.cues.play(sounds::THROW),
                EventKind::Launched => self.cues.play(sounds::FIRE_LAUNCHER),
                EventKind::Exploded { at } => self.cues.play_at(sounds::EXPLOSION, at),
                EventKind::Intercepted { at } => self.cues.play_at(sounds::EXPLOSION, at),
                EventKind::Hacking { progress } => {
                    self.hud.hack_progress = progress;
                    self.hud.hack_shown = 0.15;
                }
                EventKind::Hacked => self.hud.hack_shown = 0.0,
                EventKind::AltToggled => self.vm_events.swapped = true,
                EventKind::Planted { kind } => {
                    use crate::equipment::Kind;
                    if matches!(kind, Kind::Claymore | Kind::Betty | Kind::Trophy | Kind::Insertion) {
                        self.cues.play(sounds::PLANT);
                    }
                }
                EventKind::Detonated => self.cues.play(sounds::DETONATE),
                EventKind::DestroyedEquipment => {}
                EventKind::Hammer { .. } => self.vm_events.swapped = true,
                EventKind::BuildPlaced { .. } => {
                    self.cues.play(sounds::PLANT);
                    self.note("Structure placed: hold [F] at it to build");
                }
                EventKind::BuildRefused { status } => {
                    self.cues.play(sounds::DRY_FIRE);
                    self.note(match status {
                        crate::building::status::NO_MONEY => "Not enough money",
                        crate::building::status::LOCKED => "Not available yet",
                        crate::building::status::BUSY => "Someone else is repairing it",
                        _ => "Can't build there",
                    });
                }
                EventKind::Working { percent, .. } => {
                    self.hud.build_percent = percent;
                    if self.hud.build_shown <= 0.0 {
                        self.hud.build_blow = 0.0;
                    }
                    self.hud.build_shown = 0.15;
                }
                EventKind::WorkDone { kind } => {
                    self.hud.build_shown = 0.0;
                    self.cues.play(sounds::PLANT);
                    self.note(crate::building::work_text(kind).done);
                }
            }
        }
    }

    fn begin_leaving(&mut self, inp: &ClientIn, target: Vec3) {
        let from_pos = self.eye_curr;
        let from_dir = dir_from_angles(self.yaw, self.pitch);
        // Where the overhead camera will be once centred on the General: as it was when first
        // person started (its angle, height and zoom), only its pivot moved to the General.
        let (to_pos, dir, to_hfov) = match self.overhead {
            Some((offset, dir, hfov)) => (target + offset, dir, hfov),
            None => {
                let dir = if inp.rts_cam_dir.length() > 0.5 { inp.rts_cam_dir.normalized_or_zero() } else { Vec3::new(0.0, 0.6, -0.8) };
                let to_hfov = if inp.rts_hfov > 0.05 { inp.rts_hfov } else { self.last_hfov };
                (target - dir * 400.0, dir, to_hfov)
            }
        };
        self.view = ViewMode::Leaving { t: 0.0, from_pos, from_dir, from_hfov: self.last_hfov, target, to_pos, to_dir: dir, to_hfov };
        self.last_sent_frame = None;
    }

    fn note(&mut self, text: &'static str) {
        self.hud.build_note = text;
        self.hud.build_note_time = BUILD_NOTE_TIME;
    }

    fn tick_hud(&mut self, dt: f32) {
        if self.hud.build_shown > 0.0 {
            // A hammer blow every so often while building.
            self.hud.build_blow -= dt;
            if self.hud.build_blow <= 0.0 {
                self.hud.build_blow = HAMMER_BLOW_TIME;
                self.cues.play(sounds::MELEE_SWING);
            }
        }
        let h = &mut self.hud;
        h.build_shown = (h.build_shown - dt).max(0.0);
        h.build_note_time = (h.build_note_time - dt).max(0.0);
        h.hitmarker = (h.hitmarker - dt).max(0.0);
        h.damage_flash = (h.damage_flash - dt * 0.6).max(0.0);
        h.death_banner = (h.death_banner - dt).max(0.0);
        h.muzzle = (h.muzzle - dt).max(0.0);
        h.score_popup = (h.score_popup - dt).max(0.0);
        h.hack_shown = (h.hack_shown - dt).max(0.0);
        let decay = (1.0 - dt * 12.0).max(0.0);
        h.view_kick_pitch *= decay;
        h.view_kick_yaw *= decay;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::tests::{general, FakeHost};

    fn frame_with(client: &mut Client, sim: &mut Sim, host: &FakeHost, inp: &mut ClientIn, steps: usize) -> ClientOut {
        let mut out = ClientOut::default();
        for _ in 0..steps {
            out = client.frame(sim, inp);
            if out.send_input != 0 {
                sim.input(inp.local_player, out.input_general, out.input);
            }
            inp.dt = LOGIC_DT;
            let f = sim.frame + 1;
            sim.step(host, f);
            inp.logic_frame = f;
        }
        out
    }

    #[test]
    fn enter_transition_then_capture_and_send_one_input_per_logic_frame() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn::default();
        sim.take_control(&host, 0, 1);

        let out = client.frame(&mut sim, &inp);
        assert_eq!(out.camera_override, 1);
        assert!(matches!(client.view, ViewMode::Entering { .. }));

        let out = frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!(client.view, ViewMode::Fps);
        assert_eq!(out.capture_mouse, 1);
        assert_eq!(out.hide_object, 1);
        assert_eq!(out.hide_rts_ui, 1);

        // Same logic frame twice: only the first render frame sends.
        let a = client.frame(&mut sim, &inp);
        let b = client.frame(&mut sim, &inp);
        assert_eq!((a.send_input, b.send_input), (1, 0));
    }

    #[test]
    fn mouse_right_turns_right_and_w_walks_forward() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn::default();
        sim.take_control(&host, 0, 1);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        inp.mouse_dx = 100.0;
        frame_with(&mut client, &mut sim, &host, &mut inp, 1);
        assert!(client.yaw < 0.0, "turned clockwise (right): {}", client.yaw);
        inp.mouse_dx = 0.0;
        let yaw = client.yaw;
        inp.keys.set_down(Key::W, true);
        frame_with(&mut client, &mut sim, &host, &mut inp, 30);
        let pos = host.get(1).pos;
        let heading = pos.y.atan2(pos.x);
        assert!((heading - yaw).abs() < 0.05, "walked along view yaw {yaw}, heading {heading}");
    }

    #[test]
    fn holding_tab_opens_the_map() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn::default();
        inp.input_allowed = 1;
        sim.take_control(&host, 0, 1);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert!(!client.hud.map_open);
        inp.keys.set_down(Key::Tab, true);
        frame_with(&mut client, &mut sim, &host, &mut inp, 1);
        assert!(client.hud.map_open);
        inp.keys.set_down(Key::Tab, false);
        frame_with(&mut client, &mut sim, &host, &mut inp, 1);
        assert!(!client.hud.map_open);
    }

    #[test]
    fn field_computer_key_leaves_first_person() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn::default();
        sim.take_control(&host, 0, 1);
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        inp.keys.set_down(Key::F, true);
        frame_with(&mut client, &mut sim, &host, &mut inp, 2);
        inp.keys.set_down(Key::F, false);
        frame_with(&mut client, &mut sim, &host, &mut inp, (crate::sim::FIELD_COMPUTER_RAISE / LOGIC_DT) as usize + 5);
        assert!(matches!(client.view, ViewMode::Leaving { .. }), "{:?}", client.view);
        let mut look = None;
        for _ in 0..120 {
            let out = frame_with(&mut client, &mut sim, &host, &mut inp, 1);
            if out.rts_look_at_valid != 0 {
                look = Some(out.rts_look_at);
            }
        }
        assert_eq!(client.view, ViewMode::Rts);
        assert!(look.is_some(), "RTS camera recentred on the General");
    }

    /// Entering and leaving first person never jump: every frame the camera moves a little, starting
    /// from (and ending on) the overhead camera's pose and field of view.
    #[test]
    fn transitions_are_smooth_both_ways() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { rts_cam_pos: Vec3::new(-150.0, 60.0, 220.0), rts_hfov: 0.8, ..Default::default() };
        let rts_dir = (Vec3::new(0.0, 0.0, 0.0) - inp.rts_cam_pos).normalized_or_zero();
        inp.rts_cam_dir = rts_dir;
        sim.take_control(&host, 0, 1);
        let mut outs = Vec::new();
        for _ in 0..200 {
            let out = frame_with(&mut client, &mut sim, &host, &mut inp, 1);
            if matches!(client.view, ViewMode::Entering { .. } | ViewMode::Fps) {
                outs.push(out);
            }
            if client.view == ViewMode::Fps && outs.len() > 2 {
                break;
            }
        }
        // While first person owns the camera the view reports the first-person camera, not the
        // overhead one: leaving must not aim for that.
        let overhead_pos = inp.rts_cam_pos;
        inp.rts_cam_pos = client.view_eye;
        inp.rts_cam_dir = client.view_dir;
        inp.rts_hfov = 1.3;
        inp.rts_pivot = client.view_eye + client.view_dir * 100.0;
        // Leave through the field computer.
        inp.keys.set_down(Key::F, true);
        frame_with(&mut client, &mut sim, &host, &mut inp, 2);
        inp.keys.set_down(Key::F, false);
        let mut leaving = Vec::new();
        for _ in 0..400 {
            let out = frame_with(&mut client, &mut sim, &host, &mut inp, 1);
            if out.camera_override != 0 && matches!(client.view, ViewMode::Leaving { .. }) {
                leaving.push(out);
            }
            if client.view == ViewMode::Rts && !leaving.is_empty() {
                break;
            }
        }
        let check = |outs: &[ClientOut], what: &str| {
            for w in outs.windows(2) {
                let (a, b) = (&w[0], &w[1]);
                assert!((a.eye - b.eye).length() < 25.0, "{what}: eye jumped {:?} -> {:?}", a.eye, b.eye);
                assert!(a.dir.dot(b.dir) > 0.97, "{what}: view turned too fast {:?} -> {:?}", a.dir, b.dir);
                assert!((a.hfov - b.hfov).abs() < 0.05, "{what}: fov jumped {} -> {}", a.hfov, b.hfov);
            }
        };
        check(&outs, "entering");
        check(&leaving, "leaving");
        let first = outs[0];
        assert!((first.eye - overhead_pos).length() < 15.0 && first.dir.dot(rts_dir) > 0.97, "starts at the overhead camera");
        assert!((first.hfov - 0.8).abs() < 0.05);
        let last = leaving.last().unwrap();
        assert!(last.dir.dot(rts_dir) > 0.995, "ends looking the overhead camera's way: {:?}", last.dir);
        assert!((last.hfov - 0.8).abs() < 0.02, "ends at the overhead field of view: {}", last.hfov);
        // The overhead camera pivoted on the origin when first person started; it comes back at the
        // same offset from the General (who has not moved), so nothing snaps when it takes over.
        let ViewMode::Rts = client.view else { panic!("not back overhead") };
        let general_at = host.objects.borrow().iter().find(|o| o.id == 1).unwrap().pos;
        let expected = general_at + overhead_pos;
        assert!((last.eye - expected).length() < 8.0, "ends where the overhead camera will be: {:?} vs {:?}", last.eye, expected);
    }

    /// The create-a-class editor's full flow from the pick screen: `6` opens it, Enter on the
    /// Primary slot opens its list, a Down+Enter picks the next weapon, and Enter on the slot row
    /// saves — the outgoing one-shot then carries the custom marker and the class's wire words, and
    /// the profile keeps the class.
    #[test]
    fn key6_opens_the_editor_and_saving_picks_a_custom_class() {
        crate::profile::reset();
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        let mut inp = ClientIn { input_allowed: 1, ..Default::default() };
        sim.take_control(&host, 0, 1);

        // Reach first person first: the pick screen only listens there.
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!(client.view, ViewMode::Fps);

        // Open the editor: `6` on the pick screen (the class is unset, so the pick screen is up).
        inp.keys.set_down(Key::Key6, true);
        client.frame(&mut sim, &inp);
        inp.keys.set_down(Key::Key6, false);
        client.frame(&mut sim, &inp);
        assert!(client.class_editor.is_some(), "6 opens the editor");

        // Open the Primary slot's list, then pick the next weapon down the list.
        inp.keys.set_down(Key::Enter, true);
        client.frame(&mut sim, &inp);
        inp.keys.set_down(Key::Enter, false);
        client.frame(&mut sim, &inp);
        inp.keys.set_down(Key::Down, true);
        client.frame(&mut sim, &inp);
        inp.keys.set_down(Key::Down, false);
        client.frame(&mut sim, &inp);
        inp.keys.set_down(Key::Enter, true);
        client.frame(&mut sim, &inp);
        inp.keys.set_down(Key::Enter, false);
        client.frame(&mut sim, &inp);
        let edited = client.class_editor.as_ref().expect("still open").draft.primary.clone();
        assert_ne!(edited, crate::classmenu::Draft::default().primary, "the weapon changed");

        // Walk down the slot row to the Save row (the primary is first, Save is last of eleven).
        for _ in 0..(crate::classmenu::Slot::ALL.len() - 2) {
            inp.keys.set_down(Key::Down, true);
            client.frame(&mut sim, &inp);
            inp.keys.set_down(Key::Down, false);
            client.frame(&mut sim, &inp);
        }
        assert_eq!(client.class_editor.as_ref().expect("still open").slot, crate::classmenu::Slot::Save);
        // Confirm the Save row: the editor closes and the outgoing one-shot carries the class.
        inp.keys.set_down(Key::Enter, true);
        let out = client.frame(&mut sim, &inp);
        inp.keys.set_down(Key::Enter, false);
        assert!(client.class_editor.is_none(), "saving closes the editor");
        assert_eq!(out.choose_class, Client::CHOOSE_CLASS_CUSTOM);
        assert_eq!(out.choose_class_general, 1);
        assert!(out.choose_class_payload_len > 0);
        let words: Vec<u32> = out.choose_class_payload[..out.choose_class_payload_len as usize].to_vec();
        let class = crate::classes::CustomClass::from_wire_words(&words).expect("the payload reassembles");
        assert_eq!(class.primary, edited, "the payload carries the edited weapon");
        assert_eq!(crate::profile::classes().len(), 1, "the profile kept the class");
        assert_eq!(crate::profile::classes()[0], class);
        crate::profile::reset();
    }

    #[test]
    fn hor_plus_fov() {
        let f43 = horizontal_fov(65.0, 4.0 / 3.0).to_degrees();
        let f169 = horizontal_fov(65.0, 16.0 / 9.0).to_degrees();
        assert!((f43 - 65.0).abs() < 0.01);
        assert!(f169 > 80.0 && f169 < 82.0, "{f169}");
    }

    /// The `ZHBO_AUTOPILOT=classpick` script drives the whole editor: it opens it from the pick
    /// screen, names the class, changes the secondary, and saves — the same path an in-game run
    /// takes. This test runs the script through the client and asserts each step lands.
    #[cfg(feature = "autopilot")]
    #[test]
    fn the_classpick_autopilot_opens_names_and_saves_a_class() {
        crate::profile::reset();
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut client = Client::default();
        client.autopilot = Some(Autopilot { classpick: true, ..Default::default() });
        let mut inp = ClientIn::default();
        sim.take_control(&host, 0, 1);

        // Reach first person: the script runs there (the pick screen only listens in Fps).
        frame_with(&mut client, &mut sim, &host, &mut inp, 60);
        assert_eq!(client.view, ViewMode::Fps);

        // Run the script's full timeline (the last save pulse is at 8.4s, so ~9s of frames). The
        // save's choose one-shot is emitted on its own frame, so capture it as it goes by.
        let mut saved: Option<ClientOut> = None;
        for _ in 0..(30 * 10) {
            let out = client.frame(&mut sim, &inp);
            if out.choose_class != 0 {
                saved = Some(out);
            }
            inp.dt = LOGIC_DT;
            let f = sim.frame + 1;
            sim.step(&host, f);
            inp.logic_frame = f;
        }
        let out = saved.expect("the script saves a class");

        // The editor saved and closed, and the outgoing one-shot carries the custom class.
        assert!(client.class_editor.is_none(), "the script saves and closes the editor");
        assert_eq!(out.choose_class, Client::CHOOSE_CLASS_CUSTOM);
        let words: Vec<u32> = out.choose_class_payload[..out.choose_class_payload_len as usize].to_vec();
        let class = crate::classes::CustomClass::from_wire_words(&words).expect("the payload reassembles");
        assert_eq!(class.name, "AB", "the script names the class");
        assert_ne!(class.secondary, crate::classmenu::Draft::default().secondary, "the script changes the secondary");
        assert_eq!(crate::profile::classes().len(), 1, "the profile kept the class");
        assert_eq!(crate::profile::classes()[0].name, "AB");
        crate::profile::reset();
    }
}
