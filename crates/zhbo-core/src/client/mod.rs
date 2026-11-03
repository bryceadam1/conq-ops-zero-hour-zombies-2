//! Per-render-frame client logic for the local player: mouse look, key bindings, sending one
//! [`FpsInput`] per logic frame, the camera (with RTS <-> first-person transitions), and HUD timers.
//!
//! Nothing here affects the lockstep simulation except through the input messages it emits.

use crate::classui::cues;
use crate::host::LogicHost;
use crate::input::{buttons, pack_move, FpsInput, Key, KeySet, MAX_PITCH};
use crate::math::{dir_from_angles, lerp_angle, smoothstep, wrap_angle, Vec3};
use crate::sim::{EventKind, Mode, PlayerSim, Sim};
use crate::sounds::{self, Cues};
use crate::units::LOGIC_DT;
use crate::viewmodel::{self, Viewmodel};
use crate::weapons::zoom::{self, ZoomView};

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
/// BO2's team-deathmatch score for an assist by its level (`sim::score::assist_level`: 1, 2, 3 for
/// `assist_25`, `assist_50`, `assist_75`): `mp/scoreinfo.csv`'s `tdm score` column (25, 25, 50).
pub const ASSIST_POINTS: [u32; 3] = [25, 25, 50];

/// BO2's team-deathmatch score for a tactical's assist (`assist_concussion`, `assist_flash`,
/// `assist_emp`, `assist_proximity`: 50 each in `mp/scoreinfo.csv`'s `tdm score`).
pub const TACTICAL_ASSIST_POINTS: u32 = 50;

/// The score of an assist at `level`, or of a tactical's ([`ASSIST_POINTS`],
/// [`TACTICAL_ASSIST_POINTS`]).
pub fn assist_points(level: u8, tactical: Option<crate::sim::score::Tactical>) -> u32 {
    if tactical.is_some() {
        return TACTICAL_ASSIST_POINTS;
    }
    ASSIST_POINTS[(level.clamp(1, 3) - 1) as usize]
}

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
    /// This machine only watches (`!ThePlayerList->getLocalPlayer()->isPlayerActive()`: an
    /// observer seat, a defeated player who stays to watch; or a replay's playback). The client
    /// then never sends sim input and follows first-person Generals ([`spectate`]);
    /// `local_player` is the seat the engine observes (or the observer's own). Appended at the end
    /// of the struct (ABI rule).
    pub spectating: i32,
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
            spectating: 0,
        }
    }
}

/// One seat's match as this machine saw the sim's events: the scoreboard's row for a player other
/// than the local one (whose own row is the HUD's tally, `HudLui::kills`/`deaths`, which the
/// prediction confirms a frame sooner). Every machine consumes every event, so every machine's
/// board agrees. Presentation only.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SeatTally {
    /// The seat has played in first person (it had a record or an event).
    pub seen: bool,
    /// Kills (`HitMarker { kill }`) and deaths (`GeneralDied`), as the HUD counts its own.
    pub kills: u32,
    pub deaths: u32,
    /// His assists: the sim's own count (`sim::score::Score::assists`, the same on every machine),
    /// and the points they scored (`Assist` events, [`assist_points`]).
    pub assists: u32,
    pub assist_points: u32,
    /// His army's colour and whether he is on the local player's side (`objflags::ALLY`), last
    /// read off his General while one stood.
    pub color: u32,
    pub ally: bool,
}

/// How many shot impacts one client frame can carry (`host/zhbo/zhbo_core.h`'s
/// `ZHBO_CLIENT_HITS`). One shot per logic frame and at most a shotgun's pellets per shot; a
/// render frame that spans two logic frames, or other players' rounds on top, can overflow it,
/// and the extras are dropped (others' first).
pub const CLIENT_HITS: usize = 8;

