//! Per-render-frame client logic for the local player: mouse look, key bindings, sending one
//! [`FpsInput`] per logic frame, the camera (with RTS <-> first-person transitions), and HUD timers.
//!
//! Nothing here affects the lockstep simulation except through the input messages it emits.

use crate::input::{buttons, pack_move, FpsInput, MAX_PITCH};
use crate::math::{dir_from_angles, lerp_angle, smoothstep, wrap_angle, Vec3};
use crate::sim::{EventKind, Mode, Sim};
use crate::sounds::{self, Cues};
use crate::units::LOGIC_DT;
use crate::viewmodel::{self, Viewmodel};

/// Windows virtual-key codes.
pub mod vk {
    pub const LBUTTON: u8 = 0x01;
    pub const RBUTTON: u8 = 0x02;
    pub const SHIFT: u8 = 0x10;
    pub const CONTROL: u8 = 0x11;
    pub const TAB: u8 = 0x09;
    pub const SPACE: u8 = 0x20;
    pub const KEY_1: u8 = 0x31;
    pub const KEY_2: u8 = 0x32;
    pub const KEY_3: u8 = 0x33;
    pub const KEY_4: u8 = 0x34;
    pub const KEY_5: u8 = 0x35;
    pub const A: u8 = 0x41;
    pub const C: u8 = 0x43;
    pub const D: u8 = 0x44;
    pub const E: u8 = 0x45;
    pub const F: u8 = 0x46;
    pub const G: u8 = 0x47;
    pub const Q: u8 = 0x51;
    pub const R: u8 = 0x52;
    pub const S: u8 = 0x53;
    pub const V: u8 = 0x56;
    pub const W: u8 = 0x57;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bindings {
    pub forward: u8,
    pub back: u8,
    pub left: u8,
    pub right: u8,
    pub fire: u8,
    pub ads: u8,
    pub sprint: u8,
    pub crouch: u8,
    pub prone: u8,
    pub jump: u8,
    pub reload: u8,
    pub swap: u8,
    pub melee: u8,
    pub field_computer: u8,
    /// Throw lethal / tactical (hold to cook).
    pub lethal: u8,
    pub tactical: u8,
    /// Held: the whole map (BO2's scoreboard key).
    pub map: u8,
    /// BO2's action slot 3: the weapon's alt mode (select fire, grenade launcher, second optic).
    pub alt_mode: u8,
}

impl Default for Bindings {
    /// BO2 PC defaults, with the scorestreak/"use" key repurposed for the field computer.
    fn default() -> Self {
        Self {
            forward: vk::W,
            back: vk::S,
            left: vk::A,
            right: vk::D,
            fire: vk::LBUTTON,
            ads: vk::RBUTTON,
            sprint: vk::SHIFT,
            crouch: vk::C,
            prone: vk::CONTROL,
            jump: vk::SPACE,
            reload: vk::R,
            swap: vk::KEY_1,
            melee: vk::V,
            field_computer: vk::F,
            lethal: vk::G,
            tactical: vk::Q,
            map: vk::TAB,
            alt_mode: vk::KEY_3,
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
/// pulls back behind the General, then swings round to the overhead camera's own angle. Each is two
/// phases, split at `TRANSITION_SPLIT`.
pub const ENTER_TIME: f32 = 1.4;
pub const LEAVE_TIME: f32 = 1.6;
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
    /// `GetAsyncKeyState` high bits for every virtual key (non-zero = down).
    pub keys: [u8; 256],
    pub mouse_dx: f32,
    pub mouse_dy: f32,
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
}

impl Default for ClientIn {
    fn default() -> Self {
        Self {
            dt: 1.0 / 60.0,
            local_player: 0,
            logic_frame: 0,
            keys: [0; 256],
            mouse_dx: 0.0,
            mouse_dy: 0.0,
            wheel: 0.0,
            input_allowed: 1,
            screen_w: 1280.0,
            screen_h: 720.0,
            rts_cam_pos: Vec3::new(0.0, -200.0, 300.0),
            rts_cam_dir: Vec3::new(0.0, 0.6, -0.8),
            rts_hfov: 0.87,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
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
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewMode {
    Rts,
    Entering { t: f32, from_pos: Vec3, from_dir: Vec3, from_hfov: f32 },
    Fps,
    /// `to_*`: the overhead camera's pose once it is centred on `target`.
    Leaving { t: f32, from_pos: Vec3, from_dir: Vec3, from_hfov: f32, target: Vec3, to_pos: Vec3, to_dir: Vec3, to_hfov: f32 },
}

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
}

/// Scripted input for in-game verification (`ZHBO_AUTOPILOT=1`): walks, turns, aims and fires,
/// changes stance, jumps, glances at the map, swaps to the pistol and fires it, swaps back, knifes,
/// cooks and throws a frag, throws a concussion, fires the rifle's grenade launcher, then opens the
/// field computer. Times are seconds in first person.
///
/// `ZHBO_AUTOPILOT=equipment` runs [`Autopilot::equipment_input`] instead, for a loadout with C4 and
/// a Trophy System or Black Hat.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Autopilot {
    pub t: f32,
    pub equipment: bool,
}

impl Autopilot {
    /// Keys held and mouse delta (counts per frame) at time `t`.
    pub fn input_at(&self, b: &Bindings) -> (Vec<u8>, f32) {
        if self.equipment {
            return (self.equipment_input(b), 0.0);
        }
        let t = self.t;
        let mut keys = Vec::new();
        let mut dx = 0.0;
        if (1.0..3.0).contains(&t) {
            keys.push(b.forward);
        }
        if (2.0..2.1).contains(&t) || (2.8..2.9).contains(&t) {
            keys.push(b.crouch);
        }
        if (3.0..3.5).contains(&t) {
            dx = 12.0;
        }
        if (3.5..4.0).contains(&t) {
            dx = -12.0;
        }
        if (3.6..3.7).contains(&t) {
            keys.push(b.jump);
        }
        if (4.0..4.4).contains(&t) {
            keys.push(b.map);
        }
        if (4.5..7.5).contains(&t) {
            keys.push(b.ads);
        }
        if (5.0..7.0).contains(&t) {
            keys.push(b.fire);
        }
        if (7.2..7.3).contains(&t) || (8.4..8.5).contains(&t) {
            keys.push(b.swap);
        }
        if (7.8..7.9).contains(&t) || (8.1..8.2).contains(&t) {
            keys.push(b.fire);
        }
        if (9.2..9.3).contains(&t) {
            keys.push(b.melee);
        }
        if (9.9..10.3).contains(&t) {
            keys.push(b.lethal);
        }
        // After the frag has left the hand and the rifle is coming back up.
        if (11.2..11.3).contains(&t) {
            keys.push(b.tactical);
        }
        // The rifle's grenade launcher: switch to it, fire, switch back.
        if (12.4..12.5).contains(&t) || (13.9..14.0).contains(&t) {
            keys.push(b.alt_mode);
        }
        if (13.3..13.4).contains(&t) {
            keys.push(b.fire);
        }
        if (14.6..14.7).contains(&t) {
            keys.push(b.field_computer);
        }
        (keys, dx)
    }

    /// The equipment run: throw the lethal (C4), hold the tactical up for four seconds (the Black
    /// Hat on the drone the test puts in front; a Trophy System goes down when it is let go),
    /// double-tap reload to set the C4 off, fire a burst and reload, wait while enemy rockets come
    /// in, then open the field computer.
    pub fn equipment_input(&self, b: &Bindings) -> Vec<u8> {
        let t = self.t;
        let mut keys = Vec::new();
        if (0.5..1.0).contains(&t) {
            keys.push(b.lethal);
        }
        if (2.0..6.0).contains(&t) {
            keys.push(b.tactical);
        }
        if (7.0..7.08).contains(&t) || (7.2..7.28).contains(&t) {
            keys.push(b.reload);
        }
        // A burst and a reload (the reload animation's magazines).
        if (9.0..10.0).contains(&t) {
            keys.push(b.fire);
        }
        if (10.3..10.4).contains(&t) {
            keys.push(b.reload);
        }
        if (20.0..20.1).contains(&t) {
            keys.push(b.field_computer);
        }
        keys
    }
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

pub struct Client {
    pub cfg: ClientConfig,
    pub autopilot: Option<Autopilot>,
    pub view: ViewMode,
    pub yaw: f32,
    pub pitch: f32,
    pub general: u32,
    pub hud: HudTimers,
    last_sent_frame: Option<u32>,
    latch: u32,
    prev_held: u32,
    eye_prev: Vec3,
    eye_curr: Vec3,
    seen_frame: u32,
    since_logic: f32,
    pub last_hfov: f32,
    /// The first-person camera this frame (for HUD marks over the world).
    pub view_eye: Vec3,
    pub view_dir: Vec3,
    /// First-person viewmodel animation, and this frame's weapon events for it.
    vm: Viewmodel,
    vm_motion: viewmodel::Motion,
    vm_events: viewmodel::Events,
    /// BO2 sound cues queued this frame (drained by the FFI layer into the host).
    pub cues: Cues,
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
            view_eye: Vec3::ZERO,
            view_dir: Vec3::new(1.0, 0.0, 0.0),
            vm: Viewmodel::default(),
            vm_motion: viewmodel::Motion::default(),
            vm_events: viewmodel::Events::default(),
            cues: Cues::default(),
        }
    }

    pub fn reset(&mut self) {
        let autopilot = self.autopilot.as_ref().map(|a| Autopilot { equipment: a.equipment, ..Default::default() });
        *self = Client::new(self.cfg);
        self.autopilot = autopilot;
    }

    fn read_buttons(&self, inp: &ClientIn) -> (u32, f32, f32) {
        let b = &self.cfg.bindings;
        let down = |k: u8| inp.keys[k as usize] != 0;
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
        ] {
            if down(key) {
                held |= bit;
            }
        }
        let fwd = (down(b.forward) as i32 - down(b.back) as i32) as f32;
        let right = (down(b.right) as i32 - down(b.left) as i32) as f32;
        (held, fwd, right)
    }

    /// Run one rendered frame.
    pub fn frame(&mut self, sim: &mut Sim, inp: &ClientIn) -> ClientOut {
        let dt = inp.dt.clamp(0.0, 0.25);
        let mut scripted;
        let inp = match self.autopilot.as_mut() {
            Some(ap) if matches!(self.view, ViewMode::Fps) => {
                ap.t += dt;
                let (keys, dx) = ap.input_at(&self.cfg.bindings);
                scripted = *inp;
                scripted.keys = [0; 256];
                for k in keys {
                    scripted.keys[k as usize] = 1;
                }
                scripted.mouse_dx = dx;
                scripted.mouse_dy = 0.0;
                scripted.input_allowed = 1;
                &scripted
            }
            Some(ap) => {
                ap.t = 0.0;
                inp
            }
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
                // Swing round the General to behind their view, at the overhead camera's
                // distance and height, then move in to the eyes.
                let orbit = Orbit::around(eye, from_pos);
                let behind = self.yaw + std::f32::consts::PI;
                let to_dir = dir_from_angles(self.yaw, self.pitch);
                if u < TRANSITION_SPLIT {
                    let s = smoothstep(u / TRANSITION_SPLIT);
                    let pos = orbit.at(lerp_angle(orbit.azimuth, behind, s));
                    out.eye = pos;
                    out.dir = from_dir.lerp((eye - pos).normalized_or_zero(), s).normalized_or_zero();
                } else {
                    let s = smoothstep((u - TRANSITION_SPLIT) / (1.0 - TRANSITION_SPLIT));
                    let start = orbit.at(behind);
                    out.eye = start.lerp(eye, s);
                    out.dir = (eye - start).normalized_or_zero().lerp(to_dir, s).normalized_or_zero();
                }
                out.camera_override = 1;
                out.hfov = from_hfov + (hfov - from_hfov) * smoothstep(u);
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
                    self.hud.map_open = allowed && !in_fc && inp.keys[self.cfg.bindings.map as usize] != 0;
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
                    self.build_input(inp, &mut out, allowed && !in_fc);
                }
            }
            ViewMode::Leaving { t, from_pos, from_dir, from_hfov, target, to_pos, to_dir, to_hfov } => {
                let t = t + dt;
                let u = (t / LEAVE_TIME).min(1.0);
                // Pull back behind the General (along the view) to the overhead camera's distance
                // and height, then swing round to its angle: it ends on the overhead camera's pose.
                let orbit = Orbit::around(target, to_pos);
                let behind = self.yaw + std::f32::consts::PI;
                if u < TRANSITION_SPLIT {
                    let s = smoothstep(u / TRANSITION_SPLIT);
                    let pos = from_pos.lerp(orbit.at(behind), s);
                    out.eye = pos;
                    out.dir = from_dir.lerp((target - pos).normalized_or_zero(), s).normalized_or_zero();
                } else {
                    let s = smoothstep((u - TRANSITION_SPLIT) / (1.0 - TRANSITION_SPLIT));
                    let pos = orbit.at(lerp_angle(behind, orbit.azimuth, s));
                    out.eye = pos;
                    // Looking at the General, settling on the overhead camera's own direction.
                    out.dir = (target - pos).normalized_or_zero().lerp(to_dir, s * s).normalized_or_zero();
                }
                out.camera_override = 1;
                out.hfov = from_hfov + (to_hfov - from_hfov) * smoothstep(u);
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
        self.view = ViewMode::Entering { t: 0.0, from_pos: inp.rts_cam_pos, from_dir: from_dir.normalized_or_zero(), from_hfov };
    }

    fn build_input(&mut self, inp: &ClientIn, out: &mut ClientOut, allowed: bool) {
        let (mut held, mut fwd, mut right) = if allowed { self.read_buttons(inp) } else { (0, 0.0, 0.0) };
        if !allowed {
            fwd = 0.0;
            right = 0.0;
        }
        if allowed && inp.wheel.abs() >= 1.0 {
            self.latch |= buttons::SWAP_WEAPON;
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
            }
        }
    }

