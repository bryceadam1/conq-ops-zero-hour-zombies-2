//! Per-render-frame client logic for the local player: mouse look, key bindings, sending one
//! [`FpsInput`] per logic frame, the camera (with RTS <-> first-person transitions), and HUD timers.
//!
//! Nothing here affects the lockstep simulation except through the input messages it emits.

use crate::classui::cues;
use crate::input::{buttons, pack_move, FpsInput, Key, KeySet, MAX_PITCH};
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
    /// Held: the whole map (Zero Hour's radar).
    pub map: Key,
    /// Held: BO2's scoreboard.
    pub scoreboard: Key,
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
            map: Key::M,
            scoreboard: Key::Tab,
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
/// BO2's hit marker fades over a second (`_damagefeedback.gsc`: `fadeovertime(1)`; Ghidra G6).
pub const HITMARKER_TIME: f32 = crate::hud::center::HITMARKER_FADE;
pub const DEATH_BANNER_TIME: f32 = 3.0;
/// BO2's "+100" kill popup stays up this long; kills inside the window add up.
/// The score popup's `DisplayTime` (`hud/scorepopup.lua:12`).
pub const SCORE_POPUP_TIME: f32 = crate::hud::center::SCORE_DISPLAY;
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
    /// One-shot from the host: the command bar's Create-a-Class entry was pressed, so the pick
    /// screen opens (or re-opens) even when a class is already chosen. The bridge latches it until
    /// the first client frame consumes it. (The lobby's button opens the shell screen instead —
    /// see `menu.rs`.)
    pub class_pick_force: i32,
    /// One-shot from the host: a command-bar class button was pressed — 1-based, in
    /// [`crate::classicons`]' button order (the five default classes first, then the active class
    /// set's customs), 0 = none.
    /// The engine dispatches a bar click at a different point in the frame from the client's, so
    /// the bridge only hands the button over and the pick is emitted here, through the same
    /// one-shot the pick screen writes. See [`Client::pick_class_button`].
    pub class_button: i32,
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
            class_button: 0,
        }
    }
}

/// How many shot impacts one client frame can carry (`host/zhbo/zhbo_core.h`'s
/// `ZHBO_CLIENT_HITS`). One shot per logic frame and at most a shotgun's pellets per shot; a
/// render frame that spans two logic frames can overflow it, and the extras are dropped.
pub const CLIENT_HITS: usize = 8;

