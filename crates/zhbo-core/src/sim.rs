//! The deterministic, lockstep part of the mod: one [`PlayerSim`] per player whose General is under
//! first-person control. Driven by `zhbo_logic_*` calls from `GameLogic`; must produce identical
//! results on every client given identical inputs.

use crate::host::{damage_kind, geom, objflags, LogicHost, ObjInfo};
use crate::hitscan::{self, HitResult, Shape, Target, INFANTRY_HEAD_FRACTION, INFANTRY_HIT_RADIUS};
use crate::input::{buttons, FpsInput};
use crate::math::{basis_xy, dir_from_angles, Vec3};
use crate::movement::{self, Blocker, Body, MoveModifiers, MoveTuning, World};
use crate::units::{BO2_TO_ZH_DAMAGE, LOGIC_DT};
use crate::movement::Stance;
use crate::attachments::FMJ_VEHICLE_DAMAGE;
use crate::equipment::{EquipEvent, Equipment, Kind as EquipKind, Placed, HACK_COS, HACK_DRONE_TIME, HACK_RANGE, HACK_TIME};
use crate::projectiles::{Outcome, Projectile};
use crate::units::inches;
use crate::building::{self, Hammer, Site};
use crate::weapons::{Arsenal, Class, Perk, Rng, WeaponAction, MELEE_FX_WEAPON, MELEE_RANGE};

pub const MAX_PLAYERS: usize = 16;