    fn begin_leaving(&mut self, inp: &ClientIn, target: Vec3) {
        let from_pos = self.eye_curr;
        let from_dir = dir_from_angles(self.yaw, self.pitch);
        // Where the overhead camera will be once centred on the General: its angle, height and
        // zoom are its own, only its look-at point moves to the General.
        let dir = if inp.rts_cam_dir.length() > 0.5 { inp.rts_cam_dir.normalized_or_zero() } else { Vec3::new(0.0, 0.6, -0.8) };
        let down = (-dir.z).max(0.2);
        let along = ((inp.rts_cam_pos.z - target.z) / down).clamp(50.0, 2000.0);
        let to_pos = target - dir * along;
        let to_hfov = if inp.rts_hfov > 0.05 { inp.rts_hfov } else { self.last_hfov };
        self.view = ViewMode::Leaving { t: 0.0, from_pos, from_dir, from_hfov: self.last_hfov, target, to_pos, to_dir: dir, to_hfov };
        self.last_sent_frame = None;
    }

    fn tick_hud(&mut self, dt: f32) {
        let h = &mut self.hud;
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
        inp.keys[vk::W as usize] = 0x80;
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
        inp.keys[vk::TAB as usize] = 0x80;
        frame_with(&mut client, &mut sim, &host, &mut inp, 1);
        assert!(client.hud.map_open);
        inp.keys[vk::TAB as usize] = 0;
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
        inp.keys[vk::F as usize] = 0x80;
        frame_with(&mut client, &mut sim, &host, &mut inp, 2);
        inp.keys[vk::F as usize] = 0;
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
        // Leave through the field computer.
        inp.keys[vk::F as usize] = 0x80;
        frame_with(&mut client, &mut sim, &host, &mut inp, 2);
        inp.keys[vk::F as usize] = 0;
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
        assert!((first.eye - inp.rts_cam_pos).length() < 15.0 && first.dir.dot(rts_dir) > 0.97, "starts at the overhead camera");
        assert!((first.hfov - 0.8).abs() < 0.05);
        let last = leaving.last().unwrap();
        assert!(last.dir.dot(rts_dir) > 0.995, "ends looking the overhead camera's way: {:?}", last.dir);
        assert!((last.hfov - 0.8).abs() < 0.02, "ends at the overhead field of view: {}", last.hfov);
    }

    #[test]
    fn hor_plus_fov() {
        let f43 = horizontal_fov(65.0, 4.0 / 3.0).to_degrees();
        let f169 = horizontal_fov(65.0, 16.0 / 9.0).to_degrees();
        assert!((f43 - 65.0).abs() < 0.01);
        assert!(f169 > 80.0 && f169 < 82.0, "{f169}");
    }
}