/// One shot impact of the local player's this frame: BO2 plays the impact table's cell at `point`,
/// oriented by `dir`, in that surface's way (docs/CONTRACT.md "Shot impacts (R8d)").
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClientHit {
    /// Where the pellet stopped, Zero Hour units.
    pub point: Vec3,
    /// The pellet's direction; the impact effect faces back along it.
    pub dir: Vec3,
    /// `impacts::Surface` as its ABI value (`ZHBO_SURF_*`).
    pub surface: i32,
    /// The hit is in the head band (BO2's `FxFleshType` head bit).
    pub head: i32,
    /// The hit killed (BO2's `FxFleshType` fatal bit).
    pub fatal: i32,
    /// BO2's `WeaponDef.impactType`: the impact-table bucket.
    pub impact_type: i32,
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
    /// One-shot: the player picked class `choose_class` for `choose_class_general`: a 1-based
    /// index into `classes.rs`' list (0 is the "no pick" marker, and the bridge subtracts one for
    /// the message), or `i32::MIN` for a custom class whose wire words are in
    /// `choose_class_payload` (`classes.rs`' `CustomClass::to_wire_words`).
    pub choose_class: i32,
    pub choose_class_general: u32,
    /// The custom class's wire words when `choose_class` is the custom marker; unused otherwise.
    pub choose_class_payload: [u32; 64],
    /// How many `choose_class_payload` words are valid (0 for a shipped class pick).
    pub choose_class_payload_len: i32,
    /// One-shot: write the profile file now. The in-match client is pick-only, so it never sets
    /// this (the shell menu's editor/delete/copy use `MenuOut`'s own one-shot); a pick alone must
    /// never rewrite the profile.
    pub save_profile: i32,
    /// The profile file's full text, UTF-8 packed four bytes per word (see `profile::pack_text`).
    pub save_profile_payload: [u32; 1024],
    /// How many `save_profile_payload` words are valid.
    pub save_profile_payload_len: i32,
    /// The in-match class pick is up: the bridge leaves the cursor free and swallows the
    /// game-bound mouse/key messages behind it.
    pub menu_open: i32,
    /// The local player's shot impacts this frame, in shot order (at most [`CLIENT_HITS`]).
    pub hits: [ClientHit; CLIENT_HITS],
    /// How many `hits` are valid.
    pub hits_len: i32,
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
            save_profile: 0,
            save_profile_payload: [0; 1024],
            save_profile_payload_len: 0,
            menu_open: 0,
            hits: [ClientHit::default(); CLIENT_HITS],
            hits_len: 0,
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
    /// The scoreboard key is held.
    pub scoreboard_open: bool,
    /// The BO2 HUD's clock and between-frame state (`hud::state`).
    pub lui: crate::hud::state::HudLui,
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
    /// Seconds of client frames (render time, UI only: the class screen's pulses and fades).
    pub ui_clock: f32,
    last_sent_frame: Option<u32>,
    latch: u32,
    prev_held: u32,
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
    /// The last shot's gun kick impulse (`EventKind::Recoil`'s `gun_kick`), taken by the viewmodel
    /// motion next frame: G5's spring, in degrees/s of gun angle.
    vm_kick: Option<(f32, f32)>,
    /// BO2 sound cues queued this frame (drained by the FFI layer into the host).
    pub cues: Cues,
    /// This frame's shot impacts (the local player's), copied into `ClientOut`.
    pub hits: [ClientHit; CLIENT_HITS],
    pub hits_len: usize,
    /// Set while the pick screen was forced open from outside a spawn (the command bar's
    /// Create-a-Class entry). It keeps the screen up even when a class is already chosen, so a
    /// pick can be replaced, and clears once one lands or Escape closes it.
    pub class_pick_forced: bool,
    /// After Escape closes the in-match pick while the General is still unclassed, the auto-offer
    /// stays shut until the next take-control. The command-bar force ignores this latch.
    pub class_pick_dismissed: bool,
    /// Whether the pick screen is actually up this frame (set at the end of [`Client::frame`]).
    /// The HUD draws it (and the bridge frees the cursor) only while this is true.
    pub class_pick_open: bool,
    /// The pick screen's highlighted row (client-local; the mouse and 1-9,0 move it).
    pub class_pick_highlight: usize,
    /// The `ui_clock` reading of the last time the screen opened, while `CoD.Menu.animateIn`'s
    /// 250 ms entrance fade runs (`codmenu.lua:2300-2326`); `None` until it first opens and ignored
    /// once the fade is over.
    class_pick_since: Option<f32>,
    /// The previous frame's keys/pointer: hover and click edges for the class screens (the shared
    /// fold's prev state; the shell menu keeps its own).
    menu_prev: crate::classui::MenuPrev,
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
            vm_kick: None,
            cues: Cues::default(),
            hits: [ClientHit::default(); CLIENT_HITS],
            hits_len: 0,
            class_pick_forced: false,
            class_pick_dismissed: false,
            class_pick_open: false,
            class_pick_highlight: 0,
            class_pick_since: None,
            ui_clock: 0.0,
            menu_prev: crate::classui::MenuPrev::default(),
        }
    }

    pub fn reset(&mut self) {
        #[cfg(feature = "autopilot")]
        let autopilot =
            self.autopilot.as_ref().map(|a| Autopilot { equipment: a.equipment, build: a.build, classpick: a.classpick, scope: a.scope, ..Default::default() });
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
}