/// How long the field computer takes to come up before control returns to the RTS view: BO2's
/// scorestreak tablet pullout, then a moment on its screen.
pub const FIELD_COMPUTER_RAISE: f32 = crate::viewmodel::TABLET_PULLOUT_TIME + 0.35;
/// Taking control, the weapon is ready after the camera flies in, the tablet goes down and the
/// weapon comes up.
pub const TAKE_CONTROL_READY: f32 = crate::client::ENTER_TIME + crate::viewmodel::TABLET_PUTAWAY_TIME;
/// BO2 health regeneration: wait this long after the last hit, then regenerate.
pub const REGEN_DELAY: f32 = 5.0;
pub const REGEN_FULL_TIME: f32 = 3.0;
/// Hard cap on queued client events so a headless or paused client cannot grow it forever.
const MAX_EVENTS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Fps,
    /// Field computer coming up; `t` counts seconds since it was opened.
    FieldComputer { t: f32 },
    /// The General is back under normal RTS control (player is using the field computer view).
    Rts,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventKind {
    EnteredFps { general: u32 },
    FieldComputerOpened,
    ReturnedToRts { general: u32 },
    Shot { origin: Vec3, end: Vec3 },
    HitMarker { headshot: bool, kill: bool },
    Recoil { pitch_deg: f32, yaw_deg: f32 },
    Damaged { amount: f32 },
    ReloadStarted,
    Swapped,
    GeneralDied { general: u32 },
    /// Melee swing (hit or miss).
    Melee,
    /// Offhand pulled back (pin out); then thrown.
    OffhandReady { lethal: bool },
    Thrown { lethal: bool },
    /// A launcher fired.
    Launched,
    /// A grenade, rocket or other projectile went off.
    Exploded { at: Vec3 },
    /// Equipment set down (planted, or a thrown piece came to rest).
    Planted { kind: EquipKind },
    /// The player's C4 went off.
    Detonated,
    /// A Trophy System of this player's shot something down.
    Intercepted { at: Vec3 },
    /// This player broke an enemy's equipment.
    DestroyedEquipment,
    /// The Black Hat finished a hack.
    Hacked,
    /// Black Hat progress on the current target (0..1), each frame while hacking.
    Hacking { progress: f32 },
    AltToggled,
    /// The hammer came up or went away.
    Hammer { up: bool },
    /// A structure's site laid out with the hammer (its template).
    BuildPlaced { template: u32 },
    /// The hammer could not place it (not enough money, locked, or no room there), or someone
    /// else is repairing it (`building::status`).
    BuildRefused { status: i32 },
    /// Working at a site (`building::work`), each frame while the use key is held (percent 0..100).
    Working { kind: i32, percent: f32 },
    WorkDone { kind: i32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub player: i32,
    pub kind: EventKind,
}

#[derive(Clone, Debug)]
pub struct PlayerSim {
    pub general: u32,
    pub mode: Mode,
    pub body: Body,
    pub yaw: f32,
    pub pitch: f32,
    pub arsenal: Arsenal,
    pub rng: Rng,
    pub input: FpsInput,
    pressed_accum: u32,
    pub health: f32,
    pub max_health: f32,
    pub regen_wait: f32,
    /// Black Hat: what it is aimed at (an object id) and how far along the hack is (seconds).
    pub hack: Option<(u32, f32)>,
    /// Under RTS control: the Zero Hour weapon slot to lock to (the weapon that was in hand when
    /// first person ended, or one with ammo when the current one ran dry), or -1.
    pub rts_lock: i32,
    /// Under RTS control: whether the third Zero Hour slot throws the tactical (else the lethal),
    /// and the gun slot to go back to after a throw.
    pub rts_tactical: bool,
    pub rts_gun: i32,
    /// The hammer, when it is out.
    pub hammer: Option<Hammer>,
    /// The structure under construction in reach (the use key builds it instead of opening the
    /// field computer), and whether he is building it now.
    pub site: Option<Site>,
    pub building: bool,
}

impl PlayerSim {
    pub fn is_first_person(&self) -> bool {
        matches!(self.mode, Mode::Fps | Mode::FieldComputer { .. })
    }

    #[cfg(test)]
    pub fn spawn_for_tests() -> PlayerSim {
        PlayerSim {
            general: 1,
            mode: Mode::Fps,
            body: Body::new(Vec3::ZERO, &MoveTuning::default()),
            yaw: 0.0,
            pitch: 0.0,
            arsenal: Arsenal::new(),
            rng: Rng::new(1),
            input: FpsInput::default(),
            pressed_accum: 0,
            health: 500.0,
            max_health: 500.0,
            regen_wait: 0.0,
            hack: None,
            rts_lock: -1,
            rts_tactical: false,
            rts_gun: 0,
            hammer: None,
            site: None,
            building: false,
        }
    }
}

pub struct Sim {
    pub players: Vec<Option<PlayerSim>>,
    /// Grenades and rockets in flight, with the player who threw them.
    pub projectiles: Vec<(usize, Projectile)>,
    /// Equipment that is down.
    pub equipment: Equipment,
    /// Smoke screens: where, how wide, seconds left.
    pub smokes: Vec<(Vec3, f32, f32)>,
    pub tuning: MoveTuning,
    pub frame: u32,
    pub events: Vec<Event>,
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}

impl Sim {
    pub fn new() -> Self {
        Self {
            players: vec![None; MAX_PLAYERS],
            projectiles: Vec::new(),
            equipment: Equipment::default(),
            smokes: Vec::new(),
            tuning: MoveTuning::default(),
            frame: 0,
            events: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        *self = Sim::new();
    }

    fn emit(&mut self, player: usize, kind: EventKind) {
        if self.events.len() >= MAX_EVENTS {
            self.events.remove(0);
        }
        self.events.push(Event { player: player as i32, kind });
    }

    pub fn player(&self, player: i32) -> Option<&PlayerSim> {
        self.players.get(usize::try_from(player).ok()?)?.as_ref()
    }

    /// Ids of Generals currently driven by first-person input (their AI and physics are suspended).
    pub fn controlled_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.players.iter().flatten().filter(|p| p.is_first_person()).map(|p| p.general)
    }

    /// `MSG_ZHBO_TAKE_CONTROL`: the player clicked "Take Control" on their General.
    pub fn take_control(&mut self, host: &dyn LogicHost, player: i32, general: u32) {
        let Ok(idx) = usize::try_from(player) else { return };
        if idx >= MAX_PLAYERS {
            return;
        }
        let Some(info) = host.object(general, general) else { return };
        if info.has(objflags::DEAD) || info.owner_player != player {
            return;
        }
        // One General per player in first person; release a previous one.
        if let Some(prev) = self.players[idx].as_ref() {
            if prev.general != general && prev.is_first_person() {
                host.set_controlled(prev.general, false);
            }
        }
        let tuning = self.tuning;
        let frame = self.frame;
        let mut state = match self.players[idx].take() {
            Some(mut p) if p.general == general => {
                p.body = Body::new(info.pos, &tuning);
                p.mode = Mode::Fps;
                p.yaw = info.yaw;
                p.pitch = 0.0;
                p.input = FpsInput { yaw: info.yaw, ..Default::default() };
                p
            }
            _ => {
                // A new General comes in at the player's Tactical Insertion, if one is down.
                let (pos, yaw) = match self.equipment.take_insertion(host, idx) {
                    Some((pos, yaw)) => {
                        host.set_pose(general, pos, yaw, false);
                        (pos, yaw)
                    }
                    None => (info.pos, info.yaw),
                };
                PlayerSim {
                    general,
                    mode: Mode::Fps,
                    body: Body::new(pos, &tuning),
                    yaw,
                    pitch: 0.0,
                    arsenal: Arsenal::new(),
                    rng: Rng::new(general.wrapping_mul(2_654_435_761) ^ frame),
                    input: FpsInput { yaw, ..Default::default() },
                    pressed_accum: 0,
                    health: info.health,
                    max_health: info.max_health,
                    regen_wait: 0.0,
                    hack: None,
                    rts_lock: -1,
                    rts_tactical: false,
                    rts_gun: 0,
            hammer: None,
            site: None,
            building: false,
                }
            }
        };
        // The weapon can't fire or aim until the tablet is down and it is up again.
        // BO2's first raise, once the tablet is down.
        let first = state.arsenal.current().def.first_raise_time;
        state.arsenal.raise_left = TAKE_CONTROL_READY + first;
        state.arsenal.raise_total = first;
        state.arsenal.raise_kind = crate::weapons::RaiseKind::First;
        state.arsenal.drop_left = 0.0;
        state.arsenal.offhand = None;
        state.arsenal.ads = 0.0;
        self.players[idx] = Some(state);
        host.set_controlled(general, true);
        self.emit(idx, EventKind::EnteredFps { general });
    }

    /// `MSG_ZHBO_FPS_INPUT`: latest input for a player's General.
    pub fn input(&mut self, player: i32, general: u32, input: FpsInput) {
        let Ok(idx) = usize::try_from(player) else { return };
        if let Some(Some(p)) = self.players.get_mut(idx) {
            if p.general == general {
                let input = input.sanitized();
                p.pressed_accum |= input.pressed;
                p.input = input;
            }
        }
    }

    /// Once per logic frame, after the frame's messages were dispatched.
    pub fn step(&mut self, host: &dyn LogicHost, frame: u32) {
        self.frame = frame;
        for idx in 0..self.players.len() {
            let Some(mut p) = self.players[idx].take() else { continue };
            let keep = self.step_player(host, idx, &mut p);
            if keep {
                self.players[idx] = Some(p);
            }
        }
        self.step_projectiles(host);
        self.step_equipment(host);
        for s in &mut self.smokes {
            host.smoke(s.0, s.1);
            s.2 -= LOGIC_DT;
        }
        self.smokes.retain(|s| s.2 > 0.0);
    }

    fn step_projectiles(&mut self, host: &dyn LogicHost) {
        let mut done = Vec::new();
        for (i, (owner, proj)) in self.projectiles.iter_mut().enumerate() {
            if let Some(outcome) = proj.step(host) {
                done.push((i, *owner, outcome));
            }
        }
        for (i, owner, outcome) in done.iter().rev() {
            let (_, proj) = self.projectiles.remove(*i);
            match *outcome {
                Outcome::Exploded { at, hit_enemy, kill } => {
                    self.emit(*owner, EventKind::Exploded { at });
                    if proj.def.effect == crate::weapons::Effect::Smoke {
                        let radius = proj.def.explosion_radius.max(crate::projectiles::SMOKE_RADIUS);
                        self.smokes.push((at, radius, crate::projectiles::SMOKE_TIME));
                    }
                    let mut ev = Vec::new();
                    self.equipment.caught_in_blast(host, *owner, proj.owner, at, proj.def.explosion_radius, &mut ev);
                    self.equipment_events(ev);
                    if hit_enemy {
                        self.emit(*owner, EventKind::HitMarker { headshot: false, kill });
                    }
                }
                Outcome::Hit { kill, .. } => {
                    if kill {
                        self.emit(*owner, EventKind::HitMarker { headshot: false, kill });
                        self.scavenge(*owner);
                    }
                }
                Outcome::Landed { at, stuck_to } => {
                    let yaw = proj.vel.y.atan2(proj.vel.x);
                    let p = Placed::new(proj.def, *owner, proj.owner, proj.object, at, yaw, stuck_to);
                    self.emit(*owner, EventKind::Planted { kind: p.kind });
                    self.equipment.add(p);
                }
            }
        }
    }

    fn step_equipment(&mut self, host: &dyn LogicHost) {
        let players = &self.players;
        let engineer = |id: u32| players.iter().flatten().any(|p| p.general == id && p.arsenal.has(Perk::Engineer));
        let mut ev = Vec::new();
        self.equipment.step(host, &mut self.projectiles, &engineer, &mut ev);
        self.equipment_events(ev);
    }

    fn equipment_events(&mut self, events: Vec<EquipEvent>) {
        for e in events {
            match e {
                EquipEvent::Exploded { owner, at, hit_enemy, kill } => {
                    self.emit(owner, EventKind::Exploded { at });
                    if hit_enemy {
                        self.emit(owner, EventKind::HitMarker { headshot: false, kill });
                    }
                }
                EquipEvent::Destroyed { by: Some(by), .. } => {
                    self.emit(by, EventKind::DestroyedEquipment);
                    self.emit(by, EventKind::HitMarker { headshot: false, kill: false });
                }
                EquipEvent::Destroyed { by: None, .. } => {}
                EquipEvent::Intercepted { owner, at } => self.emit(owner, EventKind::Intercepted { at }),
            }
        }
    }

    /// Scavenger: a kill with a non-explosive weapon refills ammo.
    fn scavenge(&mut self, idx: usize) {
        if let Some(Some(p)) = self.players.get_mut(idx) {
            if p.arsenal.has(Perk::Scavenger) {
                p.arsenal.scavenge();
            }
        }
    }

    fn step_player(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim) -> bool {
        let Some(info) = host.object(p.general, p.general) else {
            self.emit(idx, EventKind::GeneralDied { general: p.general });
            return false;
        };
        if info.has(objflags::DEAD) {
            host.set_controlled(p.general, false);
            self.equipment.clear_owner(host, idx);
            self.emit(idx, EventKind::GeneralDied { general: p.general });
            return false;
        }
        if !p.is_first_person() {
            return true;
        }

        // Something else moved the General (teleport, crate, script): follow it.
        if (info.pos - p.body.pos).length_xy() > 8.0 {
            p.body.pos = info.pos;
            p.body.vel = Vec3::ZERO;
        }

        self.track_health(host, idx, p, &info);

        let mut input = p.input;
        input.pressed = p.pressed_accum;
        p.pressed_accum = 0;
        p.yaw = input.yaw;
        p.pitch = input.pitch;

        let world = HostWorld { host, viewer: p.general };
        match p.mode {
            Mode::Fps => {
                // The use key works (builds or repairs) at a structure in reach, held, instead of
                // opening the field computer.
                p.site = host.work_site(p.general, p.body.pos, building::REACH);
                p.building = p.site.is_some() && (input.held | input.pressed) & buttons::FIELD_COMPUTER != 0;
                if p.site.is_none() && input.pressed & buttons::FIELD_COMPUTER != 0 {
                    p.mode = Mode::FieldComputer { t: 0.0 };
                    p.arsenal.ads = 0.0;
                    self.emit(idx, EventKind::FieldComputerOpened);
                } else {
                    let mods = MoveModifiers {
                        speed_scale: p.arsenal.move_scale(),
                        block_sprint: p.arsenal.ads > 0.0 || p.arsenal.offhand.is_some() || p.arsenal.melee_left > 0.0 || p.building,
                        sprint_time_scale: p.arsenal.sprint_scale(),
                    };
                    movement::step(&mut p.body, &input, mods, &self.tuning, &world, LOGIC_DT);
                    let mut actions = Vec::new();
                    let stance = match p.body.stance {
                        Stance::Stand => 0,
                        Stance::Crouch => 1,
                        Stance::Prone => 2,
                    };
                    let speed = p.body.horizontal_speed();
                    // The hammer, or building, takes the hands: the gun gets no input meanwhile.
                    let busy = self.step_building(host, idx, p, &input);
                    let (held, pressed) = if busy { (0, 0) } else { (input.held, input.pressed) };
                    p.arsenal.step(held, pressed, p.body.sprinting, stance, speed, &mut p.rng, &mut actions);
                    for a in actions {
                        self.apply_weapon_action(host, idx, p, a);
                    }
                    self.step_hack(host, idx, p);
                }
            }
            Mode::FieldComputer { t } => {
                // Stand still while the laptop comes up; gravity still applies.
                let idle = FpsInput { yaw: input.yaw, pitch: input.pitch, ..Default::default() };
                movement::step(&mut p.body, &idle, MoveModifiers::default(), &self.tuning, &world, LOGIC_DT);
                let t = t + LOGIC_DT;
                if t >= FIELD_COMPUTER_RAISE {
                    p.mode = Mode::Rts;
                    // The RTS weapon is the one that was in hand.
                    p.rts_lock = p.arsenal.active as i32;
                    host.set_pose(p.general, p.body.pos, p.yaw, false);
                    host.set_controlled(p.general, false);
                    self.emit(idx, EventKind::ReturnedToRts { general: p.general });
                    return true;
                }
                p.mode = Mode::FieldComputer { t };
            }
            Mode::Rts => return true,
        }

        let moving = p.body.horizontal_speed() > 1.0;
        host.set_pose(p.general, p.body.pos, p.yaw, moving);
        true
    }

    /// The hammer (toggle; choose with ADS, turn with the wheel, place with fire) and working at a
    /// site (hold the use key); whether either has his hands.
    fn step_building(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, input: &FpsInput) -> bool {
        // In hand at the start of the frame: its input (the click that placed, say) is not the gun's.
        let had_hammer = p.hammer.is_some();
        if input.pressed & buttons::HAMMER != 0 {
            p.hammer = match p.hammer {
                Some(_) => None,
                None => Some(Hammer::default()),
            };
            self.emit(idx, EventKind::Hammer { up: p.hammer.is_some() });
        } else if p.hammer.is_some() && input.pressed & (buttons::SWAP_WEAPON | buttons::LETHAL | buttons::TACTICAL) != 0 {
            // Reaching for a weapon puts the hammer away.
            p.hammer = None;
            self.emit(idx, EventKind::Hammer { up: false });
        }
        if let Some(mut h) = p.hammer {
            h.raise = (h.raise + LOGIC_DT).min(building::HAMMER_RAISE);
            let options = host.build_options(p.general);
            let n = options.len();
            let mut placed = false;
            if n > 0 {
                if !h.picked {
                    h.picked = true;
                    h.choice = options.iter().position(|o| o.status == building::status::OK).unwrap_or(0);
                }
                if input.pressed & buttons::ADS != 0 {
                    h.choice = (h.choice + 1) % n;
                }
                if input.pressed & buttons::ROTATE_LEFT != 0 {
                    h.turn = crate::math::wrap_angle(h.turn + building::ROTATE_STEP);
                }
                if input.pressed & buttons::ROTATE_RIGHT != 0 {
                    h.turn = crate::math::wrap_angle(h.turn - building::ROTATE_STEP);
                }
                h.choice = h.choice.min(n - 1);
                (h.at, h.angle) = placement(host, p, options[h.choice].radius, h.turn);
                if input.pressed & buttons::FIRE != 0 && h.raise >= building::HAMMER_RAISE && !p.building {
                    let o = options[h.choice];
                    let (pos, angle) = (h.at, h.angle);
                    let status = if o.status != building::status::OK {
                        o.status
                    } else if host.can_place(p.general, o.template, pos, angle) && host.place(p.general, o.template, pos, angle) != 0 {
                        -1
                    } else {
                        building::status::BLOCKED
                    };
                    if status == -1 {
                        placed = true;
                        host.log(&format!("player {idx} placed {} turned {:.0} degrees", o.name(), h.turn.to_degrees()));
                        self.emit(idx, EventKind::BuildPlaced { template: o.template });
                    } else {
                        host.log(&format!("player {idx} can't build {} (status {status}, ${})", o.name(), o.cost));
                        self.emit(idx, EventKind::BuildRefused { status });
                    }
                }
            }
            if placed {
                // One structure per hammer: it goes away once placed (bring it up again for another).
                p.hammer = None;
                self.emit(idx, EventKind::Hammer { up: false });
            } else {
                p.hammer = Some(h);
            }
        }
        if p.building {
            if let Some(site) = p.site {
                let percent = host.work(p.general, site.id, building::FPS_BUILD_RATE);
                if percent < 0.0 {
                    p.building = false;
                    if input.pressed & buttons::FIELD_COMPUTER != 0 {
                        self.emit(idx, EventKind::BuildRefused { status: building::status::BUSY });
                    }
                } else if percent >= 100.0 {
                    p.building = false;
                    p.site = None;
                    self.emit(idx, EventKind::WorkDone { kind: site.kind });
                } else {
                    p.site = Some(Site { percent, ..site });
                    self.emit(idx, EventKind::Working { kind: site.kind, percent });
                }
            }
        }
        had_hammer || p.hammer.is_some() || p.building
    }

    /// Black Hat: hold it on enemy equipment or a Zero Hour drone until the hack completes.
    fn step_hack(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim) {
        if p.arsenal.hacking().is_none() {
            p.hack = None;
            return;
        }
        let eye = p.body.eye();
        let dir = dir_from_angles(p.yaw, p.pitch);
        // Equipment first, then the nearest drone in the aim cone.
        let target = match self.equipment.aimed_at(host, p.general, eye, dir) {
            Some(i) => Some((self.equipment.placed[i].object, HACK_TIME)),
            None => {
                let mut objs = Vec::new();
                host.query(eye, HACK_RANGE, p.general, &mut objs);
                objs.into_iter()
                    .filter(|o| o.has(objflags::DRONE) && o.has(objflags::ENEMY) && !o.has(objflags::DEAD))
                    .filter_map(|o| {
                        let to = o.pos + Vec3::new(0.0, 0.0, o.height * 0.5) - eye;
                        let d = to.length();
                        (d > 1e-3 && to.dot(dir) / d >= HACK_COS).then_some((o.id, d))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
                    .map(|(id, _)| (id, HACK_DRONE_TIME))
            }
        };
        let Some((target, needed)) = target else {
            p.hack = None;
            return;
        };
        let progress = match p.hack {
            Some((t, done)) if t == target => done + LOGIC_DT,
            _ => LOGIC_DT,
        };
        if progress >= needed {
            if let Some(i) = self.equipment.placed.iter().position(|e| e.object == target) {
                self.equipment.hacked(host, i, idx, p.general);
            } else {
                host.capture(target, p.general);
            }
            p.arsenal.hack_done();
            p.hack = None;
            self.emit(idx, EventKind::Hacked);
        } else {
            p.hack = Some((target, progress));
            self.emit(idx, EventKind::Hacking { progress: progress / needed });
        }
    }

    /// The General under RTS control (Zero Hour's AI fights with it): count the shots its Zero Hour
    /// weapons fired (`shots`, by slot: primary, secondary, lethal) against the arsenal, follow the
    /// weapon the player selected (`current_slot`), and say which slot to lock to, if any. The
    /// arsenal is the same one first person uses, so ammo and the weapon in hand carry over both
    /// ways. Generals never taken control of get one here (in RTS mode).
    pub fn rts_update(&mut self, host: &dyn LogicHost, player: i32, general: u32, current_slot: i32, shots: [u32; 3], target: Option<Vec3>) -> RtsWeapons {
        let Ok(idx) = usize::try_from(player) else { return RtsWeapons::default() };
        if idx >= MAX_PLAYERS {
            return RtsWeapons::default();
        }
        if self.players[idx].as_ref().is_none_or(|p| p.general != general && !p.is_first_person()) {
            let Some(info) = host.object(general, general) else { return RtsWeapons::default() };
            let frame = self.frame;
            self.players[idx] = Some(PlayerSim {
                general,
                mode: Mode::Rts,
                body: Body::new(info.pos, &self.tuning),
                yaw: info.yaw,
                pitch: 0.0,
                arsenal: Arsenal::new(),
                rng: Rng::new(general.wrapping_mul(2_654_435_761) ^ frame),
                input: FpsInput { yaw: info.yaw, ..Default::default() },
                pressed_accum: 0,
                health: info.health,
                max_health: info.max_health,
                regen_wait: 0.0,
                hack: None,
                rts_lock: -1,
                rts_tactical: false,
                rts_gun: 0,
            hammer: None,
            site: None,
            building: false,
            });
        }
        let Some(p) = self.players[idx].as_mut().filter(|p| p.general == general) else { return RtsWeapons::default() };
        if p.mode != Mode::Rts {
            return rts_weapons(&p.arsenal, -1, p.rts_tactical);
        }
        let a = &mut p.arsenal;
        for (slot, &n) in shots.iter().enumerate().take(2) {
            for _ in 0..n {
                a.rts_shot(slot);
            }
        }
        // The third slot only stands in for the throw: the grenade itself is BO2's, thrown here as
        // first person throws it (its fuse, bounces and blast), at what Zero Hour aimed at.
        let mut throws = Vec::new();
        for _ in 0..shots[2] {
            let off = if p.rts_tactical { &mut a.tactical } else { &mut a.lethal };
            if off.count > 0 {
                off.count -= 1;
                throws.push(off.def);
            }
        }
        if (0..2).contains(&current_slot) {
            p.rts_gun = current_slot;
        }
        if (0..a.weapons.len() as i32).contains(&current_slot) {
            a.active = current_slot as usize;
        }
        let mut lock = std::mem::replace(&mut p.rts_lock, -1);
        if !throws.is_empty() {
            lock = p.rts_gun; // back to the gun after the throw
        }
        let a = &p.arsenal;
        let current = if lock >= 0 { lock } else { current_slot };
        let offhand_left = if p.rts_tactical { a.tactical.count } else { a.lethal.count };
        let empty = |s: i32| if s == 2 { offhand_left == 0 } else { a.rts_ammo(s as usize).0 + a.rts_ammo(s as usize).1 == 0 };
        if (0..3).contains(&current) && empty(current) {
            // Out of ammo: the first gun that has some (the grenades only by choice).
            lock = (0..a.weapons.len()).find(|&s| a.rts_ammo(s).0 + a.rts_ammo(s).1 > 0).map_or(-1, |s| s as i32);
        }
        let out = rts_weapons(a, lock, p.rts_tactical);
        if let Some(from) = host.object(general, general).map(|o| o.pos + Vec3::new(0.0, 0.0, p.body.eye_height * 0.9)) {
            let yaw = p.yaw;
            for def in throws {
                let to = target.unwrap_or(from);
                let speed = if def.projectile_speed > 0.0 { def.projectile_speed } else { crate::units::inches(800.0) };
                let vel = throw_velocity(from, to, speed);
                let mut proj = Projectile::launch(def, general, from, vel.normalized_or_zero(), def.fuse_time, host, yaw);
                proj.vel = vel;
                self.projectiles.push((idx, proj));
            }
        }
        out
    }

    /// Under RTS control: make the third slot throw the tactical (`true`) or the lethal.
    pub fn rts_offhand(&mut self, player: i32, general: u32, tactical: bool) {
        let Some(p) = usize::try_from(player).ok().and_then(|i| self.players.get_mut(i)).and_then(|p| p.as_mut()) else { return };
        if p.general == general {
            p.rts_tactical = tactical;
        }
    }

    /// The ammo a General's weapons have, for the control bar (the loadout's when the General has
    /// no arsenal yet).
    pub fn rts_ammo(&self, general: u32) -> RtsWeapons {
        match self.players.iter().flatten().find(|p| p.general == general) {
            Some(p) => rts_weapons(&p.arsenal, -1, p.rts_tactical),
            None => rts_weapons(&Arsenal::new(), -1, false),
        }
    }

    /// How each controlled General looks in third person: its General, the viewmodel index of the
    /// weapon in hand (its world model is shown), the tablet (0 none, 1 coming up for the field
    /// computer, 2 going down as first person starts), and whether the sidearm is in hand.
    pub fn world_poses(&self) -> Vec<(u32, i32, i32, bool)> {
        self.players
            .iter()
            .flatten()
            .map(|p| {
                let a = &p.arsenal;
                let tablet = match p.mode {
                    Mode::FieldComputer { .. } => 1,
                    Mode::Fps if a.raise_left > a.raise_total + 1e-4 => 2,
                    _ => 0,
                };
                (p.general, a.current().def.viewmodel.max(0), tablet, a.active == 1)
            })
            .collect()
    }

    /// Whether `player` sees enemies at `pos` on the minimap through the shroud: one of their
    /// Sensor Grenades.
    pub fn reveals(&self, player: i32, pos: Vec3) -> bool {
        let Ok(idx) = usize::try_from(player) else { return false };
        self.equipment.sensed(idx, pos)
    }

    fn track_health(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, info: &ObjInfo) {
        p.max_health = info.max_health;
        if info.health < p.health - 1e-3 {
            p.regen_wait = REGEN_DELAY;
            self.emit(idx, EventKind::Damaged { amount: p.health - info.health });
        } else {
            p.regen_wait = (p.regen_wait - LOGIC_DT).max(0.0);
            if p.regen_wait <= 0.0 && info.health < info.max_health {
                let amount = (info.max_health / REGEN_FULL_TIME * LOGIC_DT).min(info.max_health - info.health);
                host.heal(p.general, amount);
                p.health = info.health + amount;
                return;
            }
        }
        p.health = info.health;
    }

    fn apply_weapon_action(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, action: WeaponAction) {
        match action {
            WeaponAction::Fire { spread_deg, jitter, recoil, pellet } => {
                let def = p.arsenal.current().def;
                let eye = p.body.eye();
                // Random point in the spread cone (polar mapping of two uniform samples).
                let angle = jitter.0 * std::f32::consts::PI;
                let radius = spread_deg.to_radians() * jitter.1.abs().sqrt();
                let dir = dir_from_angles(p.yaw + radius * angle.cos(), p.pitch + radius * angle.sin());

                let mut targets = gather_targets(host, eye, def.max_range, p.general);
                let ground = |x: f32, y: f32| host.ground_height(x, y);
                let hit = hitscan::trace(eye, dir, def.max_range, &targets, p.general, &ground);
                // One tracer and sound per shot, not per pellet.
                let fx = if pellet == 0 { Some(def.fx_weapon.as_str()).filter(|f| !f.is_empty()) } else { None };
                // Enemy equipment in the way takes the shot.
                let reach = match hit {
                    HitResult::Object { distance, .. } => distance,
                    other => (other.point() - eye).length(),
                };
                if let Some((i, t)) = self.equipment.shot(host, p.general, eye, dir, reach) {
                    if let Some(fx) = fx {
                        host.fire_fx(p.general, fx, 0, eye + dir * t);
                    }
                    let e = self.equipment.destroy(host, i, Some(idx));
                    self.equipment_events(vec![e]);
                    if pellet == 0 {
                        self.emit(idx, EventKind::Shot { origin: eye, end: eye + dir * t });
                        self.emit(idx, EventKind::Recoil { pitch_deg: recoil.0, yaw_deg: recoil.1 });
                    }
                    return;
                }
                match hit {
                    HitResult::Object { id, point, distance, headshot } => {
                        if let Some(fx) = fx {
                            host.fire_fx(p.general, fx, id, point);
                        }
                        let mult = if headshot { def.head_multiplier } else { 1.0 };
                        let victim = host.object(id, p.general);
                        let infantry = victim.is_some_and(|v| v.has(objflags::INFANTRY));
                        let fmj = if def.fmj && !infantry { FMJ_VEHICLE_DAMAGE } else { 1.0 };
                        let amount = def.damage_at(distance) * mult * fmj * BO2_TO_ZH_DAMAGE;
                        let mut kill = host.damage(p.general, id, amount, damage_kind::SMALL_ARMS);
                        // FMJ: on through the first soldier into whoever is behind.
                        if def.fmj && infantry {
                            targets.retain(|t| t.id != id);
                            if let HitResult::Object { id: next, distance, headshot, .. } = hitscan::trace(point + dir * 0.1, dir, def.max_range - distance, &targets, p.general, &ground) {
                                let mult = if headshot { def.head_multiplier } else { 1.0 };
                                kill |= host.damage(p.general, next, def.damage_at(distance) * mult * PENETRATION_DAMAGE * BO2_TO_ZH_DAMAGE, damage_kind::SMALL_ARMS);
                            }
                        }
                        self.emit(idx, EventKind::HitMarker { headshot, kill });
                        if kill {
                            if p.arsenal.has(Perk::Scavenger) {
                                p.arsenal.scavenge();
                            }
                        }
                    }
                    other => {
                        if let Some(fx) = fx {
                            host.fire_fx(p.general, fx, 0, other.point());
                        }
                    }
                }
                if pellet == 0 {
                    self.emit(idx, EventKind::Shot { origin: eye, end: hit.point() });
                    self.emit(idx, EventKind::Recoil { pitch_deg: recoil.0, yaw_deg: recoil.1 });
                }
            }
            WeaponAction::Launch { def, pellet } => {
                let eye = p.body.eye();
                // The tri-bolt's bolts fan out a little.
                let spread = (pellet as f32 - (def.pellets.max(1) - 1) as f32 * 0.5) * 0.04;
                let dir = dir_from_angles(p.yaw + spread, p.pitch);
                if !def.fx_weapon.is_empty() {
                    host.fire_fx(p.general, &def.fx_weapon, 0, eye + dir * 10.0);
                }
                let proj = Projectile::launch(def, p.general, eye + dir * 2.0, dir, def.fuse_time, host, p.yaw);
                self.projectiles.push((idx, proj));
                if pellet == 0 {
                    self.emit(idx, EventKind::Launched);
                }
            }
            WeaponAction::Throw { def, fuse_left } => {
                let eye = p.body.eye();
                // BO2 throws a little above the crosshair.
                let dir = dir_from_angles(p.yaw, (p.pitch + 0.12).min(1.4));
                let mut proj = Projectile::launch(def, p.general, eye + dir * 1.5, dir, fuse_left, host, p.yaw);
                proj.vel = proj.vel + p.body.vel * 0.5;
                proj.flak_jacket = p.arsenal.has(Perk::FlakJacket);
                proj.tactical_mask = p.arsenal.has(Perk::TacticalMask);
                self.projectiles.push((idx, proj));
                self.emit(idx, EventKind::Thrown { lethal: def.class == Class::Lethal });
            }
            WeaponAction::CookedOff { def } => {
                let mut proj = Projectile::launch(def, p.general, p.body.eye(), Vec3::ZERO, 0.0, host, p.yaw);
                proj.vel = Vec3::ZERO;
                proj.fuse = 0.0;
                proj.flak_jacket = p.arsenal.has(Perk::FlakJacket);
                self.projectiles.push((idx, proj));
            }
            WeaponAction::OffhandReady { lethal } => self.emit(idx, EventKind::OffhandReady { lethal }),
            WeaponAction::Melee { damage } => {
                self.emit(idx, EventKind::Melee);
                let eye = p.body.eye();
                let (fwd, _) = basis_xy(p.yaw);
                let mut near = Vec::new();
                host.query(eye, MELEE_RANGE + 12.0, p.general, &mut near);
                let victim = near
                    .iter()
                    .filter(|o| o.id != p.general && !o.has(objflags::DEAD) && !o.has(objflags::ALLY))
                    .filter(|o| !o.has(objflags::NO_COLLIDE) && !o.has(objflags::STRUCTURE))
                    .filter(|o| {
                        let to = o.pos - p.body.pos;
                        let d = to.length_xy();
                        d <= MELEE_RANGE + reach_radius(o) && (d < 1e-3 || (to.x * fwd.x + to.y * fwd.y) / d > 0.6)
                    })
                    .min_by(|a, b| {
                        let da = (a.pos - p.body.pos).length_xy();
                        let db = (b.pos - p.body.pos).length_xy();
                        da.total_cmp(&db).then(a.id.cmp(&b.id))
                    });
                if let Some(v) = victim {
                    host.fire_fx(p.general, MELEE_FX_WEAPON, v.id, v.pos);
                    let kill = host.damage(p.general, v.id, damage * BO2_TO_ZH_DAMAGE, damage_kind::MELEE);
                    self.emit(idx, EventKind::HitMarker { headshot: false, kill });
                    if kill {
                        if p.arsenal.has(Perk::Scavenger) {
                            p.arsenal.scavenge();
                        }
                    }
                }
            }
            WeaponAction::Plant { def } => {
                let (fwd, _) = basis_xy(p.yaw);
                let at = p.body.pos + fwd * PLANT_DISTANCE;
                let at = Vec3::new(at.x, at.y, host.ground_height(at.x, at.y));
                let object = if def.projectile_object.is_empty() { 0 } else { host.spawn(&def.projectile_object, p.general, at, p.yaw) };
                let placed = Placed::new(def, idx, p.general, object, at, p.yaw, None);
                self.emit(idx, EventKind::Planted { kind: placed.kind });
                self.equipment.add(placed);
            }
            WeaponAction::Detonate => {
                let mut ev = Vec::new();
                self.equipment.detonate(host, idx, &mut ev);
                if !ev.is_empty() {
                    self.emit(idx, EventKind::Detonated);
                }
                // Points for what the C4 kills (the owner's own state is out of the table now).
                for e in &ev {
                    if let EquipEvent::Exploded { at, hit_enemy, kill, .. } = *e {
                        self.emit(idx, EventKind::Exploded { at });
                        if hit_enemy {
                            self.emit(idx, EventKind::HitMarker { headshot: false, kill });
                        }
                        if kill {
                        }
                    }
                }
            }
            WeaponAction::AltToggled => self.emit(idx, EventKind::AltToggled),
            WeaponAction::ReloadStarted => self.emit(idx, EventKind::ReloadStarted),
            WeaponAction::Swapped => self.emit(idx, EventKind::Swapped),
            WeaponAction::DryFire | WeaponAction::ReloadFinished => {}
        }
    }

}

/// A General's Zero Hour weapon slots under RTS control (primary, secondary, lethal): rounds in
/// the magazine (grenades in hand for the lethal), spare rounds, and the slot to lock to (-1 none).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RtsWeapons {
    pub ammo: [i32; 3],
    pub reserve: [i32; 3],
    pub lock: i32,
    /// Tactical grenades left, and whether the third slot throws them (else the lethal).
    pub tactical: i32,
    pub tactical_selected: i32,
}

fn rts_weapons(a: &Arsenal, lock: i32, tactical_selected: bool) -> RtsWeapons {
    let mut out = RtsWeapons { lock, tactical: a.tactical.count as i32, tactical_selected: tactical_selected as i32, ..Default::default() };
    for s in 0..3 {
        let (ammo, reserve) = a.rts_ammo(s);
        out.ammo[s] = ammo as i32;
        out.reserve[s] = reserve as i32;
    }
    out
}

/// Launch velocity for a grenade of throw speed `speed` from `from` to land at `to` (the low arc;
/// out of reach, the farthest throw, at 45 degrees).
pub fn throw_velocity(from: Vec3, to: Vec3, speed: f32) -> Vec3 {
    let g = crate::projectiles::GRAVITY;
    let flat = Vec3::new(to.x - from.x, to.y - from.y, 0.0);
    let range = flat.length_xy().max(1.0);
    let rise = to.z - from.z;
    let v2 = speed * speed;
    let disc = v2 * v2 - g * (g * range * range + 2.0 * rise * v2);
    let tan = if disc >= 0.0 { (v2 - disc.sqrt()) / (g * range) } else { 1.0 };
    let angle = tan.atan();
    let along = flat.normalized_or_zero();
    along * (speed * angle.cos()) + Vec3::new(0.0, 0.0, speed * angle.sin())
}

/// Planted equipment goes down this far in front of the General.
pub const PLANT_DISTANCE: f32 = inches(30.0);
/// FMJ: damage to the second soldier a round passes through.
pub const PENETRATION_DAMAGE: f32 = 0.75;

fn reach_radius(o: &ObjInfo) -> f32 {
    if o.has(objflags::INFANTRY) {
        INFANTRY_HIT_RADIUS
    } else {
        o.major_radius
    }
}

/// Convert nearby objects into hitscan targets. Friendlies are skipped so bullets pass through them
/// (BO2 has no friendly fire in core modes); dead and non-colliding objects are skipped.
pub fn gather_targets(host: &dyn LogicHost, eye: Vec3, range: f32, shooter: u32) -> Vec<Target> {
    let mut objs = Vec::new();
    host.query(eye, range, shooter, &mut objs);
    let mut targets: Vec<Target> = objs
        .iter()
        .filter(|o| o.id != shooter && !o.has(objflags::DEAD) && !o.has(objflags::NO_COLLIDE) && !o.has(objflags::ALLY))
        .map(target_from)
        .collect();
    // Deterministic order regardless of partition iteration order.
    targets.sort_by_key(|t| t.id);
    targets
}

pub fn target_from(o: &ObjInfo) -> Target {
    if o.has(objflags::INFANTRY) {
        return Target {
            id: o.id,
            pos: o.pos,
            shape: Shape::Cylinder { radius: INFANTRY_HIT_RADIUS.min(o.major_radius.max(1.0)), height: o.height.max(6.0) },
            head_fraction: INFANTRY_HEAD_FRACTION,
        };
    }
    let shape = match o.geom_type {
        geom::BOX => Shape::Box { half_x: o.major_radius, half_y: o.minor_radius, height: o.height.max(1.0), yaw: o.yaw },
        geom::SPHERE => Shape::Sphere { radius: o.major_radius },
        _ => Shape::Cylinder { radius: o.major_radius, height: o.height.max(1.0) },
    };
    Target { id: o.id, pos: o.pos, shape, head_fraction: 0.0 }
}

/// [`World`] backed by the engine: terrain from `TheTerrainLogic`, solid footprints from structures
/// and ground vehicles.
pub struct HostWorld<'a> {
    pub host: &'a dyn LogicHost,
    pub viewer: u32,
}

impl World for HostWorld<'_> {
    fn ground_height(&self, x: f32, y: f32) -> f32 {
        self.host.ground_height(x, y)
    }

    fn is_walkable(&self, x: f32, y: f32) -> bool {
        self.host.is_walkable(x, y)
    }

    fn extent(&self) -> (Vec3, Vec3) {
        self.host.extent()
    }

    fn blockers(&self, pos: Vec3, radius: f32, out: &mut Vec<Blocker>) {
        let mut objs = Vec::new();
        self.host.query(pos, radius, self.viewer, &mut objs);
        objs.sort_by_key(|o| o.id);
        for o in objs.iter().filter(|o| o.id != self.viewer) {
            if o.has(objflags::DEAD) || o.has(objflags::NO_COLLIDE) || o.has(objflags::AIRBORNE) {
                continue;
            }
            if !(o.has(objflags::STRUCTURE) || o.has(objflags::VEHICLE)) {
                continue;
            }
            let top = o.pos.z + o.height.max(1.0);
            out.push(match o.geom_type {
                geom::BOX => Blocker::Box { center: o.pos, half_x: o.major_radius, half_y: o.minor_radius, yaw: o.yaw, top },
                _ => Blocker::Circle { center: o.pos, radius: o.major_radius, top },
            });
        }
    }
}