/// One shot impact this frame (the presented seat's, or another player's round): BO2 plays the impact table's cell at `point`,
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
    /// The round left a solid it penetrated here: the type's exit row (`zhbo_impact_exit_fx`).
    pub exit: i32,
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
    pub choose_class_payload: [u32; crate::classwire::PAYLOAD_WORDS],
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
    /// This frame's shot impacts: the presented seat's in shot order, then other players' (at
    /// most [`CLIENT_HITS`]).
    pub hits: [ClientHit; CLIENT_HITS],
    /// How many `hits` are valid.
    pub hits_len: i32,
    /// The camera's roll this frame, radians, as W3D's `Matrix3D::Look_At_Dir` takes it (the view
    /// kick's roll axis). Both are a right-handed turn about the forward axis — `Look_At_Dir`
    /// applies `Rotate_Z(-roll)` because the camera looks down −Z, and BO2's `+roll` drops the
    /// right side — so the value passes unchanged. Appended at the end of the struct (ABI rule).
    pub view_roll: f32,
    /// The dual-wield left gun fired this frame: its muzzle flash and brass at the `_lh` model's
    /// `tag_flash1`/`tag_brass1` (`viewmodel_fired` is the right gun's, at `tag_flash`/`tag_brass`).
    pub viewmodel_fired_left: i32,
    /// One-shot: select this object in the RTS view (the local General as he comes back overhead
    /// alive, so his command bar — the class set while he has no class — is up again), 0 = none.
    /// Appended at the end of the struct (ABI rule).
    pub select_object: u32,
    /// With `send_input`: the logic frame this machine showed as the input was made (the sim's
    /// frame then). The message carries it: the sim resolves the input's shots against that frame
    /// (`sim::antilag`) and this client's prediction knows which inputs the record holds
    /// (`client::predict`). Appended at the end of the struct (ABI rule).
    pub input_seen_frame: u32,
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
            choose_class_payload: [0; crate::classwire::PAYLOAD_WORDS],
            choose_class_payload_len: 0,
            save_profile: 0,
            save_profile_payload: [0; 1024],
            save_profile_payload_len: 0,
            menu_open: 0,
            hits: [ClientHit::default(); CLIENT_HITS],
            hits_len: 0,
            view_roll: 0.0,
            viewmodel_fired_left: 0,
            select_object: 0,
            input_seen_frame: 0,
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
mod events;
pub mod predict;
pub mod remote;
pub mod spectate;
pub mod tracers;
#[cfg(feature = "autopilot")]
pub use autopilot::{Autopilot, Scripted};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HudTimers {
    pub hitmarker: f32,
    pub hitmarker_kill: bool,
    pub hitmarker_head: bool,
    pub damage_flash: f32,
    pub death_banner: f32,
    /// The damage flinch's view pitch (degrees, G7), settling back. The weapon's view kick is the
    /// sim's (`PlayerSim::view_kick`).
    pub flinch_pitch: f32,
    pub muzzle: f32,
    pub score_popup: f32,
    pub score_points: u32,
    /// The weapon hint on screen (`invalidcmdhint.menu`'s ownerdraw 113): its localize key and
    /// seconds since it was set.
    pub hint: Option<(&'static str, f32)>,
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
    /// The sim's view kick aim offset (yaw, pitch radians) at the last two logic frames.
    kick_prev: (f32, f32, f32),
    kick_curr: (f32, f32, f32),
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
    /// BO2's render-only zoom state: the hybrid's alt-scope fraction and the zoom direction.
    zoom_view: ZoomView,
    /// The last shot's gun kick impulse (`EventKind::Recoil`'s `gun_kick`), taken by the viewmodel
    /// motion next frame: G5's spring, in degrees/s of gun angle.
    vm_kick: Option<(f32, f32)>,
    /// BO2 sound cues queued this frame (drained by the FFI layer into the host).
    pub cues: Cues,
    /// The local player's shellshock as presented: its view kick and ringing loop.
    pub shock: crate::shellshock::Presented,
    /// This frame's impacts of the presented seat's rounds, copied into `ClientOut`.
    pub hits: [ClientHit; CLIENT_HITS],
    pub hits_len: usize,
    /// Other first-person Generals' footsteps (`sounds::RemoteSteps`).
    remote_steps: sounds::RemoteSteps,
    /// Every General as a BO2 third-person player: torso actions, their sounds, his voice.
    pub remote: remote::Remote,
    /// Every seat's match (the scoreboard's other rows), by seat.
    pub seats: [SeatTally; crate::sim::MAX_PLAYERS],
    /// This frame's impacts of other players' rounds (`events::present_remote`): they take the
    /// slots of `ClientOut.hits` the local player's leave.
    pub remote_hits: Vec<ClientHit>,
    /// Set while the pick screen was forced open from outside a spawn (the command bar's
    /// Create-a-Class entry). It keeps the screen up even when a class is already chosen, so a
    /// pick can be replaced, and clears once one lands or Escape closes it.
    pub class_pick_forced: bool,
    /// Escape on the spawn's class screen (first person, no class yet) declined the spawn: BO2
    /// never lets a player play without a class (`_menus.gsc`'s `back` only closes the menu; only
    /// `menuclass` spawns), so the screen closes and the client sends the field-computer press that
    /// hands the General straight back overhead (`PlayerSim::awaiting_class`). Until the sim does,
    /// first person shows a still view with no input; the next take-control clears it.
    pub class_pick_declined: bool,
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
    /// Who views this machine's tracers (the last rendered frame's local player, his General and
    /// the first-person one), for the host layer's per-viewer choice after each logic step, and
    /// the own gun's tracers for the viewmodel pass.
    pub tracers: tracers::ViewTracers,
    /// The local player's prediction over his inputs in flight (`predict`): what first person is
    /// drawn from while the engine's logic is available to it ([`Client::frame_with`]).
    pub predict: predict::Predictor,
    /// Some while this machine only watches ([`ClientIn::spectating`]): who it follows.
    pub spectator: Option<spectate::Spectator>,
    /// The seat whose view this frame presented: the local player's own, or the General the
    /// spectator follows (-1: none). The HUD and the native viewmodel's class art read it.
    pub viewed: i32,
    /// The followed General's input angles (yaw, pitch) at the last two logic frames.
    aim_prev: (f32, f32),
    aim_curr: (f32, f32),
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
            kick_prev: (0.0, 0.0, 0.0),
            kick_curr: (0.0, 0.0, 0.0),
            seen_frame: 0,
            since_logic: 0.0,
            last_hfov: 1.0,
            overhead: None,
            view_eye: Vec3::ZERO,
            view_dir: Vec3::new(1.0, 0.0, 0.0),
            vm: Viewmodel::default(),
            zoom_view: ZoomView::default(),
            vm_motion: viewmodel::Motion::default(),
            vm_events: viewmodel::Events::default(),
            vm_kick: None,
            cues: Cues::default(),
            shock: Default::default(),
            hits: [ClientHit::default(); CLIENT_HITS],
            hits_len: 0,
            remote_hits: Vec::with_capacity(CLIENT_HITS),
            seats: [SeatTally::default(); crate::sim::MAX_PLAYERS],
            remote_steps: Default::default(),
            remote: Default::default(),
            class_pick_forced: false,
            class_pick_declined: false,
            class_pick_open: false,
            class_pick_highlight: 0,
            class_pick_since: None,
            ui_clock: 0.0,
            menu_prev: crate::classui::MenuPrev::default(),
            tracers: tracers::ViewTracers::default(),
            predict: predict::Predictor::default(),
            spectator: None,
            viewed: -1,
            aim_prev: (0.0, 0.0),
            aim_curr: (0.0, 0.0),
        }
    }

    /// The General this machine looks through in first person (entering, in it, or leaving), whose
    /// own body the view hides; `None` overhead.
    pub fn first_person_general(&self) -> Option<u32> {
        (!matches!(self.view, ViewMode::Rts) && self.general != 0).then_some(self.general)
    }

    pub fn reset(&mut self) {
        #[cfg(feature = "autopilot")]
        let autopilot = self.autopilot.as_ref().map(Autopilot::fresh);
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
    /// one writer of `choose_class`. Buttons below `classicons::preset_rows()` are the five default
    /// classes by registry index (a 1-based `choose_class`); the rest are the ACTIVE class set's
    /// customs ([`crate::profile::classes`]), sent as their wire words. A button whose class is
    /// gone (an empty custom slot, or a registry with fewer presets than the bar drew) sends
    /// nothing, like an empty pick row.
    fn pick_class_button(&mut self, out: &mut ClientOut, button: i32) {
        let Some(index) = usize::try_from(button - 1).ok() else { return };
        if index < crate::classicons::preset_rows() {
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
            let Some(class) = classes.get(index - crate::classicons::preset_rows()) else { return };
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

    /// Close the pick screen without picking. A classed General keeps his kit and plays on (BO2's
    /// mid-life Back). An unclassed one in first person is not spawned, and closing the screen
    /// declines the spawn: the field-computer press goes out and the sim returns him overhead, where
    /// the command bar's class set is his way back in (BO2: Back leaves you unspawned, and the
    /// pause menu's CHOOSE CLASS reopens the list).
    fn close_pick(&mut self, unclassed: bool) {
        self.class_pick_forced = false;
        if unclassed && matches!(self.view, ViewMode::Fps) {
            self.class_pick_declined = true;
            self.latch |= buttons::FIELD_COMPUTER;
        }
        self.cues.play(cues::BACKOUT);
    }

    /// First person with the spawn declined and the sim not yet handing the General back: the
    /// view holds still and only the leave press is sent.
    fn declining(&self, unclassed: bool) -> bool {
        unclassed && self.class_pick_declined
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
    /// first person the force opens it too, and an unclassed General always has it up (he is not
    /// spawned without a class) until a pick lands or Escape declines the spawn.
    fn class_pick_wanted(&self, unclassed: bool) -> bool {
        // A spectator never picks (it has no General of its own to send a class for).
        if self.spectator.is_some() {
            return false;
        }
        match self.view {
            // Overhead: only the command bar's force opens it (an unclassed player keeps playing).
            ViewMode::Rts => self.class_pick_forced,
            // First person: the force, or the spawn's screen while unclassed and not declined.
            ViewMode::Fps => self.class_pick_forced || (unclassed && !self.class_pick_declined),
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

/// The multiplayer autopilot's aim: the view angles from the local General's eye to the chest of
/// the nearest other General in first person (only those have a body the sim moves), or `None`.
#[cfg(feature = "autopilot")]
fn autopilot_aim(sim: &Sim, local: i32) -> Option<(f32, f32)> {
    let eye = sim.player(local)?.body.eye();
    let chest = |p: &crate::sim::PlayerSim| p.body.pos + Vec3::new(0.0, 0.0, p.body.eye_height * 0.75);
    let target = sim
        .players
        .iter()
        .enumerate()
        .filter(|(i, _)| *i as i32 != local)
        .filter_map(|(_, p)| p.as_ref())
        .filter(|p| p.is_first_person())
        .map(chest)
        .min_by(|a, b| (*a - eye).length().total_cmp(&(*b - eye).length()))?;
    let d = target - eye;
    Some((d.y.atan2(d.x), d.z.atan2(d.length_xy())))
}

impl Client {
    /// The autopilot's scripted input for this frame (`None`: the player's own). Its time base is
    /// the logic frame while it drives first person, and it re-bases outside it.
    #[cfg(feature = "autopilot")]
    fn autopilot_input(&mut self, sim: &Sim, inp: &ClientIn, unclassed: bool) -> Option<ClientIn> {
        let pick_wanted = self.class_pick_wanted(unclassed);
        // A spectator's view is the followed General's; the script never plays for him.
        let fps = matches!(self.view, ViewMode::Fps) && self.spectator.is_none();
        // How far the shot leaves from the aim right now (the view kick, the idle sway, the sights'
        // pose: `PlayerSim::fire_angles` against the input's angles), on the record first person
        // shows: the multiplayer script holds its sights on the other General through the recoil, as
        // a player does.
        let shown = self.predict.view().or_else(|| sim.player(inp.local_player));
        let off_aim = shown.map_or((0.0, 0.0), |p| {
            let (fy, fp) = p.fire_angles();
            (wrap_angle(fy - p.input.yaw), fp - p.input.pitch)
        });
        let ap = self.autopilot.as_mut()?;
        if !fps {
            ap.t = 0.0;
            ap.base_frame = inp.logic_frame;
            ap.in_fps = false;
            return None;
        }
        if !ap.in_fps {
            ap.in_fps = true;
            ap.session += 1;
            ap.mp_fight = None;
            ap.mp_leave_at = None;
            if ap.mp {
                let class = sim.player(inp.local_player).and_then(|p| p.class.clone()).unwrap_or_else(|| "none".into());
                ap.log.push(format!("autopilot mp: first-person session {} (class {class})", ap.session));
            }
        }
        if ap.mp {
            ap.mp_aim = autopilot_aim(sim, inp.local_player).map(|(yaw, pitch)| (wrap_angle(yaw - off_aim.0), pitch - off_aim.1));
            // What his General shows in third person on the other machines: the weapon in hand and
            // the camo it wears (`Sim::third_person_camo`, what their native pass composes).
            let holds = sim
                .player(inp.local_player)
                .filter(|p| p.class.is_some() && p.is_first_person())
                .map(|p| {
                    let vm = p.arsenal.current().def.viewmodel;
                    (p.general, vm, sim.third_person_camo(p.general, vm))
                });
            if holds.is_some() && holds != ap.mp_holds {
                if let Some((general, vm, camo)) = &holds {
                    ap.log.push(format!("autopilot mp: General {general} weapon {vm} camo {} in hand", camo.as_deref().unwrap_or("none")));
                }
                ap.mp_holds = holds;
            }
        }
        // The scripts' time windows are tuned in logic ticks (the sim's), so read the clock from
        // the logic frame, not the render dt: hitches and frame-rate differences would drift every
        // window relative to the state it aims at.
        ap.t = (inp.logic_frame - ap.base_frame) as f32 * LOGIC_DT;
        let script = ap.input_at(&self.cfg.bindings);
        let mut scripted = *inp;
        scripted.keys = script.keys;
        // A script's class-pick key is only pressed while the spawn pick is really up (a re-entry
        // on an already classed General must not turn it into a weapon swap).
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
        if let Some((yaw, pitch)) = script.look {
            self.yaw = wrap_angle(yaw);
            self.pitch = pitch.clamp(-MAX_PITCH, MAX_PITCH);
        }
        Some(scripted)
    }

    /// Without the autopilot feature the player's input is always the input.
    #[cfg(not(feature = "autopilot"))]
    fn autopilot_input(&mut self, _sim: &Sim, _inp: &ClientIn, _unclassed: bool) -> Option<ClientIn> {
        None
    }

    /// Run one rendered frame without the engine's logic at hand: nothing is predicted, first
    /// person follows the sim's record.
    pub fn frame(&mut self, sim: &mut Sim, inp: &ClientIn) -> ClientOut {
        self.frame_with(sim, None, inp)
    }

    /// Run one rendered frame. With `host` (the engine's logic, only read here) the local player's
    /// first person is predicted over his inputs in flight (`predict`), as BO2's client predicts
    /// its own player over the commands the server has not acknowledged.
    pub fn frame_with(&mut self, sim: &mut Sim, host: Option<&dyn LogicHost>, inp: &ClientIn) -> ClientOut {
        let dt = inp.dt.clamp(0.0, 0.25);
        self.ui_clock += dt;
        // The events since the last frame belong to the view that was presented then: the own
        // player's, or the General the spectator followed. A player who just became a spectator
        // (defeated as his General fell) still sees his own death.
        let own_before = self.spectator.is_none();
        let presented_before = if own_before { inp.local_player } else { self.viewed };
        self.set_spectating(inp.spectating != 0);
        // Who this frame presents: the local player, or the spectator's follow choice.
        let viewed = self.spectate(sim, inp, own_before && inp.spectating != 0);
        self.viewed = viewed;
        // The sim's record names the General the local player is commanding, in first person or
        // overhead. Adopt it before the class screen runs: a fresh General's first class pick can
        // come from the command bar before he was ever taken into first person, and the client
        // only learned his id on entering before — so that pick named General 0, the sim dropped
        // it, and the General kept the stock kit.
        let player = sim.player(viewed).cloned();
        if let Some(p) = &player {
            self.general = p.general;
        }
        let spectating = self.spectator.is_some();
        let unclassed = player.as_ref().is_none_or(|p| p.class.is_none());
        let scripted = self.autopilot_input(sim, inp, unclassed);
        let inp = scripted.as_ref().unwrap_or(inp);
        let mut out = ClientOut { hfov: self.last_hfov, viewmodel: viewmodel::NONE, ..Default::default() };
        self.hud.map_open = false;
        self.hud.scoreboard_open = false;

        // Track logic frames for eye interpolation: the render clock since the frame's logic step,
        // carried across steps (a frame rate that does not divide 30 Hz otherwise draws the eye
        // unevenly: zeroing it on the step's render frame dropped that frame's time), and one
        // frame's step back when the render missed some (not the eye of frames ago). Following, the
        // followed General's aim is tracked the same way.
        if let Some(p) = &player {
            if inp.logic_frame != self.seen_frame {
                let ticks = inp.logic_frame.wrapping_sub(self.seen_frame).clamp(1, 8);
                self.seen_frame = inp.logic_frame;
                self.since_logic = (self.since_logic + dt - LOGIC_DT * ticks as f32).clamp(0.0, LOGIC_DT);
                let eye = p.body.eye();
                self.eye_prev = if ticks > 1 { eye - (eye - self.eye_curr) * (1.0 / ticks as f32) } else { self.eye_curr };
                self.eye_curr = eye;
                self.kick_prev = self.kick_curr;
                // The sim's aim offset: the view kick and the scope's idle sway.
                let (yaw, pitch) = p.aim_offset();
                self.kick_curr = (yaw, pitch, p.view_kick.roll());
                self.aim_prev = self.aim_curr;
                self.aim_curr = (p.input.yaw, p.input.pitch);
            } else {
                self.since_logic = (self.since_logic + dt).min(LOGIC_DT);
            }
        }
        let alpha = (self.since_logic / LOGIC_DT).clamp(0.0, 1.0);
        let eye = self.eye_prev.lerp(self.eye_curr, alpha);

        // The prediction runs in first person on a classed General (behind the class screen
        // nothing goes out to predict), never for a spectator (it sends nothing: the record it shows
        // is the followed General's). The sim's record says which inputs it took in: they leave
        // the flight, and what their predictions presented is owed against the sim's own events.
        let predicting = host.is_some() && self.spectator.is_none() && matches!(self.view, ViewMode::Fps) && player.as_ref().is_some_and(|p| p.is_first_person() && p.class.is_some());
        let owed = match (&player, predicting) {
            (Some(p), true) => Some(self.predict.ack(p)),
            _ => {
                self.predict.reset();
                None
            }
        };
        self.tracers.predicted = predicting;
        self.tally_seats(sim, host, inp.local_player);
        self.consume_events(sim, inp, presented_before, own_before, player.as_ref().map(|p| p.body.pos), owed);
        self.remote_steps.update(&sim.players, viewed, &mut self.cues);
        self.remote.update(&sim.players, viewed, dt, &mut self.cues);
        crate::nativebody::ffi::publish_torso(self.remote.torsos());
        self.tick_hud(dt);
        // What first person shows: the prediction (last frame's, until this frame's below), else
        // the sim's record.
        let shown = self.predict.view().cloned().or_else(|| player.clone());
        self.hud.lui.tick(dt, shown.as_ref().filter(|p| p.is_first_person()));

        // A forced open (the lobby's Create a Class button or the command bar's entry) keeps the
        // screen up even when a class is already chosen, so a kit can be replaced.
        if inp.class_pick_force != 0 && !spectating {
            self.class_pick_forced = true;
        }

        // The command bar's class buttons. The engine dispatches a bar click from its own command
        // processing, at a different point in the frame from this call, so the bridge hands the
        // button over as a one-shot and the pick is emitted here. (Queueing the pick into
        // `ClientOut` from the bridge instead would be dropped: every `zhbo_client_frame` writes a
        // fresh whole struct over the bridge's copy before it can read the pick.)
        if inp.class_button > 0 && !spectating {
            self.pick_class_button(&mut out, inp.class_button);
        }

        // The class pick's entrance: BO2's in-game class menu opens with `uin_main_pause`
        // (`hud/class.lua`), once per open. The screen only shows when `class_pick_wanted` says so
        // (the command-bar force in either view; first person's spawn screen while unclassed and not
        // declined).
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
        let hfov = horizontal_fov(self.view_fov(dt, shown.as_ref()), aspect);
        self.last_hfov = hfov;

        let first_person = player.as_ref().is_some_and(|p| p.is_first_person());
        let allowed = inp.input_allowed != 0;
        // Out of first person nothing presents a shock: its ringing stops with the view.
        if !matches!(self.view, ViewMode::Fps) || !first_person {
            let loops = &mut self.cues.loops;
            self.shock.stop(&mut |e, g| loops.push((e, g)));
        }

        match self.view {
            ViewMode::Rts => {
                if first_person {
                    let p = player.as_ref().unwrap();
                    self.start_entering(inp, p.general, p.yaw, p.body.eye());
                    if spectating {
                        // Fly into the followed General's eyes at his own aim.
                        self.snap_follow(p);
                    }
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
                if spectating {
                    self.follow_aim(alpha);
                }
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
                    // A declined spawn holds the view still too, until the sim hands him back.
                    let menu = self.class_pick_wanted(unclassed) || self.declining(unclassed);
                    if spectating {
                        // Following: his aim, as the sim stepped it, smoothed between logic
                        // frames. The spectator's own mouse never turns the view.
                        self.follow_aim(alpha);
                    } else if allowed && !in_fc && !menu {
                        let zoom = hfov / base_fov;
                        // A shellshock's look control (`crate::shellshock`, t6mp `FUN_005109f0`):
                        // the sensitivity scaled and the turn rate capped, both fading back.
                        let look = p.shock.look();
                        let sens = self.cfg.sensitivity * zoom * look.sensitivity;
                        let inv = if self.cfg.invert_y { -1.0 } else { 1.0 };
                        let cap = |d: f32, max_deg_s: f32| if max_deg_s.is_finite() { d.clamp(-(max_deg_s.to_radians() * dt), max_deg_s.to_radians() * dt) } else { d };
                        self.yaw = wrap_angle(self.yaw + cap(-inp.mouse_dx * sens, look.max_yaw_speed));
                        self.pitch = (self.pitch + cap(-inp.mouse_dy * sens * inv, look.max_pitch_speed)).clamp(-MAX_PITCH, MAX_PITCH);
                    }
                    // This frame's input goes out first: the prediction steps it at once. A
                    // spectator sends nothing.
                    if !spectating {
                        self.build_input(inp, &mut out, allowed && !in_fc, p.hammer.is_some(), unclassed, sim.frame);
                    }
                    let predicted = match (predicting, host) {
                        (true, Some(host)) => {
                            let sent = (out.send_input != 0).then_some((out.input_seen_frame, out.input));
                            let adv = self.predict.advance(sim, host, inp.local_player.max(0) as usize, sent, dt);
                            let view = self.predict.view().cloned();
                            self.present_predicted(inp, view.as_ref(), adv);
                            view
                        }
                        _ => None,
                    };
                    let p: &PlayerSim = predicted.as_ref().unwrap_or(p);
                    // The view kick moves the camera with the aim (BO2 adds it to the view angles):
                    // the sim's, interpolated across the logic frame like the eye; predicted, the
                    // prediction's last two frames at its render clock.
                    let (eye, kick) = match (predicted.is_some(), self.predict.eye(), self.predict.view_prev()) {
                        (true, Some(e), Some(prev)) => {
                            let a = self.predict.alpha();
                            let ((py, pp), (ny, np)) = (prev.aim_offset(), p.aim_offset());
                            let (pr, nr) = (prev.view_kick.roll(), p.view_kick.roll());
                            (e, (py + (ny - py) * a, pp + (np - pp) * a, pr + (nr - pr) * a))
                        }
                        _ => {
                            let lerp = |a: f32, b: f32| a + (b - a) * alpha;
                            (eye, (lerp(self.kick_prev.0, self.kick_curr.0), lerp(self.kick_prev.1, self.kick_curr.1), lerp(self.kick_prev.2, self.kick_curr.2)))
                        }
                    };
                    out.camera_override = 1;
                    out.eye = eye;
                    let kick_yaw = kick.0;
                    let kick_pitch = kick.1 + self.hud.flinch_pitch.to_radians();
                    let kick_roll = kick.2;
                    // A shellshock's view kick turns the view only (t6mp `FUN_00510270` turns the
                    // refdef's axes; the aim, which the input carries, is untouched).
                    let loops = &mut self.cues.loops;
                    self.shock.update(&p.shock, self.since_logic, dt, &mut |e, g| loops.push((e, g)));
                    let (shock_yaw, shock_pitch) = self.shock.kick(p.arsenal.tuning.shock_view_kick_fade);
                    out.dir = dir_from_angles(self.yaw + kick_yaw + shock_yaw, (self.pitch + kick_pitch + shock_pitch).clamp(-MAX_PITCH, MAX_PITCH));
                    out.view_roll = kick_roll;
                    out.hfov = hfov;
                    out.hide_rts_ui = 1;
                    out.hide_object = self.general;
                    // A spectator never takes the mouse: it plays nothing.
                    out.capture_mouse = (allowed && !menu && !spectating) as i32;
                    self.view_eye = out.eye;
                    self.view_dir = out.dir;
                    self.hud.map_open = allowed && !in_fc && !menu && !spectating && inp.keys.is_down(self.cfg.bindings.map);
                    self.hud.scoreboard_open = allowed && !menu && !spectating && inp.keys.is_down(self.cfg.bindings.scoreboard);
                    let events = std::mem::take(&mut self.vm_events);
                    // The viewmodel's motion reads the def's gun-position numbers (G4) and the
                    // sim's own stance/speed/aim; the view's `cg_fov` is G4's wide-FOV reference.
                    let pose = self.vm_motion.update(dt, self.yaw, self.pitch, self.cfg.cg_fov, p, self.vm_kick.take());
                    out.viewmodel_offset = pose.offset;
                    // The gun is placed in the unrolled eye frame (`nativevm::place::root`, the W3D
                    // pose): it rolls with the camera (BO2 draws it in the view's axes), so it
                    // stays square to the screen.
                    out.viewmodel_angles = pose.angles + Vec3::new(0.0, 0.0, kick_roll);
                    self.cues.update(dt, p, p.input.held & buttons::FIRE != 0);
                    if unclassed {
                        // Not spawned: nothing in hand behind the class screen (BO2 shows no
                        // viewmodel before the class spawns the player).
                    } else if let Mode::FieldComputer { t } = p.mode {
                        // A scorestreak: the tablet comes up before the view leaves.
                        let (clip, time) = viewmodel::tablet_up(t);
                        out.viewmodel = clip;
                        out.viewmodel_time = time;
                    } else {
                        out.viewmodel_fired = events.shot as i32;
                        out.viewmodel_fired_left = events.left_shot as i32;
                        let f = self.vm.update(dt, p, events);
                        out.viewmodel = f.clip;
                        out.viewmodel_weapon = f.model;
                        out.viewmodel_time = f.t;
                        out.viewmodel_ads = p.arsenal.ads;
                        crate::nativevm::ffi::set_left_clip(f.left);
                    }
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
                    // Back overhead alive (the field computer, or a declined spawn): he is selected
                    // again, as he was when Take Control was pressed (entering deselects him). A
                    // dead General has no record and is not.
                    if !spectating && player.as_ref().is_some_and(|p| p.general == self.general && !p.is_first_person()) {
                        out.select_object = self.general;
                    }
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
        // The frame's impacts (consume_events and the prediction filled them above): the
        // presented seat's own, then other players' into the slots left.
        out.hits = self.hits;
        let mut n = self.hits_len;
        for hit in self.remote_hits.drain(..) {
            if n == CLIENT_HITS {
                break;
            }
            out.hits[n] = hit;
            n += 1;
        }
        out.hits_len = n as i32;
        // A fresh open starts `animateIn`'s entrance fade (`codmenu.lua:2300-2326`); a screen that
        // stays up keeps its clock, and a closed one starts over on the next open.
        if self.class_pick_open && !was_open {
            self.class_pick_since = Some(self.ui_clock);
        }
        // The fresh-press/hover detectors compare against this frame's keys and cursor.
        self.menu_prev.update(&menu_pointer(inp));
        // The own gun's tracers queued by the logic steps since the last frame are this frame's,
        // drawn from the viewmodel's flash tag (`zhbo_viewmodel_tracer`) while it shows.
        self.tracers.begin_frame(out.camera_override != 0 && out.hide_object != 0 && out.viewmodel >= 0);
        self.tracers.viewer = tracers::Viewer { player: viewed, general: self.general, first_person: out.hide_object };
        out
    }

    /// The view FOV this frame (degrees at 4:3): `cg_fov` through BO2's ADS zoom (`weapons::zoom`,
    /// `FUN_004a0380`) on the held weapon's sim ADS fraction and zoom level, then `cg_fovScale` and
    /// `cg_fovMin` (`FUN_004a0920`).
    fn view_fov(&mut self, dt: f32, player: Option<&crate::sim::PlayerSim>) -> f32 {
        let hip = self.cfg.cg_fov;
        let Some(p) = player else { return hip };
        let a = &p.arsenal;
        let w = a.current();
        self.zoom_view.step(dt, w.def, w.alt_up(), a.ads);
        let fov = self.zoom_view.fov(hip, w.def, a.ads, w.zoom);
        zoom::view_fov(fov, a.tuning.cg_fov_scale, a.tuning.cg_fov_min)
    }

    fn start_entering(&mut self, inp: &ClientIn, general: u32, yaw: f32, eye: Vec3) {
        self.general = general;
        self.vm = Viewmodel::default(); // the weapon comes up as first person starts
        self.vm_motion = viewmodel::Motion::default();
        self.vm_kick = None;
        // A fresh take-control puts the spawn's class screen up again if the last one was declined.
        self.class_pick_declined = false;
        self.cues.reset();
        self.yaw = yaw;
        self.pitch = 0.0;
        self.eye_prev = eye;
        self.eye_curr = eye;
        self.kick_prev = (0.0, 0.0, 0.0);
        self.kick_curr = (0.0, 0.0, 0.0);
        self.latch = 0;
        self.prev_held = u32::MAX; // ignore keys already held when entering (e.g. the click itself)
        let from_dir = if inp.rts_cam_dir.length() > 0.5 { inp.rts_cam_dir } else { Vec3::new(0.0, 0.6, -0.8) };
        let from_hfov = if inp.rts_hfov > 0.05 { inp.rts_hfov } else { self.last_hfov };
        self.overhead = Some((inp.rts_cam_pos - inp.rts_pivot, from_dir.normalized_or_zero(), from_hfov));
        self.view = ViewMode::Entering { t: 0.0, from_pos: inp.rts_cam_pos, from_dir: from_dir.normalized_or_zero(), from_hfov };
    }

    /// This frame's first-person input (`seen_frame`: the sim frame on screen, carried with it).
    fn build_input(&mut self, inp: &ClientIn, out: &mut ClientOut, allowed: bool, hammer: bool, unclassed: bool, seen_frame: u32) {
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
        // A declined spawn sends nothing but the leave press `close_pick` latched.
        if self.declining(unclassed) {
            held = 0;
            fwd = 0.0;
            right = 0.0;
        }
        if allowed && inp.wheel.abs() >= 1.0 && !self.declining(unclassed) {
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
            out.input_seen_frame = seen_frame;
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

    fn begin_leaving(&mut self, inp: &ClientIn, target: Vec3) {
        // From where first person's camera was (the prediction's eye while predicting).
        let from_pos = if matches!(self.view, ViewMode::Fps) && self.predict.active() { self.view_eye } else { self.eye_curr };
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
        // The hint is cleared once its 1.8 s are up (`FUN_004ddc80`: start + 0x708 ms < now).
        h.hint = h.hint.map(|(k, t)| (k, t + dt)).filter(|&(_, t)| t <= crate::hud::HINT_TIME);
        h.hack_shown = (h.hack_shown - dt).max(0.0);
        let decay = (1.0 - dt * 12.0).max(0.0);
        h.flinch_pitch *= decay;
    }
}

/// The client's tests live in their own file: `mod.rs` is at the module-size limit.
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