/// The class screens' input frame, projected from this frame's client input (the shell menu's
/// `MenuIn` projects into the same shape; the shared folds work on either).
fn menu_pointer(inp: &ClientIn) -> crate::classui::MenuPointer {
    crate::classui::MenuPointer {
        keys: inp.keys,
        mouse_x: inp.mouse_x,
        mouse_y: inp.mouse_y,
        mouse_buttons: inp.mouse_buttons,
        wheel: inp.wheel,
        screen_w: inp.screen_w,
        screen_h: inp.screen_h,
    }
}

impl Client {
    /// The class screen's view for `classui::draw_screen` (the shell menu produces the same shape,
    /// with its editor and management affordances). A match is pick-only.
    pub fn class_screen_view(&self) -> crate::classui::ScreenView<'_> {
        crate::classui::ScreenView {
            highlight: self.class_pick_highlight,
            editor: None,
            personalize: None,
            deny: 0.0,
            manage: false,
            now: self.ui_clock,
            // A match is pick-only: the set is chosen in the lobby, so it never switches here.
            set_change: None,
            // BO2's class screen opens as a popup and `animateIn` fades it in over 250 ms; the
            // view only carries the age while that fade is running, so a settled screen draws the
            // static prims exactly.
            open: self
                .class_pick_since
                .filter(|since| self.ui_clock - since < crate::classui::MENU_OPEN_MS / 1000.0)
                .map(|since| (self.ui_clock - since).max(0.0)),
        }
    }

    /// The pick screen's input, on the shared fold (`classui::pick_frame`): an activated row picks
    /// it (no editor, no delete/copy in a match — BO2 only creates classes in the lobby). Escape
    /// closes the screen and keeps the current kit.
    fn class_pick_input(&mut self, inp: &ClientIn, out: &mut ClientOut, unclassed: bool) {
        let frame = crate::classui::pick_frame(&menu_pointer(inp), &self.menu_prev, self.class_pick_highlight, false);
        self.class_pick_highlight = frame.highlight;
        if frame.navigated && frame.activate.is_none() {
            self.cues.play(cues::GRID_NAV);
        }
        if let Some(i) = frame.activate {
            match crate::classui::pick_rows(false).get(i).copied() {
                Some(crate::classui::PickRow::Preset(j)) => {
                    // 1-based: 0 is the one-shot's "no pick" marker; the bridge subtracts the one.
                    out.choose_class = j as i32 + 1;
                    out.choose_class_general = self.general;
                    out.choose_class_payload_len = 0;
                    self.cues.play(cues::GRID_EQUIP);
                    self.class_pick_forced = false;
                }
                Some(crate::classui::PickRow::Custom(j)) => {
                    if let Some(class) = crate::profile::classes().get(j) {
                        if pick_custom_class(out, self.general, class) {
                            self.cues.play(cues::GRID_EQUIP);
                            self.class_pick_forced = false;
                        } else {
                            // Never send a truncated payload: the sim could not decode it and the
                            // pick would silently do nothing (the General keeping the stock kit).
                            self.cues.play(sounds::DRY_FIRE);
                            self.note("That class is too large to send");
                        }
                    }
                }
                // The in-match list has no CREATE A CLASS row.
                Some(crate::classui::PickRow::Empty(_)) | None => {}
            }
        } else if frame.back {
            self.close_pick(unclassed);
        }
    }

    /// A command-bar class button was pressed (`button` is 1-based, see `ClientIn::class_button`):
    /// emit the pick the class screen would emit for that class, so the bar and the screen share
    /// one writer of `choose_class`. Buttons below `classicons::PRESETS` are the five default
    /// classes by registry index (a 1-based `choose_class`); the rest are the ACTIVE class set's
    /// customs ([`crate::profile::classes`]), sent as their wire words. A button whose class is
    /// gone (an empty custom slot, or a registry with fewer presets than the bar drew) sends
    /// nothing, like an empty pick row.
    fn pick_class_button(&mut self, out: &mut ClientOut, button: i32) {
        let Some(index) = usize::try_from(button - 1).ok() else { return };
        if index < crate::classicons::PRESETS {
            if crate::classes::at(index).is_none() {
                return;
            }
            // 1-based: 0 is the one-shot's "no pick" marker; the bridge subtracts the one.
            out.choose_class = index as i32 + 1;
            out.choose_class_general = self.general;
            out.choose_class_payload_len = 0;
        } else {
            // Snapshot the profile once, like the pick screen's draw does.
            let classes = crate::profile::classes();
            let Some(class) = classes.get(index - crate::classicons::PRESETS) else { return };
            if !pick_custom_class(out, self.general, class) {
                // Never send a truncated payload (see `pick_custom_class`).
                self.cues.play(sounds::DRY_FIRE);
                self.note("That class is too large to send");
                return;
            }
        }
        self.cues.play(cues::GRID_EQUIP);
        self.class_pick_forced = false;
    }

    /// Close the pick screen without picking: keep the current (or base) kit and let the player
    /// play on. Escaping while still unclassed latches the auto-offer shut until the next
    /// take-control; the command-bar force still opens the screen regardless.
    fn close_pick(&mut self, unclassed: bool) {
        self.class_pick_forced = false;
        if unclassed {
            self.class_pick_dismissed = true;
        }
        self.cues.play(cues::BACKOUT);
    }

    /// The class screen's input, from either the overhead view or first person. Returns true while
    /// the pick screen owns the frame (so the first-person input path stays out of it).
    fn menu_input(&mut self, inp: &ClientIn, out: &mut ClientOut, unclassed: bool) -> bool {
        if self.class_pick_wanted(unclassed) {
            self.class_pick_input(inp, out, unclassed);
            return true;
        }
        false
    }

    /// Whether the pick screen is up. In the overhead view only the command-bar force opens it; in
    /// first person the force opens it too, and an unclassed General is offered it at take-control
    /// unless Escape dismissed the offer for this life.
    fn class_pick_wanted(&self, unclassed: bool) -> bool {
        match self.view {
            // Overhead: only the command bar's force opens it (an unclassed player keeps playing).
            ViewMode::Rts => self.class_pick_forced,
            // First person: the force, or the spawn offer while unclassed and not dismissed.
            ViewMode::Fps => self.class_pick_forced || (unclassed && !self.class_pick_dismissed),
            // Never during the camera transitions.
            ViewMode::Entering { .. } | ViewMode::Leaving { .. } => false,
        }
    }
}