/// Where the hammer puts a structure with a footprint of `radius`, and which way it faces: its
/// front towards him, turned by `turn`.
pub fn placement(host: &dyn LogicHost, p: &PlayerSim, radius: f32, turn: f32) -> (Vec3, f32) {
    let pos = building::place_point(p.body.eye(), dir_from_angles(p.yaw, p.pitch), radius, |x, y| host.ground_height(x, y));
    (pos, crate::math::wrap_angle(p.yaw + std::f32::consts::PI + turn))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::input::pack_move;
    use std::cell::RefCell;

    /// A structure the fake host offers to work on.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct FakeSite {
        pub id: u32,
        pub template: u32,
        pub at: Vec3,
        pub angle: f32,
        pub percent: f32,
        pub kind: i32,
        /// Someone else is repairing it.
        pub busy: bool,
    }

    /// A flat world with a set of objects; records engine calls.
    #[derive(Default)]
    pub struct FakeHost {
        pub objects: RefCell<Vec<ObjInfo>>,
        pub controlled: RefCell<Vec<u32>>,
        pub fx: RefCell<Vec<(u32, String, u32)>>,
        pub damage_log: RefCell<Vec<(u32, u32, f32)>>,
        pub disabled: RefCell<Vec<(u32, i32, f32)>>,
        pub spawned: RefCell<Vec<(u32, String)>>,
        pub destroyed: RefCell<Vec<u32>>,
        pub played_fx: RefCell<Vec<String>>,
        pub smoked: RefCell<Vec<(Vec3, f32)>>,
        /// Building: the build menu, and the structures to work on (laid out, or damaged).
        pub options: RefCell<Vec<crate::building::BuildOption>>,
        pub sites: RefCell<Vec<FakeSite>>,
    }

    impl FakeHost {
        pub fn add(&self, info: ObjInfo) {
            self.objects.borrow_mut().push(info);
        }

        pub fn get(&self, id: u32) -> ObjInfo {
            *self.objects.borrow().iter().find(|o| o.id == id).unwrap()
        }

        fn rel(&self, mut o: ObjInfo, viewer: u32) -> ObjInfo {
            let owner = self.objects.borrow().iter().find(|v| v.id == viewer).map(|v| v.owner_player);
            o.flags &= !(objflags::ALLY | objflags::ENEMY);
            if let Some(owner) = owner {
                o.flags |= if o.owner_player == owner { objflags::ALLY } else { objflags::ENEMY };
            }
            o
        }
    }

    impl LogicHost for FakeHost {
        fn log(&self, _msg: &str) {}
        fn ground_height(&self, _x: f32, _y: f32) -> f32 {
            0.0
        }
        fn is_walkable(&self, _x: f32, _y: f32) -> bool {
            true
        }
        fn extent(&self) -> (Vec3, Vec3) {
            (Vec3::new(-5000.0, -5000.0, -100.0), Vec3::new(5000.0, 5000.0, 1000.0))
        }
        fn object(&self, id: u32, viewer: u32) -> Option<ObjInfo> {
            self.objects.borrow().iter().find(|o| o.id == id).map(|o| self.rel(*o, viewer))
        }
        fn query(&self, center: Vec3, radius: f32, viewer: u32, out: &mut Vec<ObjInfo>) {
            for o in self.objects.borrow().iter() {
                if (o.pos - center).length() <= radius {
                    out.push(self.rel(*o, viewer));
                }
            }
        }
        fn set_pose(&self, id: u32, pos: Vec3, yaw: f32, _moving: bool) {
            for o in self.objects.borrow_mut().iter_mut().filter(|o| o.id == id) {
                o.pos = pos;
                o.yaw = yaw;
            }
        }
        fn set_controlled(&self, id: u32, controlled: bool) {
            let mut c = self.controlled.borrow_mut();
            c.retain(|&x| x != id);
            if controlled {
                c.push(id);
            }
        }
        fn fire_fx(&self, shooter: u32, fx_weapon: &str, target: u32, _pos: Vec3) {
            self.fx.borrow_mut().push((shooter, fx_weapon.to_string(), target));
        }
        fn damage(&self, shooter: u32, victim: u32, amount: f32, _kind: i32) -> bool {
            self.damage_log.borrow_mut().push((shooter, victim, amount));
            let mut objs = self.objects.borrow_mut();
            let v = objs.iter_mut().find(|o| o.id == victim).unwrap();
            v.health -= amount;
            if v.health <= 0.0 {
                v.flags |= objflags::DEAD;
                return true;
            }
            false
        }
        fn heal(&self, id: u32, amount: f32) {
            for o in self.objects.borrow_mut().iter_mut().filter(|o| o.id == id) {
                o.health = (o.health + amount).min(o.max_health);
            }
        }
        fn spawn(&self, template: &str, _owner: u32, _pos: Vec3, _yaw: f32) -> u32 {
            let mut s = self.spawned.borrow_mut();
            let id = 10_000 + s.len() as u32;
            s.push((id, template.to_string()));
            id
        }
        fn destroy(&self, id: u32) {
            self.destroyed.borrow_mut().push(id);
        }
        fn disable(&self, id: u32, kind: i32, seconds: f32) {
            self.disabled.borrow_mut().push((id, kind, seconds));
        }
        fn fx(&self, name: &str, _pos: Vec3) {
            self.played_fx.borrow_mut().push(name.to_string());
        }
        fn smoke(&self, center: Vec3, radius: f32) {
            self.smoked.borrow_mut().push((center, radius));
        }
        fn build_options(&self, _general: u32) -> Vec<crate::building::BuildOption> {
            self.options.borrow().clone()
        }
        fn can_place(&self, _general: u32, _template: u32, pos: Vec3, _angle: f32) -> bool {
            self.sites.borrow().iter().all(|s| (s.at - pos).length_xy() > 80.0)
        }
        fn place(&self, _general: u32, template: u32, pos: Vec3, angle: f32) -> u32 {
            let mut sites = self.sites.borrow_mut();
            let id = 20_000 + sites.len() as u32;
            sites.push(FakeSite { id, template, at: pos, angle, ..Default::default() });
            id
        }
        fn work_site(&self, _general: u32, pos: Vec3, reach: f32) -> Option<crate::building::Site> {
            // Footprints 40 across.
            self.sites.borrow().iter().filter(|s| s.percent < 100.0 && (s.at - pos).length_xy() <= 20.0 + reach)
                .map(|s| crate::building::Site { id: s.id, template: s.template, kind: s.kind, percent: s.percent, ..Default::default() })
                .next()
        }
        fn work(&self, _general: u32, site: u32, rate: f32) -> f32 {
            let mut sites = self.sites.borrow_mut();
            let s = sites.iter_mut().find(|s| s.id == site).unwrap();
            if s.busy {
                return -1.0;
            }
            s.percent = (s.percent + 10.0 * rate).min(100.0);
            s.percent
        }
        fn capture(&self, id: u32, new_owner: u32) {
            let owner = self.objects.borrow().iter().find(|o| o.id == new_owner).map(|o| o.owner_player);
            if let Some(owner) = owner {
                for o in self.objects.borrow_mut().iter_mut().filter(|o| o.id == id) {
                    o.owner_player = owner;
                }
            }
        }
    }

    pub fn general(id: u32, player: i32) -> ObjInfo {
        ObjInfo {
            id,
            owner_player: player,
            flags: objflags::INFANTRY,
            geom_type: geom::CYLINDER,
            major_radius: 10.0,
            minor_radius: 10.0,
            height: 12.0,
            health: 300.0,
            max_health: 300.0,
            ..Default::default()
        }
    }

    pub fn soldier(id: u32, player: i32, x: f32) -> ObjInfo {
        ObjInfo { pos: Vec3::new(x, 0.0, 0.0), health: 120.0, max_health: 120.0, ..general(id, player) }
    }

    fn run(sim: &mut Sim, host: &FakeHost, player: i32, general: u32, input: FpsInput, frames: u32) {
        for _ in 0..frames {
            sim.input(player, general, input);
            let f = sim.frame + 1;
            sim.step(host, f);
        }
    }

    /// Logic frames covering `seconds`.
    fn frames(seconds: f32) -> u32 {
        (seconds / LOGIC_DT).ceil() as u32 + 1
    }

    /// Let the camera fly in, the tablet go down and the weapon come up.
    fn wait_until_ready(sim: &mut Sim, host: &FakeHost) {
        let ready = sim.player(0).unwrap().arsenal.raise_left;
        run(sim, host, 0, 1, FpsInput::default(), frames(ready));
    }

    fn events(sim: &Sim) -> Vec<EventKind> {
        sim.events.iter().map(|e| e.kind).collect()
    }

    /// The contract's core promise: identical inputs produce identical sim outcomes. Two Sims over
    /// two identical fake hosts run the same 1200 frames of seeded pseudo-random scripted input —
    /// shots, melee, reloads, alt modes, grenade throws, equipment planting, building, the Black
    /// Hat — and must agree on every event, on every engine call and on the final state (refactor
    /// plan 4.2: the regression net for the sim, weapons and equipment refactors; it also watches
    /// for HashMap iteration or float associativity drift).
    #[test]
    fn identical_inputs_are_deterministic() {
        const FRAMES: u32 = 1200;
        /// A cycle of scripted intents: settle, fire, reload, move, throw, hack, build, alt mode.
        /// Phases keep the weapon usable (a uniformly random button mask would perpetually
        /// interrupt the raise), and the xorshift only jitters within each phase.
        const CYCLE: u32 = 600;
        // A fixed-seed xorshift so a flaky RNG can never hide a real drift.
        let script = {
            let mut seed: u64 = 0x5EED_CAFE_F00D_D065;
            let mut next = || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed
            };
            let mut yaw = 0.0f32;
            let mut pitch = 0.0f32;
            let mut script = Vec::with_capacity(FRAMES as usize);
            for f in 0..FRAMES {
                let t = f % CYCLE;
                let (mut held, mut pressed) = (0u32, 0u32);
                let jitter = |r: u64, scale: f32| (r % 41) as f32 / 100.0 * scale - scale / 2.0;
                yaw += jitter(next(), 0.4);
                pitch = (pitch + jitter(next(), 0.1)).clamp(-crate::input::MAX_PITCH, crate::input::MAX_PITCH);
                let fwd = (next() % 5) as f32 / 2.0 - 1.0;
                let side = (next() % 5) as f32 / 2.0 - 1.0;
                // `pressed` must be a single-frame edge (the sim acts on every frame it is set);
                // held buttons derive their own press edge in the arsenal.
                match t {
                    // Settle: the weapon comes up, no buttons.
                    0..=39 => {}
                    // Fire bursts at the enemy soldier standing ahead.
                    40..=139 => {
                        held |= buttons::ADS | buttons::FIRE;
                        pitch = pitch.clamp(-0.15, 0.0);
                    }
                    // Reload.
                    140..=159 => held |= buttons::RELOAD,
                    // Sprint forward, then walk.
                    160..=189 => held |= buttons::SPRINT,
                    // Pull the lethal's pin (held), then let go: thrown. Aimed up so the blast
                    // lands clear of the General.
                    190..=229 => {
                        held |= buttons::LETHAL;
                        pitch = 0.6;
                    }
                    // Same for the tactical: its held edge must land after the lethal's 0.45 s
                    // throwing wind-down has finished consuming presses.
                    250..=289 => {
                        held |= buttons::TACTICAL;
                        pitch = 0.6;
                    }
                    // Melee swings.
                    300..=329 => {
                        if t % 6 == 0 {
                            pressed |= buttons::MELEE;
                        }
                    }
                    // Stance bits.
                    330..=349 => {
                        if next() % 3 == 0 {
                            held |= buttons::CROUCH;
                        }
                        if next() % 5 == 0 {
                            pressed |= buttons::JUMP;
                        }
                        if next() % 7 == 0 {
                            held |= buttons::PRONE;
                        }
                    }
                    // The alt mode (the launcher): toggle in for one frame, then fire a press
                    // edge after its raise finishes (a semi weapon needs a fresh press), aimed up
                    // so the blast lands clear.
                    350 => pressed |= buttons::ALT_MODE,
                    351..=374 => held |= buttons::FIRE,
                    376..=384 => {
                        held |= buttons::FIRE;
                        pitch = 0.5;
                    }
                    385 => pressed |= buttons::ALT_MODE,
                    // The Black Hat: open the field computer (one frame); its raise hands control
                    // back to the RTS view (the retake at 470 brings first person up again).
                    395 => pressed |= buttons::FIELD_COMPUTER,
                    // Bring the hammer up, place a structure, put it away (single-frame presses).
                    520 => pressed |= buttons::HAMMER,
                    545 => pressed |= buttons::FIRE,
                    565 => pressed |= buttons::HAMMER,
                    _ => {}
                }
                script.push(FpsInput {
                    held,
                    pressed,
                    move_packed: pack_move(fwd, side),
                    yaw,
                    pitch,
                });
            }
            script
        };

        let make_world = || {
            let host = FakeHost::default();
            host.add(general(1, 0));
            host.add(soldier(2, 1, 60.0));
            host.add(soldier(3, 1, 90.0));
            host.options.borrow_mut().push(crate::building::BuildOption {
                template: 7,
                cost: 100,
                status: crate::building::status::OK,
                radius: 20.0,
                ..Default::default()
            });
            let mut sim = Sim::new();
            sim.take_control(&host, 0, 1);
            // The lethal slot plants (a claymore) so the equipment path is exercised alongside the
            // thrown tactical.
            if let Some(p) = sim.players[0].as_mut() {
                p.arsenal = crate::weapons::Arsenal::with_loadout(
                    crate::weapons::Loadout::default().with_spec("lethal=claymore_mp"),
                );
            }
            (host, sim)
        };
        let (host_a, mut sim_a) = make_world();
        let (host_b, mut sim_b) = make_world();

        for input in &script {
            let f = sim_a.frame + 1;
            // Take control again after the field computer's RTS handoff (tests re-entry).
            if f % 600 == 470 {
                sim_a.take_control(&host_a, 0, 1);
                sim_b.take_control(&host_b, 0, 1);
            }
            sim_a.input(0, 1, *input);
            sim_a.step(&host_a, f);
            sim_b.input(0, 1, *input);
            sim_b.step(&host_b, f);
            assert_eq!(events(&sim_a), events(&sim_b), "event drift at frame {}", f);
            assert_eq!(host_a.objects, host_b.objects, "object drift at frame {}", f);
        }

        // The final state agrees: where the General is, what he holds, what the engine saw (a run
        // may legitimately end with the General dead on his own explosives — then he is dead in
        // both).
        assert_eq!(sim_a.player(0).is_none(), sim_b.player(0).is_none(), "the General's fate agrees");
        assert_eq!(host_a.get(1).pos, host_b.get(1).pos, "the General ended up in the same place");
        if let (Some(a), Some(b)) = (sim_a.player(0), sim_b.player(0)) {
            assert_eq!(a.arsenal, b.arsenal, "the whole arsenal agrees (rounds, clips, cooldowns)");
        }
        assert_eq!(sim_a.projectiles.len(), sim_b.projectiles.len());
        assert_eq!(sim_a.equipment, sim_b.equipment);
        assert_eq!(*host_a.spawned.borrow(), *host_b.spawned.borrow());
        assert_eq!(*host_a.sites.borrow(), *host_b.sites.borrow());
        // And the run actually exercised the systems (a silent no-op run would prove nothing).
        let kinds = events(&sim_a);
        assert!(kinds.iter().any(|k| matches!(k, EventKind::Shot { .. })), "fired");
        assert!(kinds.iter().any(|k| matches!(k, EventKind::Planted { .. })), "planted the claymore");
        assert!(kinds.contains(&EventKind::Thrown { lethal: false }), "threw a tactical");
        assert!(kinds.contains(&EventKind::Launched), "fired the launcher alt mode");
        assert!(kinds.contains(&EventKind::ReloadStarted), "reloaded");
        assert!(kinds.contains(&EventKind::FieldComputerOpened), "hacked");
        assert!(kinds.iter().any(|k| matches!(k, EventKind::Exploded { .. })), "something went off");
        assert!(kinds.iter().any(|k| matches!(k, EventKind::Melee)), "swung the knife");
        assert!(!host_a.sites.borrow().is_empty(), "built");
    }

    #[test]
    fn take_control_suspends_ai_and_moves_general() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        assert_eq!(*host.controlled.borrow(), vec![1]);
        run(&mut sim, &host, 0, 1, FpsInput { move_packed: pack_move(1.0, 0.0), ..Default::default() }, 30);
        assert!(host.get(1).pos.x > 20.0, "walked forward: {:?}", host.get(1).pos);
        assert_eq!(sim.controlled_ids().collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn cannot_control_someone_elses_general() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 1, 1);
        assert!(sim.player(1).is_none());
        assert!(host.controlled.borrow().is_empty());
    }

    #[test]
    fn shooting_kills_an_enemy_and_reports_hitmarkers() {
        let host = FakeHost::default();
        let mut g = general(1, 0);
        g.pos = Vec3::new(0.0, 0.0, 0.0);
        host.add(g);
        host.add(soldier(2, 1, 60.0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        wait_until_ready(&mut sim, &host);
        // Let the weapon raise, then aim (ADS for accuracy) and fire. Eye is at 10 units; soldier
        // body centre is at 6, so aim slightly down.
        let pitch = -(4.0f32 / 60.0).atan();
        let aim = FpsInput { held: buttons::ADS, pitch, ..Default::default() };
        run(&mut sim, &host, 0, 1, aim, 20);
        let fire = FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() };
        run(&mut sim, &host, 0, 1, fire, 20);
        assert!(host.get(2).has(objflags::DEAD), "enemy killed; damage {:?}", host.damage_log.borrow());
        let ev = events(&sim);
        assert!(ev.iter().any(|e| matches!(e, EventKind::HitMarker { kill: true, .. })));
        assert!(host.fx.borrow().iter().all(|(s, w, _)| *s == 1 && w == "ZHBO_MTAR_FX"));
    }

    #[test]
    fn bullets_pass_through_friendlies() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(soldier(3, 0, 30.0));
        host.add(soldier(2, 1, 60.0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        wait_until_ready(&mut sim, &host);
        let pitch = -(4.0f32 / 60.0).atan();
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS, pitch, ..Default::default() }, 20);
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() }, 10);
        assert_eq!(host.get(3).health, 120.0, "friendly untouched");
        assert!(host.get(2).health < 120.0, "enemy behind was hit");
    }

    fn option(template: u32, status: i32) -> crate::building::BuildOption {
        let mut o = crate::building::BuildOption { template, cost: 500, status, ..Default::default() };
        o.name[..4].copy_from_slice(b"Shed");
        o
    }

    #[test]
    fn the_hammer_places_a_structure_and_holding_use_builds_it() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.options.borrow_mut().extend([option(7, crate::building::status::OK), option(8, crate::building::status::OK)]);
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        wait_until_ready(&mut sim, &host);
        let ammo = sim.player(0).unwrap().arsenal.current().ammo;
        let look = |held: u32, pressed: u32| FpsInput { held, pressed, pitch: -0.5, ..Default::default() };
        run(&mut sim, &host, 0, 1, look(buttons::HAMMER, buttons::HAMMER), 1);
        assert!(sim.player(0).unwrap().hammer.is_some());
        // ADS picks the next structure, and does not aim the gun.
        run(&mut sim, &host, 0, 1, look(buttons::ADS, buttons::ADS), 1);
        run(&mut sim, &host, 0, 1, look(0, 0), frames(crate::building::HAMMER_RAISE));
        assert_eq!(sim.player(0).unwrap().arsenal.ads, 0.0);
        sim.events.clear();
        run(&mut sim, &host, 0, 1, look(buttons::FIRE, buttons::FIRE), 1);
        assert!(events(&sim).contains(&EventKind::BuildPlaced { template: 8 }), "{:?}", events(&sim));
        assert_eq!(sim.player(0).unwrap().arsenal.current().ammo, ammo, "the gun did not fire");
        let at = host.sites.borrow()[0].at;
        assert!(at.x >= crate::building::PLACE_MIN - 0.01 && at.x < crate::building::PLACE_RANGE && at.z == 0.0, "placed ahead on the ground: {at:?}");
        // One structure per hammer: it went away.
        assert!(sim.player(0).unwrap().hammer.is_none());
        assert!(events(&sim).contains(&EventKind::Hammer { up: false }));
        // Up again, on the same spot: no room.
        run(&mut sim, &host, 0, 1, look(buttons::HAMMER, buttons::HAMMER), 1);
        run(&mut sim, &host, 0, 1, look(0, 0), frames(crate::building::HAMMER_RAISE));
        sim.events.clear();
        run(&mut sim, &host, 0, 1, look(buttons::FIRE, buttons::FIRE), 1);
        assert!(events(&sim).contains(&EventKind::BuildRefused { status: crate::building::status::BLOCKED }));
        assert!(sim.player(0).unwrap().hammer.is_some(), "refused: still in hand");
        run(&mut sim, &host, 0, 1, look(buttons::HAMMER, buttons::HAMMER), 1);

        // Up to the site: the use key builds it (not the field computer), only while held.
        host.sites.borrow_mut()[0].at = Vec3::new(25.0, 0.0, 0.0);
        run(&mut sim, &host, 0, 1, look(buttons::FIELD_COMPUTER, buttons::FIELD_COMPUTER), 3);
        assert_eq!(sim.player(0).unwrap().mode, Mode::Fps);
        assert!(sim.player(0).unwrap().building);
        run(&mut sim, &host, 0, 1, look(0, 0), 2);
        let paused = host.sites.borrow()[0].percent;
        assert!((paused - 30.0).abs() < 0.01, "three frames of building: {paused}");
        sim.events.clear();
        run(&mut sim, &host, 0, 1, look(buttons::FIELD_COMPUTER, buttons::FIELD_COMPUTER), 7);
        assert!(events(&sim).contains(&EventKind::WorkDone { kind: crate::building::work::BUILD }), "{:?}", events(&sim));
        assert!(!sim.player(0).unwrap().building);
        // Built: the use key is the field computer's again.
        run(&mut sim, &host, 0, 1, look(0, 0), 1);
        run(&mut sim, &host, 0, 1, look(buttons::FIELD_COMPUTER, buttons::FIELD_COMPUTER), 1);
        assert!(matches!(sim.player(0).unwrap().mode, Mode::FieldComputer { .. }));
    }

    #[test]
    fn the_hammer_refuses_what_he_cannot_afford_and_puts_away_for_a_weapon() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.options.borrow_mut().push(option(7, crate::building::status::NO_MONEY));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        wait_until_ready(&mut sim, &host);
        run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::HAMMER, ..Default::default() }, 1);
        run(&mut sim, &host, 0, 1, FpsInput::default(), frames(crate::building::HAMMER_RAISE));
        sim.events.clear();
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::FIRE, pressed: buttons::FIRE, ..Default::default() }, 1);
        assert!(events(&sim).contains(&EventKind::BuildRefused { status: crate::building::status::NO_MONEY }));
        assert!(host.sites.borrow().is_empty());
        run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::SWAP_WEAPON, ..Default::default() }, 1);
        assert!(sim.player(0).unwrap().hammer.is_none());
    }

    #[test]
    fn the_wheel_turns_the_structure_being_placed() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.options.borrow_mut().push(option(7, crate::building::status::OK));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        wait_until_ready(&mut sim, &host);
        run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::HAMMER, ..Default::default() }, 1);
        run(&mut sim, &host, 0, 1, FpsInput::default(), frames(crate::building::HAMMER_RAISE));
        let facing = sim.player(0).unwrap().hammer.unwrap().angle;
        run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::ROTATE_LEFT, ..Default::default() }, 1);
        run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::ROTATE_LEFT, ..Default::default() }, 1);
        run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::ROTATE_RIGHT, ..Default::default() }, 1);
        let turned = sim.player(0).unwrap().hammer.unwrap().angle;
        assert!((crate::math::wrap_angle(turned - facing) - crate::building::ROTATE_STEP).abs() < 1e-4, "{facing} -> {turned}");
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::FIRE, pressed: buttons::FIRE, ..Default::default() }, 1);
        assert!((host.sites.borrow()[0].angle - turned).abs() < 1e-4, "placed as turned");
    }

    #[test]
    fn holding_use_at_a_damaged_structure_repairs_it() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let repair = crate::building::work::REPAIR;
        host.sites.borrow_mut().push(FakeSite { id: 5, template: 3, at: Vec3::new(30.0, 0.0, 0.0), percent: 60.0, kind: repair, ..Default::default() });
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        wait_until_ready(&mut sim, &host);
        let hold = FpsInput { held: buttons::FIELD_COMPUTER, pressed: buttons::FIELD_COMPUTER, ..Default::default() };
        run(&mut sim, &host, 0, 1, hold, 1);
        assert!(events(&sim).contains(&EventKind::Working { kind: repair, percent: 70.0 }), "{:?}", events(&sim));
        assert_eq!(sim.player(0).unwrap().mode, Mode::Fps, "the use key repaired, not the field computer");
        run(&mut sim, &host, 0, 1, hold, 3);
        assert!(events(&sim).contains(&EventKind::WorkDone { kind: repair }), "{:?}", events(&sim));

        // Someone else repairing it: refused once, when the key goes down.
        host.sites.borrow_mut().push(FakeSite { id: 6, template: 3, at: Vec3::new(30.0, 0.0, 0.0), percent: 50.0, kind: repair, busy: true, ..Default::default() });
        sim.events.clear();
        run(&mut sim, &host, 0, 1, hold, 1);
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::FIELD_COMPUTER, ..Default::default() }, 3);
        let refused = events(&sim).iter().filter(|e| **e == EventKind::BuildRefused { status: crate::building::status::BUSY }).count();
        assert_eq!(refused, 1, "{:?}", events(&sim));
        assert_eq!(host.sites.borrow()[1].percent, 50.0);
    }

    #[test]
    fn field_computer_returns_control_to_rts_and_back() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::FIELD_COMPUTER, ..Default::default() }, 1);
        assert!(matches!(sim.player(0).unwrap().mode, Mode::FieldComputer { .. }));
        assert_eq!(*host.controlled.borrow(), vec![1], "still controlled while the laptop comes up");
        run(&mut sim, &host, 0, 1, FpsInput::default(), frames(FIELD_COMPUTER_RAISE));
        assert_eq!(sim.player(0).unwrap().mode, Mode::Rts);
        assert!(host.controlled.borrow().is_empty(), "AI resumed");
        assert!(events(&sim).contains(&EventKind::ReturnedToRts { general: 1 }));
        sim.take_control(&host, 0, 1);
        assert_eq!(sim.player(0).unwrap().mode, Mode::Fps);
    }

    #[test]
    fn health_regenerates_after_delay_and_death_releases() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
        host.objects.borrow_mut()[0].health = 150.0;
        run(&mut sim, &host, 0, 1, FpsInput::default(), 30 * 4);
        assert!(host.get(1).health < 151.0, "no regen during the delay");
        run(&mut sim, &host, 0, 1, FpsInput::default(), 30 * 5);
        assert_eq!(host.get(1).health, 300.0, "regenerated to full");
        host.objects.borrow_mut()[0].flags |= objflags::DEAD;
        run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
        assert!(sim.player(0).is_none());
        assert!(events(&sim).contains(&EventKind::GeneralDied { general: 1 }));
    }

    #[test]
    fn rts_shots_use_the_generals_ammo_and_carry_into_first_person() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let full = sim.rts_ammo(1);
        // Zero Hour fires the rifle: one round a shot, the clip refilled from the spare rounds.
        let r = sim.rts_update(&host, 0, 1, 0, [1, 0, 0], None);
        assert_eq!((r.ammo[0], r.reserve[0]), (full.ammo[0] - 1, full.reserve[0]));
        assert_eq!(r.lock, -1);
        // Empty the rifle completely: the General is moved to the pistol.
        let rounds = (full.ammo[0] + full.reserve[0]) as u32;
        let r = sim.rts_update(&host, 0, 1, 0, [rounds, 0, 0], None);
        assert_eq!(r.ammo[0] + r.reserve[0], 0);
        assert_eq!(r.lock, 1, "to the pistol");
        // The player picks the pistol; a thrown grenade is one fewer, and he goes back to the pistol.
        let r = sim.rts_update(&host, 0, 1, 1, [0, 1, 1], Some(Vec3::new(200.0, 0.0, 0.0)));
        assert_eq!(r.ammo[2], full.ammo[2] - 1);
        assert_eq!(r.lock, 1, "back to the gun after the throw");
        assert_eq!(sim.projectiles.len(), 1, "a BO2 grenade, thrown as in first person");
        // Taking control keeps the ammo and the weapon in hand.
        sim.take_control(&host, 0, 1);
        let p = sim.player(0).unwrap();
        assert_eq!(p.arsenal.active, 1);
        assert_eq!(p.arsenal.weapons[1].ammo as i32, r.ammo[1]);
        assert_eq!(p.arsenal.weapons[0].ammo + p.arsenal.weapons[0].reserve, 0);
        // While in first person nothing is counted.
        let before = sim.rts_ammo(1);
        sim.rts_update(&host, 0, 1, 0, [5, 5, 1], None);
        assert_eq!(sim.rts_ammo(1), before);
    }

    #[test]
    fn rts_throws_land_near_the_target_and_can_be_tacticals() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let full = sim.rts_ammo(1);
        sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None);
        sim.rts_offhand(0, 1, true);
        let r = sim.rts_update(&host, 0, 1, 2, [0, 0, 1], Some(Vec3::new(150.0, 40.0, 0.0)));
        assert_eq!((r.tactical, r.ammo[2], r.tactical_selected), (full.tactical - 1, full.ammo[2], 1), "the tactical, not the lethal");
        let (_, proj) = &sim.projectiles[0];
        assert_eq!(proj.def.class, crate::weapons::Class::Tactical);
        // The arc comes down by the target.
        let mut p = proj.pos;
        let mut v = proj.vel;
        while v.z > 0.0 || p.z > 0.0 {
            v.z -= crate::projectiles::GRAVITY * LOGIC_DT;
            p = p + v * LOGIC_DT;
        }
        assert!((Vec3::new(p.x, p.y, 0.0) - Vec3::new(150.0, 40.0, 0.0)).length() < 8.0, "lands at {p:?}");
    }

    #[test]
    fn returning_to_rts_locks_the_weapon_in_hand() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.players[0].as_mut().unwrap().arsenal.active = 1;
        run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::FIELD_COMPUTER, ..Default::default() }, 1);
        run(&mut sim, &host, 0, 1, FpsInput::default(), frames(FIELD_COMPUTER_RAISE));
        assert_eq!(sim.player(0).unwrap().mode, Mode::Rts);
        assert_eq!(sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None).lock, 1, "the pistol, which was in hand");
        assert_eq!(sim.rts_update(&host, 0, 1, 1, [0, 0, 0], None).lock, -1, "once");
    }

    #[test]
    fn a_smoke_grenade_screens_for_its_time_then_clears() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let smoke: &'static crate::weapons::WeaponDef =
            Box::leak(Box::new(crate::weapons::WeaponDef { id: "willy_pete_mp".into(), effect: crate::weapons::Effect::Smoke, fuse_time: 0.5, ..Default::default() }));
        let p = Projectile::launch(smoke, 1, Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0), 0.5, &host, 0.0);
        sim.projectiles.push((0, p));
        run(&mut sim, &host, 0, 1, FpsInput::default(), frames(1.0));
        assert_eq!(sim.smokes.len(), 1);
        assert!(!host.smoked.borrow().is_empty());
        run(&mut sim, &host, 0, 1, FpsInput::default(), frames(crate::projectiles::SMOKE_TIME));
        assert!(sim.smokes.is_empty(), "cleared after {} s", crate::projectiles::SMOKE_TIME);
    }

    #[test]
    fn identical_inputs_give_identical_results() {
        let make = || {
            let host = FakeHost::default();
            host.add(general(1, 0));
            host.add(soldier(2, 1, 80.0));
            host.add(soldier(4, 1, 90.0));
            let mut sim = Sim::new();
            sim.take_control(&host, 0, 1);
            for i in 0..200u32 {
                let input = FpsInput {
                    held: if i % 7 < 4 { buttons::FIRE } else { buttons::SPRINT },
                    move_packed: pack_move(((i % 11) as f32 / 5.0) - 1.0, ((i % 5) as f32 / 2.0) - 1.0),
                    yaw: (i as f32 * 0.01).sin(),
                    pitch: -0.05,
                    ..Default::default()
                };
                sim.input(0, 1, input);
                sim.step(&host, i + 1);
            }
            let hits = host.damage_log.borrow().len();
            (host.get(1).pos, host.get(2).health, host.get(4).health, hits)
        };
        assert_eq!(make(), make());
    }
}
