//! Per-render-frame client logic for the local player: mouse look, key bindings, sending one
//! [`FpsInput`] per logic frame, the camera (with RTS <-> first-person transitions), and HUD timers.
//!
//! Nothing here affects the lockstep simulation except through the input messages it emits.

use crate::input::{buttons, pack_move, FpsInput, MAX_PITCH};
use crate::math::{basis_xy, dir_from_angles, smoothstep, wrap_angle, Vec3};
use crate::sim::{EventKind, Mode, Sim};
use crate::units::LOGIC_DT;

/// Windows virtual-key codes.
pub mod vk {
    pub const LBUTTON: u8 = 0x01;
    pub const RBUTTON: u8 = 0x02;
    pub const SHIFT: u8 = 0x10;
    pub const CONTROL: u8 = 0x11;
    pub const SPACE: u8 = 0x20;
    pub const KEY_1: u8 = 0x31;
    pub const KEY_2: u8 = 0x32;
    pub const A: u8 = 0x41;
    pub const C: u8 = 0x43;
    pub const D: u8 = 0x44;
    pub const E: u8 = 0x45;
    pub const F: u8 = 0x46;
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

pub const ENTER_TIME: f32 = 0.9;
pub const LEAVE_TIME: f32 = 1.1;
pub const HITMARKER_TIME: f32 = 0.25;
pub const DEATH_BANNER_TIME: f32 = 3.0;

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
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewMode {
    Rts,
    Entering { t: f32, from_pos: Vec3, from_dir: Vec3 },
    Fps,
    Leaving { t: f32, from_pos: Vec3, from_dir: Vec3, target: Vec3 },
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
}

pub struct Client {
    pub cfg: ClientConfig,
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
        }
    }

    pub fn reset(&mut self) {
        *self = Client::new(self.cfg);
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
        let mut out = ClientOut { hfov: self.last_hfov, ..Default::default() };
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
        let ads_scale = player.as_ref().map_or(1.0, |p| p.arsenal.current().def.ads_fov_scale);
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
            ViewMode::Entering { t, from_pos, from_dir } => {
                let t = t + dt;
                let s = smoothstep(t / ENTER_TIME);
                let to_dir = dir_from_angles(self.yaw, self.pitch);
                out.camera_override = 1;
                out.eye = from_pos.lerp(eye, s);
                out.dir = from_dir.lerp(to_dir, s).normalized_or_zero();
                out.hfov = hfov;
                out.hide_rts_ui = 1;
                if s > 0.8 {
                    out.hide_object = self.general;
                }
                self.view = if t >= ENTER_TIME { ViewMode::Fps } else { ViewMode::Entering { t, from_pos, from_dir } };
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
                    self.build_input(inp, &mut out, allowed && !in_fc);
                }
            }
            ViewMode::Leaving { t, from_pos, from_dir, target } => {
                let t = t + dt;
                let s = smoothstep(t / LEAVE_TIME);
                let (fwd, _) = basis_xy(self.yaw);
                let to_pos = target - fwd * 140.0 + Vec3::new(0.0, 0.0, 190.0);
                let to_dir = (target - to_pos).normalized_or_zero();
                out.camera_override = 1;
                out.eye = from_pos.lerp(to_pos, s);
                out.dir = from_dir.lerp(to_dir, s).normalized_or_zero();
                out.hfov = hfov;
                if t >= LEAVE_TIME {
                    self.view = ViewMode::Rts;
                    out.camera_override = 0;
                    out.rts_look_at_valid = 1;
                    out.rts_look_at = target;
                } else {
                    self.view = ViewMode::Leaving { t, from_pos, from_dir, target };
                }
            }
        }
        out
    }

    fn start_entering(&mut self, inp: &ClientIn, general: u32, yaw: f32, eye: Vec3) {
        self.general = general;
        self.yaw = yaw;
        self.pitch = 0.0;
        self.eye_prev = eye;
        self.eye_curr = eye;
        self.latch = 0;
        self.prev_held = u32::MAX; // ignore keys already held when entering (e.g. the click itself)
        let from_dir = if inp.rts_cam_dir.length() > 0.5 { inp.rts_cam_dir } else { Vec3::new(0.0, 0.6, -0.8) };
        self.view = ViewMode::Entering { t: 0.0, from_pos: inp.rts_cam_pos, from_dir: from_dir.normalized_or_zero() };
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
        for kind in mine {
            match kind {
                EventKind::EnteredFps { general } => {
                    if let Some(p) = sim.player(me) {
                        self.start_entering(inp, general, p.yaw, p.body.eye());
                    }
                }
                EventKind::ReturnedToRts { general } => {
                    self.general = general;
                    self.begin_leaving(body_pos.unwrap_or(self.eye_curr));
                }
                EventKind::GeneralDied { .. } => {
                    self.hud.death_banner = DEATH_BANNER_TIME;
                    if !matches!(self.view, ViewMode::Rts) {
                        self.begin_leaving(self.eye_curr - Vec3::new(0.0, 0.0, 10.0));
                    }
                }
                EventKind::HitMarker { headshot, kill } => {
                    self.hud.hitmarker = HITMARKER_TIME;
                    self.hud.hitmarker_kill = kill;
                    self.hud.hitmarker_head = headshot;
                }
                EventKind::Recoil { pitch_deg, yaw_deg } => {
                    // BO2 recoil moves the real aim; a little extra visual kick decays on top.
                    self.pitch = (self.pitch + pitch_deg.to_radians()).clamp(-MAX_PITCH, MAX_PITCH);
                    self.yaw = wrap_angle(self.yaw + yaw_deg.to_radians());
                    self.hud.view_kick_pitch += pitch_deg * 0.5;
                    self.hud.view_kick_yaw += yaw_deg * 0.5;
                    self.hud.muzzle = 0.05;
                }
                EventKind::Damaged { amount } => {
                    self.hud.damage_flash = (self.hud.damage_flash + amount / 60.0).min(1.0);
                }
                EventKind::FieldComputerOpened
                | EventKind::Shot { .. }
                | EventKind::ReloadStarted
                | EventKind::Swapped => {}
            }
        }
    }

    fn begin_leaving(&mut self, target: Vec3) {
        let from_pos = self.eye_curr;
        let from_dir = dir_from_angles(self.yaw, self.pitch);
        self.view = ViewMode::Leaving { t: 0.0, from_pos, from_dir, target };
        self.last_sent_frame = None;
    }

    fn tick_hud(&mut self, dt: f32) {
        let h = &mut self.hud;
        h.hitmarker = (h.hitmarker - dt).max(0.0);
        h.damage_flash = (h.damage_flash - dt * 0.6).max(0.0);
        h.death_banner = (h.death_banner - dt).max(0.0);
        h.muzzle = (h.muzzle - dt).max(0.0);
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
        frame_with(&mut client, &mut sim, &host, &mut inp, 30);
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

    #[test]
    fn hor_plus_fov() {
        let f43 = horizontal_fov(65.0, 4.0 / 3.0).to_degrees();
        let f169 = horizontal_fov(65.0, 16.0 / 9.0).to_degrees();
        assert!((f43 - 65.0).abs() < 0.01);
        assert!(f169 > 80.0 && f169 < 82.0, "{f169}");
    }
}