/// `choose_class`'s marker for a class built in the create-a-class editor: the payload words carry
/// its spec instead of a registry index.
const CHOOSE_CLASS_CUSTOM: i32 = i32::MIN;

/// The match list's row for the class the General is wearing, so the pick screen can open on it
/// (`hud/changeclass.lua`'s `Button_RowGainFocus` fires for the active class). The sim names a
/// shipped class by its registry id and a custom one `custom:<name>`
/// (`sim::choose_class`), and the profile stores customs by name, which is what the row lookup
/// compares.
fn current_class_row(class: Option<&str>) -> Option<usize> {
    let class = class?;
    let presets = crate::classes::get();
    let customs = crate::profile::classes();
    crate::classui::pick_rows(false).iter().position(|row| match *row {
        crate::classui::PickRow::Preset(j) => presets.get(j).is_some_and(|c| c.id == class),
        crate::classui::PickRow::Custom(j) => customs.get(j).is_some_and(|c| class == format!("custom:{}", c.name)),
        crate::classui::PickRow::Empty(_) => false,
    })
}

/// Fill a custom-class pick into the outgoing one-shot: the marker in `choose_class`, the class's
/// wire words in the payload. The bridge turns it into `MSG_ZHBO_CHOOSE_CLASS` and the sim
/// reassembles the spec ([`crate::classes::CustomClass::from_wire_words`]).
///
/// Returns `false` when the wire words do not fit the ABI payload: a truncated payload would fail
/// to decode in the sim, which would then ignore the pick and leave the General on his old kit.
/// The caller denies the pick instead.
fn pick_custom_class(out: &mut ClientOut, general: u32, class: &crate::classes::CustomClass) -> bool {
    let words = class.to_wire_words();
    if words.len() > out.choose_class_payload.len() {
        return false;
    }
    out.choose_class = CHOOSE_CLASS_CUSTOM;
    out.choose_class_general = general;
    out.choose_class_payload[..words.len()].copy_from_slice(&words);
    out.choose_class_payload_len = words.len() as i32;
    true
}

