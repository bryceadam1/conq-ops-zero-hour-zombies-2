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
pub mod deploy;
mod fire;
mod hack;
pub mod optics;
mod plant;
mod rts;
#[cfg(test)]
mod hit_tests;

pub use build::{placement, HostWorld, PLANT_DISTANCE};
pub use plant::{can_plant, plant_launch, plant_point, plant_velocity, PLANT_TRACE_INCHES};
pub use fire::{gather_targets, throw_velocity, HitBodies};
pub use rts::{RtsIcons, RtsState, RtsWeapons, Third};

use crate::host::{damage_kind, geom, objflags, LogicHost, ObjInfo};
use crate::hitscan::{self, HitResult, Shape, Target, INFANTRY_HEAD_FRACTION, INFANTRY_HIT_RADIUS};
use crate::impacts::Surface;
use crate::input::{buttons, FpsInput};
use crate::math::{basis_xy, dir_from_angles, Vec3};
use crate::movement::{self, Blocker, Body, MoveModifiers, MoveTuning, World};
use crate::units::{BO2_TO_ZH_DAMAGE, LOGIC_DT};
use crate::movement::Stance;
use crate::equipment::{EquipEvent, Equipment, Kind as EquipKind, Placed, HACK_COS};
use crate::projectiles::{Outcome, Projectile};
use crate::units::inches;
use crate::building::{self, Hammer, Site};
use crate::weapons::idle_sway::{BreathIn, IdleSway};
use crate::weapons::view_kick::{self, ViewKick};
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
    /// `left`: the dual-wield left gun's shot (BO2's `EV_FIRE_WEAPON_LEFT`).
    Shot { origin: Vec3, end: Vec3, left: bool },
    /// One pellet landed on something (a miss emits nothing): where, the direction it was going,
    /// what it hit, and the shooting weapon's BO2 impact bucket. The client turns it into BO2's
    /// impact FX (through `ZhboClientOut.hits`) and the surface's impact sound. `exit`: the round
    /// left a solid it penetrated here (BO2's impact flag 4: the type's exit row and family).
    Impact { at: Vec3, dir: Vec3, surface: Surface, head: bool, fatal: bool, impact_type: i32, exit: bool },
    HitMarker { headshot: bool, kill: bool },
    /// A shot's gun kick spring impulse (`weapons::recoil::GunKickSpring`). The view kick is the
    /// sim's own (`PlayerSim::view_kick`): it moves the aim.
    Recoil { gun_kick: (f32, f32) },
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
    /// BO2's plant check refused the item (`WEAPON_CANT_PLANT_EQUIPMENT`, "Cannot Plant Here").
    CantPlant { lethal: bool },
    /// A launcher fired.
    Launched,
    /// A grenade, rocket, other projectile or piece of equipment went off on `surface` (what it hit
    /// directly, else the ground). `sound` is its def's own blast event (`projExplosionSound`, empty
    /// when it has none) and `impact_type` its `impactType`: the client plays the impact table's
    /// sound for the type and surface, then the def's own (`sounds::explosion_cues`).
    Exploded { at: Vec3, sound: &'static str, impact_type: i32, surface: crate::impacts::Surface },
    /// A grenade bounced off something: `sound` is its def's `bounceSound` cell for the surface
    /// (empty or `NONE`: silent), BO2's event 0x47.
    Bounced { at: Vec3, sound: &'static str },
    /// A projectile struck `surface` before arming and did not go off (BO2's event 0x51): the
    /// client plays the large bullet's hit sound for the surface, then `sound`, its def's own
    /// `projDudSound` (`sounds::dud_cues`).
    Dud { at: Vec3, sound: &'static str, surface: crate::impacts::Surface },
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
    /// The Assault Shield went down as cover (`deploy_riotshield`).
    ShieldDeployed { at: Vec3 },
    /// The player took their deployed shield back.
    ShieldPickedUp,
    /// This player's deployed shield was destroyed (by the player `by`, if one).
    ShieldDestroyed { by: Option<i32>, at: Vec3 },
    /// This player's round or strike met an Assault Shield (held, stowed or deployed) at `at`:
    /// no damage to its bearer; drawn as a metal impact, heard as the shield's.
    ShieldHit { at: Vec3, dir: Vec3, knife: bool, impact_type: i32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub player: i32,
    pub kind: EventKind,
}

/// A bullet that draws a tracer (BO2's countdown said so, `fire::tracer_due`). The sim names the
/// shooter and the weapon; which tracer is drawn is the viewer's (`FUN_004bbf10`: friendly or
/// enemy), so the host layer drains these after each logic step and the client chooses
/// (`client::tracers`). Presentation only: nothing in the sim reads them back.
#[derive(Clone, Copy, Debug)]
pub struct ShotTracer {
    /// The shooting General.
    pub shooter: u32,
    pub def: &'static crate::weapons::WeaponDef,
    /// What the bullet hit (0: the ground, the air or equipment) and where it ended.
    pub target: u32,
    pub end: Vec3,
    /// A dual-wield pair's left gun fired it: the own tracer starts at its `tag_flash1`.
    pub left: bool,
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
    /// The class's knife camo (BO2's `knifecamo`), carried when a `CustomClass` lands: what the
    /// melee weapon wears (`_class.gsc` gives `knife_mp`/`knife_held_mp` `calcweaponoptions(class, 2)`).
    pub knife_camo: String,
    /// The class's secondary camo (BO2's `secondarycamo`), carried when a `CustomClass` lands:
    /// what the sidearm wears (`_class.gsc` gives it `calcweaponoptions(class, 1)`).
    pub secondary_camo: String,
    pub mode: Mode,
    pub body: Body,
    /// The aim this frame: the input's view angles plus the view kick (BO2 adds the kick to every
    /// user command's angles, `FUN_00549fb0`) and the scope's idle sway (the server adds it to the
    /// fire angles, `FUN_005dc820`), radians.
    pub yaw: f32,
    pub pitch: f32,
    /// BO2's view kick (`weapons::view_kick`): shots set it, it recentres each frame.
    pub view_kick: ViewKick,
    /// BO2's idle sway and hold breath (`weapons::idle_sway`): the scope drifts, the aim with it.
    pub idle: IdleSway,
    /// The gun's pose as the server keeps it (`FUN_005dc820` runs `FUN_008f0700`): an aimed shot
    /// from a gun without an overlay scope flies along it (`weapons::gunpos`, the viewmodel's own
    /// math, stepped on the logic frame).
    pub gun: crate::viewmodel::Motion,
    /// The direction this frame's shots fly (radians): the aim, turned by the gun's pose while
    /// aimed through iron sights.
    pub fire_yaw: f32,
    pub fire_pitch: f32,
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
    /// The optic displays' state (the MMS's characters, the target finder's spotting).
    pub optics: optics::Optics,
    /// BO2's per-shooter tracer countdown (`centity` byte `+0x35c`, `fire::tracer_due`).
    pub tracer_count: u8,
    /// A melee lunge at an enemy in progress (`fire::MeleeCharge`).
    pub charge: Option<fire::MeleeCharge>,
}

impl PlayerSim {
    /// What the sim adds to the mouse aim: the view kick and the scope's idle sway, radians (yaw
    /// left, pitch up). The camera draws the same offset.
    pub fn aim_offset(&self) -> (f32, f32) {
        let (ky, kp) = self.view_kick.aim_offset();
        let (iy, ip) = self.idle.aim_offset();
        (ky + iy, kp + ip)
    }

    /// The fire direction (`FUN_005dc820`): the aim, and while aimed (`posFrac != 0`) with a gun
    /// that has no overlay scope, the aim's axis turned by the gun pose's angles (the axes
    /// multiplied, so the shot leaves along the sights the player sees).
    pub fn fire_angles(&self) -> (f32, f32) {
        let def = self.arsenal.current().def;
        if !(def.class.is_gun() && self.arsenal.ads != 0.0 && !crate::weapons::idle_sway::sways_view(def)) {
            return (self.yaw, self.pitch);
        }
        let (gy, gp) = self.gun.fire_angles();
        let local = dir_from_angles(gy, gp);
        let f = dir_from_angles(self.yaw, self.pitch);
        let left = dir_from_angles(self.yaw + std::f32::consts::FRAC_PI_2, 0.0);
        let up = dir_from_angles(self.yaw, self.pitch + std::f32::consts::FRAC_PI_2);
        let w = f * local.x + left * local.y + up * local.z;
        (w.y.atan2(w.x), w.z.clamp(-1.0, 1.0).asin())
    }

    /// Each held weapon paired with the class camo it wears (`_class.gsc:585-682`): the primary
    /// `calcweaponoptions(class, 0)` (`primarycamo`), the sidearm `(class, 1)` (`secondarycamo`)
    /// and the melee weapon `(class, 2)` (`knifecamo`). The client hands these to the viewmodel
    /// draw (`nativevm::class_camos`).
    pub fn held_camos(&self) -> [(&str, &str); 3] {
        let kit = &self.arsenal.loadout;
        [
            (kit.primary.as_str(), self.camo.as_str()),
            (kit.secondary.as_str(), self.secondary_camo.as_str()),
            (kit.melee.as_str(), self.knife_camo.as_str()),
        ]
    }

    pub fn is_first_person(&self) -> bool {
        matches!(self.mode, Mode::Fps | Mode::FieldComputer { .. })
    }

    /// 0 stand, 1 crouch, 2 prone.
    pub fn stance_index(&self) -> i32 {
        match self.body.stance {
            movement::Stance::Stand => 0,
            movement::Stance::Crouch => 1,
            movement::Stance::Prone => 2,
        }
    }

    /// The body clip his movement state picks in the held weapon's animation set
    /// (`nativebody::state_of`); `engine_moving`: Zero Hour is walking him (RTS control).
    pub fn body_state(&self, engine_moving: bool, run_speed: f32) -> u16 {
        let a = &self.arsenal;
        crate::nativebody::state_of(self.stance_index(), self.body.horizontal_speed(), self.body.sprinting, a.ads, a.current().def.body_anims, engine_moving, run_speed)
    }

    #[cfg(test)]
    pub fn spawn_for_tests() -> PlayerSim {
        PlayerSim {
            general: 1,
            class: Some("assault".into()),
            reticle: String::new(),
            reticle_color: String::new(),
            camo: String::new(),
            knife_camo: String::new(),
            secondary_camo: String::new(),
            mode: Mode::Fps,
            body: Body::new(Vec3::ZERO, &MoveTuning::default()),
            yaw: 0.0,
            pitch: 0.0,
            view_kick: ViewKick::default(),
            idle: IdleSway::default(),
            gun: Default::default(),
            fire_yaw: 0.0,
            fire_pitch: 0.0,
            arsenal: Arsenal::with_loadout(crate::weapons::Loadout::test_kit()),
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
            optics: Default::default(),
            tracer_count: 0,
            charge: None,
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
    /// BO2's `bulletrange` (ZH units): how far a non-spread round is traced.
    pub bullet_range: f32,
    /// BO2's bullet penetration table and dvars (`crate::penetration`, from the tuning pack).
    pub penetration: crate::penetration::Penetration,
    /// The soldier's hit boxes per body state, when not the installed table's (the tests set
    /// their own).
    pub hit_body: Option<&'static crate::hitloc::Poses>,
    pub frame: u32,
    pub events: Vec<Event>,
    /// This step's tracer bullets, for the host layer to draw per viewer and clear.
    pub tracers: Vec<ShotTracer>,
    /// The Generals carrying an Assault Shield, as the frame began (held or stowed; see
    /// [`crate::shield`]): the shots, melee strikes and blasts of this frame test them.
    pub shields: Vec<crate::shield::Bearer>,
    /// Deployed Assault Shields (`sim/deploy.rs`).
    pub deployed: Vec<deploy::Deployed>,
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
            bullet_range: crate::units::inches(crate::tuning::Tuning::default().bulletrange),
            penetration: crate::penetration::Penetration::default(),
            hit_body: None,
            frame: 0,
            events: Vec::new(),
            tracers: Vec::new(),
            shields: Vec::new(),
            deployed: Vec::new(),
        }
    }

    /// Every General's shield this frame: held while it is the weapon up, stowed on the back while
    /// it is carried otherwise (`_riotshield.gsc` `refreshshieldattachment`).
    /// A shield standing deployed is on neither (the deployed ones follow, `general` 0).
    fn shield_bearers(&self) -> Vec<crate::shield::Bearer> {
        let mut out = Vec::new();
        for p in self.players.iter().flatten() {
            let up = p.arsenal.current().def;
            // A deployed shield's weapon is out of the arsenal (`Arsenal::take_shield`).
            let stowed = || p.arsenal.weapons.iter().map(|w| w.def).find(|d| d.is_shield()).map(|d| (d, false));
            let carried = if up.is_shield() { Some((up, true)) } else { stowed() };
            if let Some(b) = carried.and_then(|(d, held)| crate::shield::Bearer::of(p.general, p.body.pos, p.yaw, d, held)) {
                out.push(b);
            }
        }
        out.extend(self.deployed_bearers());
        out
    }

    /// A sim configured from the gameplay tuning pack (`data/ConquerOps/tuning.ini`, loaded by the
    /// bridge). The tests use `Sim::new`, whose defaults are the pack's own values.
    pub fn with_tuning(t: &crate::tuning::Tuning) -> Self {
        let mut sim = Self::new();
        sim.tuning = MoveTuning::from_tuning(t);
        sim.bullet_range = crate::units::inches(t.bulletrange);
        sim.penetration = crate::penetration::Penetration::from_tuning(t);
        sim.equipment.tuning = crate::equipment::EquipTuning::from_tuning(t);
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
                p.view_kick = ViewKick::default();
                p.idle = IdleSway::default();
                p.gun = Default::default();
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
                    knife_camo: String::new(),
                    secondary_camo: String::new(),
                    mode: Mode::Fps,
                    body: Body::new(pos, &tuning),
                    yaw,
                    pitch: 0.0,
                    view_kick: ViewKick::default(),
                    idle: IdleSway::default(),
                    gun: Default::default(),
                    fire_yaw: 0.0,
                    fire_pitch: 0.0,
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
            optics: Default::default(),
            tracer_count: 0,
            charge: None,
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
        state.arsenal.alt_pending = false;
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
                    knife_camo: String::new(),
                    secondary_camo: String::new(),
                    mode: Mode::Rts,
                    body: Body::new(Vec3::ZERO, &tuning),
                    yaw: 0.0,
                    pitch: 0.0,
                    view_kick: ViewKick::default(),
                    idle: IdleSway::default(),
                    gun: Default::default(),
                    fire_yaw: 0.0,
                    fire_pitch: 0.0,
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
                    optics: Default::default(),
                    tracer_count: 0,
                    charge: None,
                }
            }
        };
        if p.general != general {
            self.players[idx] = Some(p);
            return;
        }
        let (class_id, class_name, loadout, reticle, reticle_color, camo, knife_camo, secondary_camo) = if payload.is_empty() {
            let Some(c) = crate::classes::at(index) else { self.players[idx] = Some(p); return };
            (c.id.clone(), ClassName::new(&c.name), crate::classes::loadout_of(&c), String::new(), String::new(), String::new(), String::new(), String::new())
        } else {
            let Some(c) = crate::classes::CustomClass::from_wire_words(payload) else {
                self.players[idx] = Some(p);
                return;
            };
            let name = if c.name.is_empty() { "Custom Class".to_string() } else { c.name.clone() };
            (format!("custom:{}", c.name), ClassName::new(&name), c.loadout(), c.reticle.clone(), c.reticle_color.clone(), c.camo.clone(), c.knife.clone(), c.secondary_camo.clone())
        };
        p.class = Some(class_id);
        p.reticle = reticle;
        p.reticle_color = reticle_color;
        p.camo = camo;
        p.knife_camo = knife_camo;
        p.secondary_camo = secondary_camo;
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
            p.arsenal.alt_pending = false;
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
        self.shields = self.shield_bearers();
        self.equipment.shields.clone_from(&self.shields);
        for idx in 0..self.players.len() {
            let Some(mut p) = self.players[idx].take() else { continue };
            let keep = self.step_player(host, idx, &mut p);
            if keep {
                self.players[idx] = Some(p);
            }
        }
        self.step_projectiles(host);
        self.step_equipment(host);
        self.step_deployed(host);
        for s in &mut self.smokes {
            host.smoke(s.0, s.1);
            s.2 -= LOGIC_DT;
        }
        self.smokes.retain(|s| s.2 > 0.0);
    }

    fn step_projectiles(&mut self, host: &dyn LogicHost) {
        let mut done = Vec::new();
        let mut bounces = Vec::new();
        for (i, (owner, proj)) in self.projectiles.iter_mut().enumerate() {
            let outcome = proj.step_shielded(host, &self.shields);
            if let Some((at, surface)) = proj.bounced {
                bounces.push((*owner, at, proj.def.bounce_sound(surface)));
            }
            if let Some(outcome) = outcome {
                done.push((i, *owner, outcome));
            }
        }
        for (owner, at, sound) in bounces {
            self.emit(owner, EventKind::Bounced { at, sound });
        }
        for (i, owner, outcome) in done.iter().rev() {
            let (_, proj) = self.projectiles.remove(*i);
            match *outcome {
                Outcome::Exploded { at, hit_enemy, kill, surface } => {
                    let d = proj.def;
                    self.emit(*owner, EventKind::Exploded { at, sound: d.explode_sound.as_str(), impact_type: d.impact_type, surface });
                    self.deployed_caught_in_blast(host, *owner, d, at);
                    if proj.def.effect == crate::weapons::Effect::Smoke {
                        let radius = proj.def.explosion_radius.max(crate::projectiles::SMOKE_RADIUS);
                        self.smokes.push((at, radius, crate::projectiles::SMOKE_TIME));
                    }
                    let mut ev = Vec::new();
                    self.equipment.caught_in_blast(host, *owner, proj.owner, at, proj.def.explosion_radius, &mut ev);
                    self.equipment_events(host, ev);
                    if hit_enemy {
                        self.emit(*owner, EventKind::HitMarker { headshot: false, kill });
                    }
                }
                Outcome::Dud { at, hit_enemy, kill, surface } => {
                    self.emit(*owner, EventKind::Dud { at, sound: proj.def.dud_sound.as_str(), surface });
                    if hit_enemy {
                        self.emit(*owner, EventKind::HitMarker { headshot: false, kill });
                    }
                    if kill {
                        self.scavenge(*owner);
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
        let engineer = |id: u32| players.iter().flatten().any(|p| p.general == id && p.arsenal.class_specialties().has(Specialty::DelayExplosive));
        let mut ev = Vec::new();
        self.equipment.step(host, &mut self.projectiles, &engineer, &mut ev);
        self.equipment_events(host, ev);
    }

    fn equipment_events(&mut self, host: &dyn LogicHost, events: Vec<EquipEvent>) {
        for e in events {
            match e {
                EquipEvent::Exploded { owner, at, hit_enemy, kill, def, sound, impact_type, surface } => {
                    self.emit(owner, EventKind::Exploded { at, sound, impact_type, surface });
                    self.deployed_caught_in_blast(host, owner, def, at);
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
        // The aim is the view plus the kick the last frame left (`FUN_00549fb0`).
        let (off_yaw, off_pitch) = p.aim_offset();
        p.yaw = crate::math::wrap_angle(input.yaw + off_yaw);
        p.pitch = (input.pitch + off_pitch).clamp(-crate::input::MAX_PITCH, crate::input::MAX_PITCH);
        (p.fire_yaw, p.fire_pitch) = p.fire_angles();

        let world = HostWorld { host, viewer: p.general };
        match p.mode {
            Mode::Fps => {
                // The use key works (builds or repairs) at a structure in reach, held, instead of
                // opening the field computer.
                p.site = host.work_site(p.general, p.body.pos, building::REACH);
                p.building = p.site.is_some() && (input.held | input.pressed) & buttons::FIELD_COMPUTER != 0;
                // The use key next to the player's own deployed shield takes it back.
                let picked_up = p.site.is_none() && input.pressed & buttons::FIELD_COMPUTER != 0 && self.use_picks_up_shield(host, idx, p);
                if picked_up {
                    input.pressed &= !buttons::FIELD_COMPUTER;
                }
                if p.site.is_none() && input.pressed & buttons::FIELD_COMPUTER != 0 {
                    p.mode = Mode::FieldComputer { t: 0.0 };
                    p.arsenal.ads = 0.0;
                    // Out of first person BO2 zeroes the kick (`FUN_00523070`).
                    p.view_kick = ViewKick::default();
                    p.idle = IdleSway::default();
                    p.gun = Default::default();
                    self.emit(idx, EventKind::FieldComputerOpened);
                } else {
                    let mods = MoveModifiers {
                        speed_scale: p.arsenal.move_scale(),
                        block_sprint: p.arsenal.ads > 0.0 || p.arsenal.offhand.is_some() || p.arsenal.melee_left > 0.0 || p.building,
                        sprint_time_scale: p.arsenal.sprint_scale(),
                        move_faster: p.arsenal.move_faster(),
                        charge: self.melee_charge(host, p),
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
                    // BO2's plant check, from the world, for the arsenal's plantable items.
                    p.arsenal.plant_ok = [true, false].map(|lethal| can_plant(p.arsenal.offhand_def(lethal), &p.body, p.yaw, p.pitch, p.general, host));
                    let prev_ads = p.arsenal.ads;
                    p.arsenal.step(held, pressed, p.body.sprinting, stance, speed, &mut p.rng, &mut actions);
                    // Hold breath and the idle clock run after the ADS update, before the shot
                    // (`FUN_008e4da0`: `FUN_008e4470`, …, `FUN_008e4860`, `FUN_008e3940`); the
                    // breath key is `+breath_sprint`, the sprint key.
                    let breath = BreathIn { key: held & buttons::SPRINT != 0, extra: p.arsenal.has_specialty(Specialty::HoldBreath), aqs_block: false };
                    p.idle.step(p.arsenal.current().def, p.arsenal.ads, prev_ads, stance, breath, &p.arsenal.tuning, LOGIC_DT);
                    let shot = actions.iter().find_map(|a| match a {
                        WeaponAction::Fire { gun_kick, pellet: 0, .. } => Some(*gun_kick),
                        _ => None,
                    });
                    for a in actions {
                        self.apply_weapon_action(host, idx, p, a);
                    }
                    // The kick recentres at the weapon in hand's speed (`FUN_0051f150`; a dual-wield
                    // pair goes through `centre_speed_hands` with its hands' firing states: a hand
                    // is firing while its fire cycle runs).
                    let w = p.arsenal.current();
                    let hands = match &w.left {
                        Some(left) => crate::weapons::recoil::Hands::Dual { left: w.def, right_firing: w.cooldown > 0.0, left_firing: left.cooldown > 0.0 },
                        None => crate::weapons::recoil::Hands::Single,
                    };
                    let centre = view_kick::centre_speed_hands(w.def, hands, p.arsenal.ads, stance);
                    p.view_kick.step(LOGIC_DT, centre);
                    // The gun's pose after the frame (the next frame's shots follow it).
                    let mut gun = std::mem::take(&mut p.gun);
                    gun.update(LOGIC_DT, p.yaw, p.pitch, p.arsenal.tuning.cg_fov, p, shot);
                    p.gun = gun;
                    self.step_hack(host, idx, p);
                    let dir = dir_from_angles(p.yaw, p.pitch);
                    p.optics.step(host, p.general, p.body.eye(), dir, p.arsenal.ads, p.arsenal.current().def, self.frame);
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
    /// weapon in hand whose W3D world meshes are shown (`Catalog::world_viewmodel`: a combination
    /// shows its base weapon's), the tablet (0 none, 1 coming up for the field
    /// computer, 2 going down as first person starts), and the held weapon's body animation set
    /// (`WeaponDef::body_anims`: the W3D body's model condition, `names::ANIM_SET_CONDITIONS`).
    pub fn world_poses(&self) -> Vec<(u32, i32, i32, u8)> {
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
                (p.general, crate::weapons::catalog().world_viewmodel(a.current().def).max(0), tablet, a.current().def.body_anims)
            })
            .collect()
    }

    /// The class camo option General `general`'s weapon `vm` wears in third person
    /// ([`crate::nativebody::camo_for`]).
    pub fn third_person_camo(&self, general: u32, vm: i32) -> Option<String> {
        let p = self.players.iter().flatten().find(|p| p.general == general)?;
        let primary = crate::weapons::catalog().get(&p.arsenal.loadout.primary)?;
        crate::nativebody::camo_for(primary, &p.camo, vm)
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
                let engine_moving = moving.is_some_and(|(g, m)| g == p.general && m) && !p.is_first_person();
                NativePose {
                    general: p.general,
                    weapon: a.current().def.viewmodel,
                    stance: p.stance_index(),
                    state: p.body_state(engine_moving, run_speed),
                    time,
                    rate: 1.0,
                    sidearm: a.active == 1,
                }
            })
            .collect()
    }

    /// Each General's weapon carried but not up that the third-person body wears stowed: the
    /// Assault Shield on his back (its converted index), while another weapon is up.
    pub fn stowed_weapons(&self) -> Vec<(u32, i32)> {
        self.players
            .iter()
            .flatten()
            .filter(|p| !p.arsenal.current().def.is_shield())
            .filter_map(|p| p.arsenal.weapons.iter().map(|w| w.def).find(|d| d.is_shield() && d.viewmodel >= 0).map(|d| (p.general, d.viewmodel)))
            .collect()
    }

    /// Every General's body state now (`nativebody::state_of`, from the sim's own stance, gait,
    /// sprint and ADS — never the render clock): the pose his hit boxes take in a trace.
    pub(super) fn hit_states(&self) -> Vec<(u32, u16)> {
        let run_speed = self.tuning.run_speed(false);
        self.players.iter().flatten().map(|p| (p.general, p.body_state(false, run_speed))).collect()
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

/// The BO2 hint an event shows (its localize key), if it shows one: the client draws it and the
/// log reports it.
pub fn event_hint(kind: &EventKind) -> Option<&'static str> {
    match kind {
        EventKind::CantPlant { .. } => Some("WEAPON_CANT_PLANT_EQUIPMENT"),
        _ => None,
    }
}

/// A hint's text: BO2's localized string from the converted table, else the key itself.
pub fn hint_text(key: &str) -> &str {
    crate::weapons::catalog().ui_text(key).unwrap_or(key)
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
        CantPlant { .. } => format!("player {} was told \"{}\"", e.player, event_hint(&e.kind).map_or("", hint_text)),
        Dud { at, .. } => format!("player {}'s projectile struck unarmed (a dud) at ({:.0}, {:.0})", e.player, at.x, at.y),
        Planted { kind } => format!("player {} put down equipment ({kind:?})", e.player),
        Detonated => format!("player {} set off their C4", e.player),
        Intercepted { .. } => format!("player {}'s Trophy System shot down a projectile", e.player),
        DestroyedEquipment => format!("player {} destroyed enemy equipment", e.player),
        Hacked => format!("player {} hacked with the Black Hat", e.player),
        AltToggled => format!("player {} toggled the weapon's alt mode", e.player),
        ShieldDeployed { at } => format!("player {} deployed the Assault Shield at ({:.0}, {:.0})", e.player, at.x, at.y),
        ShieldPickedUp => format!("player {} picked the Assault Shield back up", e.player),
        ShieldDestroyed { .. } => format!("player {}'s deployed Assault Shield was destroyed", e.player),
        _ => return None,
    };
    Some(msg)
}

#[cfg(test)]
mod shield_tests;

#[cfg(test)]
pub(crate) mod tests;
