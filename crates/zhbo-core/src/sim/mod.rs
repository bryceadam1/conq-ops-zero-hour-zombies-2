//! The deterministic, lockstep part of the mod: one [`PlayerSim`] per player whose General is
//! under first-person control. Driven by `zhbo_logic_*` calls from `GameLogic`; must produce
//! identical results on every client given identical inputs. The per-system stepping lives in the
//! child modules (fire, build, hack, rts); this file owns the state, the orchestration and the
//! events.
/*
** Conquer Ops - Zero Hour gameplay core: the lockstep simulation's state and orchestration.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

mod build;
mod fire;
mod hack;
mod rts;

pub use build::{placement, HostWorld, PLANT_DISTANCE};
pub use fire::{gather_targets, throw_velocity, PENETRATION_DAMAGE};
pub use rts::{RtsIcons, RtsState, RtsWeapons, Third};

use crate::host::{damage_kind, geom, objflags, LogicHost, ObjInfo};
use crate::hitscan::{self, HitResult, Shape, Target, INFANTRY_HEAD_FRACTION, INFANTRY_HIT_RADIUS};
use crate::impacts::Surface;
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
use crate::weapons::{Arsenal, Class, Rng, Specialty, WeaponAction, MELEE_FX_WEAPON, MELEE_RANGE};

pub const MAX_PLAYERS: usize = 16;

/// How long the field computer takes to come up before control returns to the RTS view: BO2's
/// scorestreak tablet pullout, then a moment on its screen.
pub const FIELD_COMPUTER_RAISE: f32 = crate::viewmodel::TABLET_PULLOUT_TIME + 0.35;
/// Taking control, the weapon is ready after the camera flies in, the tablet goes down and the
/// weapon comes up.
pub const TAKE_CONTROL_READY: f32 = crate::client::ENTER_TIME + crate::viewmodel::TABLET_PUTAWAY_TIME;
/// BO2 health regeneration: wait this long after the last hit, then regenerate. The values come
/// from the tuning pack (`data/ConquerOps/tuning.ini`); these aliases are for the sim tests.
pub fn regen_delay() -> f32 {
    crate::tuning::get().regen_delay
}
pub fn regen_full_time() -> f32 {
    crate::tuning::get().regen_full_time
}
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

/// A class name as the log carries it: a fixed inline buffer so [`EventKind`] stays `Copy` (the
/// events are copied out of the sim's ring, and a shipped class's name and a custom one's both fit
/// in [`NAME_MAX`](crate::classmenu::NAME_MAX) bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassName {
    bytes: [u8; crate::classmenu::NAME_MAX],
    len: u8,
}

impl ClassName {
    /// The name of a shipped class by its registry index, or `???` when the index is out of range.
    pub fn shipped(index: usize) -> Self {
        match crate::classes::at(index) {
            Some(c) => Self::new(&c.name),
            None => Self::new("???"),
        }
    }

    /// A name from any string, truncated to the inline buffer (byte-wise, so a multi-byte name can
    /// split; class names are the player's own printable keys, so that is not reachable in practice).
    pub fn new(name: &str) -> Self {
        let mut bytes = [0u8; crate::classmenu::NAME_MAX];
        let n = name.len().min(crate::classmenu::NAME_MAX);
        bytes[..n].copy_from_slice(&name.as_bytes()[..n]);
        Self { bytes, len: n as u8 }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("???")
    }
}

impl core::fmt::Display for ClassName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventKind {
    EnteredFps { general: u32 },
    /// The pick menu's choice applied: the General carries the class kit now. The name is carried
    /// here (not the pick index) so the log names a class the player built in the editor as well as
    /// a shipped one — a custom class has no registry index to look up.
    ClassChosen { general: u32, class: ClassName },
    FieldComputerOpened,
    ReturnedToRts { general: u32 },
    Shot { origin: Vec3, end: Vec3 },
    /// One pellet landed on something (a miss emits nothing): where, the direction it was going,
    /// what it hit, and the shooting weapon's BO2 impact bucket. The client turns it into BO2's
    /// impact FX (through `ZhboClientOut.hits`) and the surface's impact sound.
    Impact { at: Vec3, dir: Vec3, surface: Surface, head: bool, fatal: bool, impact_type: i32 },
    HitMarker { headshot: bool, kill: bool },
    /// A shot's view kick (G5: pitch up, yaw, roll -0.5 x yaw) and the gun kick spring's impulse
    /// (`weapons::recoil::GunKickSpring`).
    Recoil { pitch_deg: f32, yaw_deg: f32, roll_deg: f32, gun_kick: (f32, f32) },
    /// Damage flinch (G7): the view kicks this far, the engine's no-direction branch (the sim
    /// carries no damage direction), and settles over `FlinchTime`.
    Flinch { pitch_deg: f32 },
    Damaged { amount: f32 },
    ReloadStarted,
    Swapped,
    GeneralDied { general: u32 },
    /// Melee swing (hit or miss).
    Melee,
    /// The melee landed on someone (BO2's `meleeHitSound` moment).
    MeleeHit,
    /// Offhand pulled back (pin out); then thrown.
    OffhandReady { lethal: bool },
    Thrown { lethal: bool },
    /// A launcher fired.
    Launched,
    /// A grenade, rocket, other projectile or piece of equipment went off on `surface` (what it hit
    /// directly, else the ground). `sound` is its def's own blast event (`projExplosionSound`, empty
    /// when it has none) and `impact_type` its `impactType`: the client plays the impact table's
    /// sound for the type and surface, then the def's own (`sounds::explosion_cues`).
    Exploded { at: Vec3, sound: &'static str, impact_type: i32, surface: crate::impacts::Surface },
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
    /// The class the General assumed (`classes.rs` id), or `None` while the pick menu is up:
    /// the first control after a spawn (a fresh `PlayerSim`) picks once.
    pub class: Option<String>,
    /// The class's reticle choice (`reticle_N`), carried when a `CustomClass` lands.
    pub reticle: String,
    /// The class's reticle colour choice (`reticle_color_N`), carried when a `CustomClass` lands.
    pub reticle_color: String,
    /// The class's camo choice (`camo_*`), carried when a `CustomClass` lands.
    pub camo: String,
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
    /// Under RTS control: the weapon lock, what the third slot does and the gun to go back to
    /// (`rts.rs`).
    pub rts: RtsState,
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
            class: Some("assault".into()),
            reticle: String::new(),
            reticle_color: String::new(),
            camo: String::new(),
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
            rts: RtsState::default(),
            hammer: None,
            site: None,
            building: false,
        }
    }
}

/// One General's native third-person frame: what the bridge needs to draw his body and the weapon
/// in his hand without asking the sim again (Track R7). Presentation only; nothing reads it back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativePose {
    pub general: u32,
    /// The converted weapon index in hand, or -1 for none (the knife/unarmed).
    pub weapon: i32,
    /// 0 stand, 1 crouch, 2 prone.
    pub stance: i32,
    /// The body clip (`zhbo_native::thirdperson::state`).
    pub state: u16,
    /// Seconds into the clip and its playback rate.
    pub time: f32,
    pub rate: f32,
    /// The second gun is in hand (BO2's pistol stance).
    pub sidearm: bool,
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

    /// A sim configured from the gameplay tuning pack (`data/ConquerOps/tuning.ini`, loaded by the
    /// bridge). The tests use `Sim::new`, whose defaults are the pack's own values.
    pub fn with_tuning(t: &crate::tuning::Tuning) -> Self {
        let mut sim = Self::new();
        sim.tuning = MoveTuning::from_tuning(t);
        sim
    }

    pub fn reset(&mut self) {
        *self = Sim::with_tuning(&crate::tuning::get());
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
                    class: None,
                    reticle: String::new(),
                    reticle_color: String::new(),
                    camo: String::new(),
                    mode: Mode::Fps,
                    body: Body::new(pos, &tuning),
                    yaw,
                    pitch: 0.0,
                    // A General spawns unarmed: knife only until a class is picked. (The class
                    // pick replaces this arsenal; Escape on the spawn offer leaves it.)
                    arsenal: Arsenal::unarmed(),
                    rng: Rng::new(general.wrapping_mul(2_654_435_761) ^ frame),
                    input: FpsInput { yaw, ..Default::default() },
                    pressed_accum: 0,
                    health: info.health,
                    max_health: info.max_health,
                    regen_wait: 0.0,
                    hack: None,
                    rts: RtsState::default(),
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

    /// `MSG_ZHBO_CHOOSE_CLASS`: the player picked a class for their General from the pick menu.
    /// An empty `payload` means a shipped class (`classes.rs`' list, by index — identical on every
    /// client, so the index is deterministic); a non-empty payload carries a custom class's wire
    /// words (see `classes.rs`' `CustomClass::to_wire_words`), which the sim decodes and applies
    /// as-is (the spec travels with the pick, so no shared list is needed).
    /// The first pick after a spawn applies: the kit replaces the base kit, and the weapon raises
    /// like after take_control (BO2: the pick happens between spawns, not mid-fight).
    pub fn choose_class(&mut self, player: i32, general: u32, index: usize, payload: &[u32]) {
        let Ok(idx) = usize::try_from(player) else { return };
        if idx >= MAX_PLAYERS {
            return;
        }
        // The class can be picked before the General is ever taken into first person: the command
        // bar's Create a Class button opens the pick while he is still under RTS control. Make a
        // fresh record for him so the kit is in place when he takes the field; its position is
        // filled in from the object when he does. He is under RTS control here (a first-person
        // pick always has the record `take_control` made), so the class is ready immediately.
        let mut p = match self.players[idx].take() {
            Some(p) => p,
            None => {
                let tuning = self.tuning;
                let frame = self.frame;
                PlayerSim {
                    general,
                    class: None,
                    reticle: String::new(),
                    reticle_color: String::new(),
                    camo: String::new(),
                    mode: Mode::Rts,
                    body: Body::new(Vec3::ZERO, &tuning),
                    yaw: 0.0,
                    pitch: 0.0,
                    arsenal: Arsenal::unarmed(),
                    rng: Rng::new(general.wrapping_mul(2_654_435_761) ^ frame),
                    input: FpsInput::default(),
                    pressed_accum: 0,
                    health: 0.0,
                    max_health: 0.0,
                    regen_wait: 0.0,
                    hack: None,
                    rts: RtsState::default(),
                    hammer: None,
                    site: None,
                    building: false,
                }
            }
        };
        if p.general != general {
            self.players[idx] = Some(p);
            return;
        }
        let (class_id, class_name, loadout, reticle, reticle_color, camo) = if payload.is_empty() {
            let Some(c) = crate::classes::at(index) else { self.players[idx] = Some(p); return };
            (c.id.clone(), ClassName::new(&c.name), crate::classes::loadout_of(&c), String::new(), String::new(), String::new())
        } else {
            let Some(c) = crate::classes::CustomClass::from_wire_words(payload) else {
                self.players[idx] = Some(p);
                return;
            };
            let name = if c.name.is_empty() { "Custom Class".to_string() } else { c.name.clone() };
            (format!("custom:{}", c.name), ClassName::new(&name), c.loadout(), c.reticle.clone(), c.reticle_color.clone(), c.camo.clone())
        };
        p.class = Some(class_id);
        p.reticle = reticle;
        p.reticle_color = reticle_color;
        p.camo = camo;
        p.arsenal = Arsenal::with_loadout(loadout);
        // The kit is ready as soon as the class lands, whichever view the player is in. In first
        // person the weapon still has to come up (BO2: the pick happens between spawns, not
        // mid-fight); in overhead it is ready for the next time the General takes the field.
        if matches!(p.mode, Mode::Fps) {
            let first = p.arsenal.current().def.first_raise_time;
            p.arsenal.raise_left = TAKE_CONTROL_READY + first;
            p.arsenal.raise_total = first;
            p.arsenal.raise_kind = crate::weapons::RaiseKind::First;
            p.arsenal.drop_left = 0.0;
            p.arsenal.offhand = None;
            p.arsenal.ads = 0.0;
        } else {
            // A class picked while commanding: mark it so the next Take Control readies the kit
            // without re-raising a weapon the General already has out.
            p.arsenal.raise_left = 0.0;
            p.arsenal.raise_total = 0.0;
        }
        self.players[idx] = Some(p);
        self.emit(idx, EventKind::ClassChosen { general, class: class_name });
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
                Outcome::Exploded { at, hit_enemy, kill, surface } => {
                    let d = proj.def;
                    self.emit(*owner, EventKind::Exploded { at, sound: d.explode_sound.as_str(), impact_type: d.impact_type, surface });
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
        let engineer = |id: u32| players.iter().flatten().any(|p| p.general == id && p.arsenal.class_specialties().has(Specialty::ShowEnemyEquipment));
        let mut ev = Vec::new();
        self.equipment.step(host, &mut self.projectiles, &engineer, &mut ev);
        self.equipment_events(ev);
    }

    fn equipment_events(&mut self, events: Vec<EquipEvent>) {
        for e in events {
            match e {
                EquipEvent::Exploded { owner, at, hit_enemy, kill, sound, impact_type, surface } => {
                    self.emit(owner, EventKind::Exploded { at, sound, impact_type, surface });
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
            if p.arsenal.has_specialty(Specialty::Scavenger) {
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
                        move_faster: p.arsenal.move_faster(),
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
                    p.rts.lock = p.arsenal.active as i32;
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

    /// What each General's native third person needs this frame (Track R7): the weapon in hand
    /// (`WeaponDef::viewmodel`, -1 for none), the stance and the body clip the sim's movement
    /// state picks, the clip's clock and the pistol stance. `moving` is the engine's answer for
    /// the General the bridge asks about while he is under RTS control (the sim does not step his
    /// body then), ignored while first person.
    pub fn native_poses(&self, moving: Option<(u32, bool)>) -> Vec<NativePose> {
        let run_speed = self.tuning.run_speed(false);
        let time = self.frame as f32 * LOGIC_DT;
        self.players
            .iter()
            .flatten()
            // First person draws the viewmodel's arms; only the overhead view needs the body.
            .filter(|p| !p.is_first_person())
            .map(|p| {
                let a = &p.arsenal;
                let stance = match p.body.stance {
                    movement::Stance::Stand => 0,
                    movement::Stance::Crouch => 1,
                    movement::Stance::Prone => 2,
                };
                let engine_moving = moving.is_some_and(|(g, m)| g == p.general && m) && !p.is_first_person();
                let speed = p.body.horizontal_speed();
                NativePose {
                    general: p.general,
                    weapon: a.current().def.viewmodel,
                    stance,
                    state: crate::nativebody::state_of(stance, speed, p.body.sprinting, a.ads, a.active == 1, engine_moving, run_speed),
                    time,
                    rate: 1.0,
                    sidearm: a.active == 1,
                }
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
            p.regen_wait = regen_delay();
            let amount = p.health - info.health;
            self.emit(idx, EventKind::Damaged { amount });
            let toughness = p.arsenal.class_specialties().has(Specialty::BulletFlinch);
            let flinch = crate::weapons::recoil::flinch_degrees(amount / crate::units::BO2_TO_ZH_DAMAGE, toughness, &p.arsenal.tuning);
            self.emit(idx, EventKind::Flinch { pitch_deg: flinch });
        } else {
            p.regen_wait = (p.regen_wait - LOGIC_DT).max(0.0);
            if p.regen_wait <= 0.0 && info.health < info.max_health {
                let amount = (info.max_health / regen_full_time() * LOGIC_DT).min(info.max_health - info.health);
                host.heal(p.general, amount);
                p.health = info.health + amount;
                return;
            }
        }
        p.health = info.health;
    }

}

/// The log line for an event, or `None` for the quiet ones (the ffi log and the tests share this
/// vocabulary, so the event enum has one wording).
pub fn describe_event(e: &Event) -> Option<String> {
    use EventKind::*;
    let msg = match e.kind {
        EnteredFps { general } => format!("player {} took control of General {general}", e.player),
        ClassChosen { general, class } => {
            format!("player {}'s General {general} assumed the {class} class", e.player)
        }
        FieldComputerOpened => format!("player {} opened the field computer", e.player),
        ReturnedToRts { general } => format!("player {} returned General {general} to RTS control", e.player),
        GeneralDied { general } => format!("player {}'s General {general} died", e.player),
        HitMarker { headshot, kill: true } => format!("player {} killed a target (headshot: {headshot})", e.player),
        Damaged { amount } => format!("player {}'s General took {amount:.0} damage", e.player),
        Swapped => format!("player {} swapped weapons", e.player),
        Melee => format!("player {} meleed", e.player),
        MeleeHit => format!("player {}'s melee landed", e.player),
        Thrown { lethal } => format!("player {} threw a {}", e.player, if lethal { "lethal" } else { "tactical" }),
        Exploded { at, .. } => format!("player {}'s grenade went off at ({:.0}, {:.0})", e.player, at.x, at.y),
        Planted { kind } => format!("player {} put down equipment ({kind:?})", e.player),
        Detonated => format!("player {} set off their C4", e.player),
        Intercepted { .. } => format!("player {}'s Trophy System shot down a projectile", e.player),
        DestroyedEquipment => format!("player {} destroyed enemy equipment", e.player),
        Hacked => format!("player {} hacked with the Black Hat", e.player),
        AltToggled => format!("player {} toggled the weapon's alt mode", e.player),
        _ => return None,
    };
    Some(msg)
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
        /// The ground's surface (`surface_at`); dirt when unset.
        pub ground: std::cell::Cell<Option<Surface>>,
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
        fn surface_at(&self, _x: f32, _y: f32) -> Surface {
            self.ground.get().unwrap_or(Surface::Dirt)
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
                        if t.is_multiple_of(6) {
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

    /// Bug 1: a General spawns with no guns and no grenades, only the melee knife, until a class
    /// is chosen. `Loadout::default()` (the classes' base kit) is untouched.
    #[test]
    fn a_fresh_general_spawns_unarmed() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        let p = sim.player(0).unwrap();
        assert!(p.class.is_none());
        let a = &p.arsenal;
        assert!(a.is_unarmed(), "no gun at spawn");
        assert_eq!(a.weapons.len(), 1, "the knife is the only weapon");
        assert_eq!(a.current().def.class, crate::weapons::Class::Melee);
        assert_eq!(a.melee.id, "knife_held_mp");
        assert_eq!((a.lethal.count, a.tactical.count), (0, 0), "no grenades");
        assert!(a.loadout.primary.is_empty() && a.loadout.secondary.is_empty());
        assert!(a.loadout.perks.is_empty());
        // The full base kit is still what the classes build on.
        let base = crate::weapons::Loadout::default();
        assert!(!base.primary.is_empty() && !base.secondary.is_empty() && !base.lethal.is_empty());
    }

    /// Unarmed is a no-op for every gun action (no panic, no shot, no index-0 fallback); the knife
    /// still swings, and the hammer and field computer still work.
    #[test]
    fn unarmed_weapon_input_is_a_no_op_and_melee_still_works() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        wait_until_ready(&mut sim, &host);
        let everything = buttons::FIRE
            | buttons::ADS
            | buttons::RELOAD
            | buttons::SWAP_WEAPON
            | buttons::ALT_MODE
            | buttons::LETHAL
            | buttons::TACTICAL;
        run(&mut sim, &host, 0, 1, FpsInput { held: everything, ..Default::default() }, 40);
        let kinds = events(&sim);
        for kind in [EventKind::Swapped, EventKind::AltToggled, EventKind::ReloadStarted, EventKind::Launched, EventKind::Detonated] {
            assert!(!kinds.contains(&kind), "unarmed did not {kind:?}: {kinds:?}");
        }
        assert!(!kinds.iter().any(|k| matches!(k, EventKind::Shot { .. } | EventKind::Thrown { .. } | EventKind::OffhandReady { .. })), "{kinds:?}");
        assert_eq!(sim.player(0).unwrap().arsenal.ads, 0.0, "nothing to aim");
        assert!(sim.projectiles.is_empty());
        // The knife: BO2 always has melee.
        sim.events.clear();
        run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::MELEE, ..Default::default() }, 1);
        assert!(events(&sim).contains(&EventKind::Melee));
    }

    #[test]
    fn the_hammer_and_field_computer_work_unarmed() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.options.borrow_mut().push(option(7, crate::building::status::OK));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        wait_until_ready(&mut sim, &host);
        assert!(sim.player(0).unwrap().arsenal.is_unarmed());
        let look = |held: u32, pressed: u32| FpsInput { held, pressed, pitch: -0.5, ..Default::default() };
        run(&mut sim, &host, 0, 1, look(buttons::HAMMER, buttons::HAMMER), 1);
        run(&mut sim, &host, 0, 1, look(0, 0), frames(crate::building::HAMMER_RAISE));
        run(&mut sim, &host, 0, 1, look(buttons::FIRE, buttons::FIRE), 1);
        assert!(events(&sim).contains(&EventKind::BuildPlaced { template: 7 }), "{:?}", events(&sim));
        sim.events.clear();
        run(&mut sim, &host, 0, 1, look(buttons::FIELD_COMPUTER, buttons::FIELD_COMPUTER), 1);
        assert!(events(&sim).contains(&EventKind::FieldComputerOpened), "{:?}", events(&sim));
    }

    /// Bug 1, overhead half: a classless General reports no usable slot/ammo, so the bridge's
    /// "an empty weapon can't fire" veto holds Zero Hour's own weapon; after a class lands the kit
    /// fires exactly as before, and the RTS update never re-creates the record over the kit.
    #[test]
    fn a_classless_general_under_rts_cannot_fire_then_fires_with_its_class_kit() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        // The bridge calls rts_update every logic frame; the first call makes the record unarmed.
        let r = sim.rts_update(&host, 0, 1, 0, [3, 3, 3], None);
        assert_eq!(r.ammo, [0, 0, 0]);
        assert_eq!(r.reserve, [0, 0, 0]);
        assert_eq!(r.lock, -1, "nothing to switch to");
        assert_eq!(sim.rts_ammo(1).ammo, [0, 0, 0], "the control bar shows no usable weapon");
        assert!(sim.projectiles.is_empty(), "the third slot threw nothing");
        // A class lands from the command bar: the kit is there immediately.
        sim.choose_class(0, 1, 0, &[]);
        let full = sim.rts_ammo(1);
        assert!(full.ammo[0] > 0 && full.ammo[1] > 0, "the class kit's guns: {full:?}");
        let r = sim.rts_update(&host, 0, 1, 0, [1, 0, 0], None);
        assert_eq!((r.ammo[0], r.reserve[0]), (full.ammo[0] - 1, full.reserve[0]));
        assert_eq!(sim.player(0).unwrap().arsenal.weapons[0].main.id, "tar21_mp", "the kit was not reset");
        assert_eq!(sim.player(0).unwrap().class.as_deref(), Some("assault"));
    }

    /// Bug 3, overhead half: a custom class picked while commanding keeps its exact kit through
    /// `take_control` (which must not re-create the record or re-apply the base kit), and the
    /// weapon is ready as soon as first person starts.
    #[test]
    fn an_overhead_custom_pick_keeps_its_kit_through_take_control() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        // The bridge's per-frame rts_update makes the (unarmed) record before the pick.
        sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None);
        let custom = crate::classes::CustomClass {
            name: "Overhead".into(),
            primary: "fnp45_mp".into(),
            secondary: "an94_mp".into(),
            lethal: "claymore_mp".into(),
            tactical: "concussion_grenade_mp".into(),
            melee: "knife_held_mp".into(),
            perks: vec!["Toughness".into()],
            ..Default::default()
        };
        sim.choose_class(0, 1, usize::MAX, &custom.to_wire_words());
        // In overhead the kit is ready now (no first-person raise).
        assert_eq!(sim.player(0).unwrap().arsenal.raise_left, 0.0);
        sim.take_control(&host, 0, 1);
        let p = sim.player(0).unwrap();
        assert_eq!(p.class.as_deref(), Some("custom:Overhead"));
        let a = &p.arsenal;
        assert_eq!(a.weapons[0].main.id, "fnp45_mp");
        assert_eq!(a.weapons[1].main.id, "an94_mp");
        assert_eq!(a.lethal.def.id, "claymore_mp");
        assert!(a.has(crate::weapons::Perk::TOUGHNESS));
        assert!(a.raise_left >= TAKE_CONTROL_READY, "the weapon still comes up in first person");
        // A later rts_update does not re-create/reset the record.
        sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None);
        assert_eq!(sim.player(0).unwrap().arsenal.weapons[0].main.id, "fnp45_mp");
        // A re-pick in first person (the spawn offer on a later take-control) replaces the kit.
        let second = crate::classes::CustomClass {
            name: "Rifle".into(),
            primary: "an94_mp".into(),
            secondary: "fnp45_mp".into(),
            lethal: "frag_grenade_mp".into(),
            tactical: "concussion_grenade_mp".into(),
            melee: "knife_held_mp".into(),
            ..Default::default()
        };
        sim.choose_class(0, 1, usize::MAX, &second.to_wire_words());
        let a = &sim.player(0).unwrap().arsenal;
        assert_eq!(a.weapons[0].main.id, "an94_mp", "the re-pick replaced the first kit");
        assert!(a.raise_left >= TAKE_CONTROL_READY, "the first-person pick re-raises");
    }

    #[test]
    fn custom_class_pick_logs_its_own_name() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        let custom = crate::classes::CustomClass {
            name: "Ghost Recon".into(),
            ..Default::default()
        };
        let words = custom.to_wire_words();
        sim.choose_class(0, 1, usize::MAX, &words);
        let ev = events(&sim);
        let logged = ev.iter().find_map(|e| match e {
            EventKind::ClassChosen { class, .. } => Some(*class),
            _ => None,
        });
        match logged {
            Some(name) => assert_eq!(name.as_str(), "Ghost Recon"),
            other => panic!("expected custom class name, got {other:?}"),
        }
    }

    #[test]
    fn a_custom_class_carries_its_camo_choice() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        let custom = crate::classes::CustomClass { camo: "camo_gold".into(), ..Default::default() };
        sim.choose_class(0, 1, usize::MAX, &custom.to_wire_words());
        assert_eq!(sim.player(0).unwrap().camo, "camo_gold");
    }

    #[test]
    fn class_chosen_from_overhead_applies_its_kit() {
        // A General built from the Command Center and still under RTS control: the class picked
        // from the command bar must replace the base kit without first-person control. He has no
        // `PlayerSim` yet (he was never taken into first person), so the pick creates the record.
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        assert!(sim.player(0).is_none(), "no record before the pick");
        sim.choose_class(0, 1, 0, &[]);
        let p = sim.player(0).expect("the pick made the record");
        assert_eq!(p.class.as_deref(), Some(crate::classes::at(0).unwrap().id.as_str()));
        let ev = events(&sim);
        assert!(
            ev.iter().any(|e| matches!(e, EventKind::ClassChosen { .. })),
            "the overhead pick emitted ClassChosen"
        );
    }

    #[test]
    fn class_pick_replaces_an_existing_class() {
        // Re-opening the pick screen (the lobby/command-bar button) and choosing again swaps the
        // kit instead of being ignored.
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        let first = sim.player(0).and_then(|p| p.class.clone());
        sim.choose_class(0, 1, 1, &[]);
        let second = sim.player(0).and_then(|p| p.class.clone());
        assert_ne!(first, second, "the second pick replaced the first");
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
        // A General spawns unarmed; this test is about a gun, so pick the first preset.
        sim.choose_class(0, 1, 0, &[]);
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
    fn a_landed_melee_reports_its_hit_and_a_miss_does_not() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        wait_until_ready(&mut sim, &host);
        sim.events.clear();
        run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::MELEE, ..Default::default() }, 1);
        let ev = events(&sim);
        assert!(ev.contains(&EventKind::Melee) && !ev.contains(&EventKind::MeleeHit), "a swing at the air: {ev:?}");
        run(&mut sim, &host, 0, 1, FpsInput::default(), 60);
        // An enemy at arm's length straight ahead (yaw 0 faces +x).
        host.add(soldier(2, 1, 8.0));
        sim.events.clear();
        run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::MELEE, ..Default::default() }, 1);
        let ev = events(&sim);
        assert!(ev.contains(&EventKind::Melee) && ev.contains(&EventKind::MeleeHit), "{ev:?}");
    }

    #[test]
    fn a_pellet_reports_the_surface_it_hit() {
        // An enemy structure straight ahead: the pellets land on concrete, not on the ground.
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(ObjInfo {
            id: 2,
            owner_player: 1,
            flags: objflags::STRUCTURE,
            geom_type: geom::BOX,
            major_radius: 20.0,
            minor_radius: 20.0,
            height: 40.0,
            health: 5000.0,
            max_health: 5000.0,
            pos: Vec3::new(60.0, 0.0, 0.0),
            ..Default::default()
        });
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        wait_until_ready(&mut sim, &host);
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS, ..Default::default() }, 20);
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS | buttons::FIRE, ..Default::default() }, 20);
        let impacts: Vec<_> = events(&sim)
            .into_iter()
            .filter_map(|k| match k {
                EventKind::Impact { surface, at, .. } => {
                    assert!((at.x - 40.0).abs() < 1.0, "the impact is on the wall's near face, got {at:?}");
                    Some(surface)
                }
                _ => None,
            })
            .collect();
        assert!(!impacts.is_empty(), "the wall took pellets");
        assert!(impacts.iter().all(|s| *s == Surface::Concrete), "{impacts:?}");
    }

    #[test]
    fn a_shot_that_hits_nothing_raises_no_impact() {
        // Up into the sky, nothing in front: the bullet flies to its range and stops in the air.
        // (Level fire is not enough: the hip spread cone reaches the ground at long range, which
        // is a real ground hit and rightly raises an impact.)
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        wait_until_ready(&mut sim, &host);
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::FIRE, pitch: 0.3, ..Default::default() }, 10);
        assert!(
            !events(&sim).iter().any(|k| matches!(k, EventKind::Impact { .. })),
            "no hit, no impact (and so no impact sound)"
        );
    }

    #[test]
    fn a_missed_shot_into_the_ground_asks_the_host_for_its_surface() {
        // Firing down: the pellets stop on the terrain, whose surface is the host's answer.
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        wait_until_ready(&mut sim, &host);
        let down = FpsInput { held: buttons::FIRE, pitch: -0.6, ..Default::default() };
        run(&mut sim, &host, 0, 1, down, 10);
        let impacts: Vec<_> = events(&sim)
            .into_iter()
            .filter_map(|k| match k {
                EventKind::Impact { surface, .. } => Some(surface),
                _ => None,
            })
            .collect();
        assert!(!impacts.is_empty(), "the ground took pellets");
        assert!(impacts.iter().all(|s| *s == Surface::Dirt), "{impacts:?}");
    }

    #[test]
    fn a_kill_reports_a_fatal_flesh_impact() {
        // The template test's shot, asserting the record's blood bits: body (not head), fatal.
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(soldier(2, 1, 60.0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        wait_until_ready(&mut sim, &host);
        let pitch = -(4.0f32 / 60.0).atan();
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS, pitch, ..Default::default() }, 20);
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() }, 20);
        assert!(host.get(2).has(objflags::DEAD), "the soldier was killed");
        let flesh: Vec<_> = events(&sim)
            .into_iter()
            .filter_map(|k| match k {
                EventKind::Impact { surface, head, fatal, .. } => (surface == Surface::Flesh).then_some((head, fatal)),
                _ => None,
            })
            .collect();
        assert!(!flesh.is_empty(), "blood on the soldier");
        assert!(flesh.iter().all(|(head, _)| !head), "a body shot: {flesh:?}");
        assert!(flesh.iter().any(|(_, fatal)| *fatal), "one of them killed: {flesh:?}");
    }

    #[test]
    fn bullets_pass_through_friendlies() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(soldier(3, 0, 30.0));
        host.add(soldier(2, 1, 60.0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
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
    fn damage_flinches_the_view_and_toughness_cuts_it() {
        // G7: flinch = BO2 damage x bg_viewKickScale (0.175), x0.25 with Toughness.
        let flinch_for = |toughness: bool| {
            let host = FakeHost::default();
            host.add(general(1, 0));
            let mut sim = Sim::new();
            sim.take_control(&host, 0, 1);
            run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
            let p = sim.players[0].as_mut().unwrap();
            p.arsenal.loadout.perks.retain(|k| *k != crate::weapons::Perk::TOUGHNESS);
            if toughness {
                p.arsenal.loadout.perks.push(crate::weapons::Perk::TOUGHNESS);
            }
            // 60 Zero Hour points are 40 on BO2's scale.
            host.objects.borrow_mut()[0].health -= 40.0 * crate::units::BO2_TO_ZH_DAMAGE;
            run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
            events(&sim).into_iter().find_map(|k| if let EventKind::Flinch { pitch_deg } = k { Some(pitch_deg) } else { None }).expect("a flinch")
        };
        assert!((flinch_for(false) - 7.0).abs() < 1e-3);
        assert!((flinch_for(true) - 1.75).abs() < 1e-3);
    }

    #[test]
    fn rts_shots_use_the_generals_ammo_and_carry_into_first_person() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        // The General under RTS gets a record from the bridge's per-frame update; spawn unarmed,
        // so give him a class first (the test is about the class kit's ammo).
        sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None);
        sim.choose_class(0, 1, 0, &[]);
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
        sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None);
        sim.choose_class(0, 1, 0, &[]);
        let full = sim.rts_ammo(1);
        sim.rts_offhand(0, 1, Third::Tactical.wire());
        let r = sim.rts_update(&host, 0, 1, 2, [0, 0, 1], Some(Vec3::new(150.0, 40.0, 0.0)));
        assert_eq!((r.tactical, r.ammo[2], r.tactical_selected), (full.tactical - 1, full.ammo[2], 1), "the tactical, not the lethal");
        let (_, proj) = &sim.projectiles[0];
        assert_eq!(proj.def.class, crate::weapons::Class::Tactical);
        // The arc comes down by the target.
        let mut p = proj.pos;
        let mut v = proj.vel;
        while v.z > 0.0 || p.z > 0.0 {
            v.z -= crate::projectiles::GRAVITY * LOGIC_DT;
            p += v * LOGIC_DT;
        }
        assert!((Vec3::new(p.x, p.y, 0.0) - Vec3::new(150.0, 40.0, 0.0)).length() < 8.0, "lands at {p:?}");
    }

    #[test]
    fn returning_to_rts_locks_the_weapon_in_hand() {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        // The pistol is slot 1 of a class kit; the unarmed kit has only the knife.
        sim.choose_class(0, 1, 0, &[]);
        sim.players[0].as_mut().unwrap().arsenal.active = 1;
        run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::FIELD_COMPUTER, ..Default::default() }, 1);
        run(&mut sim, &host, 0, 1, FpsInput::default(), frames(FIELD_COMPUTER_RAISE));
        assert_eq!(sim.player(0).unwrap().mode, Mode::Rts);
        assert_eq!(sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None).lock, 1, "the pistol, which was in hand");
        assert_eq!(sim.rts_update(&host, 0, 1, 1, [0, 0, 0], None).lock, -1, "once");
    }

    /// BO2's blast is the impact table's for the def's `impactType` and the surface it went off on
    /// (`FUN_004c0ac0` -> `FUN_00533210`): the sim reports the surface in the event, the effect and
    /// the client's cues follow it.
    #[test]
    fn a_blast_on_two_surfaces_plays_their_own_effect_and_sound() {
        crate::impacts::install(
            "impact 77 default FX_D\nimpact 77 concrete FX_C\nsound 77 default SND_D\nsound 77 concrete SND_C\n",
        );
        let frag: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef {
            id: "test_frag_mp".into(),
            effect: crate::weapons::Effect::Explode,
            impact_type: 77,
            explode_sound: "OWN".into(),
            fuse_time: 0.1,
            ..Default::default()
        }));
        let blast_on = |ground: Surface| {
            let host = FakeHost::default();
            host.ground.set(Some(ground));
            host.add(general(1, 0));
            let mut sim = Sim::new();
            sim.projectiles.push((0, Projectile::launch(frag, 1, Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0), 0.1, &host, 0.0)));
            run(&mut sim, &host, 0, 1, FpsInput::default(), frames(0.5));
            let ev = events(&sim)
                .into_iter()
                .find_map(|k| match k {
                    EventKind::Exploded { sound, impact_type, surface, .. } => Some((sound, impact_type, surface)),
                    _ => None,
                })
                .expect("it went off");
            let fx = host.played_fx.borrow().clone();
            (ev, fx)
        };
        let ((sound, t, on_concrete), fx_concrete) = blast_on(Surface::Concrete);
        assert_eq!((sound, t, on_concrete), ("OWN", 77, Surface::Concrete));
        let ((_, _, on_dirt), fx_dirt) = blast_on(Surface::Dirt);
        assert_eq!(on_dirt, Surface::Dirt);
        // The table's cell, then the def's own: dirt has no cell of its own and takes the default.
        assert_eq!(fx_concrete, vec!["FX_C".to_string()]);
        assert_eq!(fx_dirt, vec!["FX_D".to_string()]);
        assert_eq!(crate::sounds::explosion_cues(sound, t, on_concrete), [Some("SND_C"), Some("OWN")]);
        assert_eq!(crate::sounds::explosion_cues(sound, t, on_dirt), [Some("SND_D"), Some("OWN")]);
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