impl Client {
/// Run one rendered frame.
pub fn frame(&mut self, sim: &mut Sim, inp: &ClientIn) -> ClientOut {
        let dt = inp.dt.clamp(0.0, 0.25);
        self.ui_clock += dt;
        // The sim's record names the General the local player is commanding, in first person or
        // overhead. Adopt it before the class screen runs: a fresh General's first class pick can
        // come from the command bar before he was ever taken into first person, and the client
        // only learned his id on entering before — so that pick named General 0, the sim dropped
        // it, and the General kept the stock kit.
        let player = sim.player(inp.local_player).cloned();
        if let Some(p) = &player {
            self.general = p.general;
        }
        let unclassed = player.as_ref().is_none_or(|p| p.class.is_none());
        #[cfg(feature = "autopilot")]
        let pick_wanted = self.class_pick_wanted(unclassed);
        #[allow(unused_mut, unused_assignments)]
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
                // A script's class-pick key is only pressed while the spawn pick is really up (a
                // re-entry on an already classed General must not turn it into a weapon swap).
                if let Some(key) = script.class_pick {
                    if pick_wanted {
                        scripted.keys.set_down(key, true);
                    }
                }
                scripted.mouse_dx = script.dx;
                scripted.mouse_dy = script.dy;
                scripted.wheel = script.wheel;
                scripted.class_pick_force = script.pick_force;
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
        self.hud.scoreboard_open = false;

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
        self.hud.lui.tick(dt, player.as_ref().filter(|p| p.is_first_person()));

        // A forced open (the lobby's Create a Class button or the command bar's entry) keeps the
        // screen up even when a class is already chosen, so a kit can be replaced.
        if inp.class_pick_force != 0 {
            self.class_pick_forced = true;
        }

        // The command bar's class buttons. The engine dispatches a bar click from its own command
        // processing, at a different point in the frame from this call, so the bridge hands the
        // button over as a one-shot and the pick is emitted here. (Queueing the pick into
        // `ClientOut` from the bridge instead would be dropped: every `zhbo_client_frame` writes a
        // fresh whole struct over the bridge's copy before it can read the pick.)
        if inp.class_button > 0 {
            self.pick_class_button(&mut out, inp.class_button);
        }

        // The class pick's entrance: BO2's in-game class menu opens with `uin_main_pause`
        // (`hud/class.lua`), once per open. The screen only shows when `class_pick_wanted` says so
        // (the command-bar force in either view; first person's spawn offer while unclassed and not
        // dismissed).
        if self.class_pick_wanted(unclassed) {
            if !self.hud.class_pick_seen {
                self.hud.class_pick_seen = true;
                self.cues.play(cues::IN_GAME_OPEN);
                // BO2's changeclass screen focuses the class the player is wearing (its rows fire
                // `Button_RowGainFocus` on the active one), so opening the panel mid-life lands on
                // it rather than on the first row.
                if let Some(row) = current_class_row(player.as_ref().and_then(|p| p.class.as_deref())) {
                    self.class_pick_highlight = row;
                }
            }
        } else {
            self.hud.class_pick_seen = false;
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
                } else {
                    // In the overhead view only the command-bar's Create a Class entry
                    // (`class_pick_force`) opens the screen; an unclassed General is *not* offered
                    // it here (he never has to stop playing the RTS to pick).
                    if self.class_pick_wanted(unclassed) {
                        self.menu_input(inp, &mut out, unclassed);
                    }
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
                    // The class screen takes the cursor and freezes the view behind it.
                    let menu = self.class_pick_wanted(unclassed);
                    if allowed && !in_fc && !menu {
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
                    out.capture_mouse = (allowed && !menu) as i32;
                    self.view_eye = out.eye;
                    self.view_dir = out.dir;
                    self.hud.map_open = allowed && !in_fc && !menu && inp.keys.is_down(self.cfg.bindings.map);
                    self.hud.scoreboard_open = allowed && !menu && inp.keys.is_down(self.cfg.bindings.scoreboard);
                    let events = std::mem::take(&mut self.vm_events);
                    // The viewmodel's motion reads the def's gun-position numbers (G4) and the
                    // sim's own stance/speed/aim; the view's `cg_fov` is G4's wide-FOV reference.
                    let pose = self.vm_motion.update(dt, self.yaw, self.pitch, self.cfg.cg_fov, p, self.vm_kick.take());
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
                    self.build_input(inp, &mut out, allowed && !in_fc, p.hammer.is_some(), unclassed);
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
        // The class pick's state after this frame's input: it draws over the view, `menu_open`
        // makes the bridge leave the cursor free and eat the game-bound mouse/key messages behind
        // it. Both follow the same latch so the HUD and the bridge can never disagree.
        let was_open = self.class_pick_open;
        self.class_pick_open = self.class_pick_wanted(unclassed);
        out.menu_open = self.class_pick_open as i32;
        // The frame's impacts (consume_events filled them above).
        out.hits = self.hits;
        out.hits_len = self.hits_len as i32;
        // A fresh open starts `animateIn`'s entrance fade (`codmenu.lua:2300-2326`); a screen that
        // stays up keeps its clock, and a closed one starts over on the next open.
        if self.class_pick_open && !was_open {
            self.class_pick_since = Some(self.ui_clock);
        }
        // The fresh-press/hover detectors compare against this frame's keys and cursor.
        self.menu_prev.update(&menu_pointer(inp));
        out
    }

    fn start_entering(&mut self, inp: &ClientIn, general: u32, yaw: f32, eye: Vec3) {
        self.general = general;
        self.vm = Viewmodel::default(); // the weapon comes up as first person starts
        self.vm_motion = viewmodel::Motion::default();
        self.vm_kick = None;
        // A fresh take-control re-offers the spawn pick even if the last life Escaped it.
        self.class_pick_dismissed = false;
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

    fn build_input(&mut self, inp: &ClientIn, out: &mut ClientOut, allowed: bool, hammer: bool, unclassed: bool) {
        let (mut held, mut fwd, mut right) = if allowed { self.read_buttons(inp) } else { (0, 0.0, 0.0) };
        if !allowed {
            fwd = 0.0;
            right = 0.0;
        }
        // The class pick owns the frame while it is up: it reads the arrows/Enter/mouse and
        // nothing else reaches the game (no look, no movement, no firing).
        if self.menu_input(inp, out, unclassed) {
            self.prev_held = u32::MAX;
            return;
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
        self.hits_len = 0;
        // The weapon in hand after the step: its sounds for this frame's shots, swing and throws.
        let held = sim.player(me).map(|p| (p.arsenal.current().def, p.arsenal.current().ammo, p.arsenal.current().string, p.arsenal.lethal.def, p.arsenal.tactical.def));
        let fire = held.map_or(sounds::FIRE, |(def, ..)| sounds::fire_for(def));
        let shots = mine.iter().filter(|k| matches!(k, EventKind::Shot { .. })).count();
        if let Some((def, ammo, string, ..)) = held {
            // A fire string's opening (`fireStartSoundPlayer`: a burst's recording) plays once, on
            // the frame its first round leaves — the trigger string was empty before this step.
            if let Some(start) = sounds::some(def.fire_start_sound.as_str()).filter(|_| shots > 0 && string as usize <= shots) {
                self.cues.play(start);
            }
            for i in 0..shots {
                // The gunshot is per shot; the impact is per pellet and comes from its own event (a
                // shot into the air no longer plays a hard-surface sound). The last one this frame
                // left the magazine at `ammo`.
                self.cues.play(if i + 1 == shots { sounds::shot_for(def, ammo, fire) } else { fire });
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
                    self.hud.lui.deaths += 1;
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
                        self.hud.lui.kills += 1;
                        if self.hud.score_popup <= 0.0 {
                            self.hud.score_points = 0;
                        }
                        self.hud.score_points += KILL_POINTS;
                        self.hud.score_popup = SCORE_POPUP_TIME;
                    }
                }
                EventKind::Recoil { pitch_deg, yaw_deg, gun_kick, .. } => {
                    // BO2 assault rifles mostly recover: part of each kick moves the real aim, the
                    // rest is a visual kick that settles back.
                    self.pitch = (self.pitch + (pitch_deg * RECOIL_PERMANENT).to_radians()).clamp(-MAX_PITCH, MAX_PITCH);
                    self.yaw = wrap_angle(self.yaw + (yaw_deg * RECOIL_PERMANENT).to_radians());
                    self.hud.view_kick_pitch += pitch_deg * (1.0 - RECOIL_PERMANENT);
                    self.hud.view_kick_yaw += yaw_deg * (1.0 - RECOIL_PERMANENT);
                    // The gun itself kicks on G5's spring, not with the camera.
                    self.vm_kick = Some(gun_kick);
                    self.hud.muzzle = 0.05;
                }
                EventKind::Flinch { pitch_deg } => self.hud.view_kick_pitch += pitch_deg,
                EventKind::Damaged { amount } => {
                    self.cues.play(sounds::HURT);
                    self.hud.damage_flash = (self.hud.damage_flash + amount / 60.0).min(1.0);
                }
                // A pellet landed: BO2's impact FX (the bridge plays it from `ClientOut.hits`) and
                // the surface's sound here. Blood for a soldier, the hard-surface cue otherwise.
                EventKind::Impact { at, dir, surface, head, fatal, impact_type } => {
                    self.cues.play_at(if surface == crate::impacts::Surface::Flesh { sounds::IMPACT_FLESH } else { sounds::IMPACT }, at);
                    if self.hits_len < CLIENT_HITS {
                        self.hits[self.hits_len] =
                            ClientHit { point: at, dir, surface: surface as i32, head: head as i32, fatal: fatal as i32, impact_type };
                        self.hits_len += 1;
                    }
                }
                EventKind::Shot { .. } => self.vm_events.shot = true,
                EventKind::ReloadStarted => self.vm_events.reload_started = true,
                EventKind::Swapped => self.vm_events.swapped = true,
                EventKind::FieldComputerOpened => {}
                EventKind::ClassChosen { .. } => {
                    // No cue here: BO2's class change applies silently on spawn. The menu already
                    // played the equip when the pick was made (and the command bar does too).
                    // A pick landed: the forced screen closes, and the next spawn's pick returns.
                    self.class_pick_forced = false;
                }
                EventKind::Melee => self.cues.play(held.map_or(sounds::MELEE_SWING, |(def, ..)| sounds::melee_swing_for(def))),
                EventKind::MeleeHit => {
                    if let Some((def, ..)) = held.filter(|(def, ..)| !def.melee_hit_sound.is_empty()) {
                        self.cues.play(def.melee_hit_sound.as_str());
                    }
                }
                EventKind::OffhandReady { lethal } => {
                    self.cues.play(held.map_or(sounds::PULL_PIN, |(_, _, _, l, t)| sounds::pullback_for(if lethal { l } else { t })))
                }
                EventKind::Thrown { lethal } => self.cues.play(held.map_or(sounds::THROW, |(_, _, _, l, t)| sounds::throw_for(if lethal { l } else { t }))),
                EventKind::Launched => self.cues.play(held.map_or(sounds::FIRE_LAUNCHER, |(def, ammo, ..)| sounds::shot_for(def, ammo, sounds::launch_for(def)))),
                EventKind::Exploded { at, sound } => self.cues.play_at(sounds::explosion_for(sound), at),
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

/// The client's tests live in their own file: `mod.rs` is at the module-size limit.
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
