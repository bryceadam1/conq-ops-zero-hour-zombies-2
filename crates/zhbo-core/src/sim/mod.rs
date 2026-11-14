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

pub mod antilag;
mod build;
pub mod deploy;
pub mod drops;
pub mod scavenger;
pub mod guard;
mod fire;
mod hack;
pub mod shock;
mod throwback;
pub use shock::FLASH_WEAPON;
pub mod optics;
mod plant;
mod rts;
pub mod score;
mod spawn;
pub mod zombies;
pub mod laststand;
pub mod wallbuy;
pub mod powerups;
pub mod magicbox;
mod lockstep;
/// The fake engine world and the lockstep match harness: the sim's own tests, and behind the
/// `harness` feature the full-roster gate `zhbo-assets` runs over a real install.
#[cfg(any(test, feature = "harness"))]
pub mod fakehost;
#[cfg(any(test, feature = "harness"))]
pub mod harness;
pub mod predict;
#[cfg(test)]
mod death_tests;
#[cfg(test)]
mod hit_tests;
#[cfg(test)]
mod lockstep_tests;
#[cfg(test)]
mod latency_tests;
#[cfg(test)]
mod spread_tests;

pub use build::{placement, HostWorld, PLANT_DISTANCE};
pub use plant::{can_plant, plant_launch, plant_point, plant_velocity, PLANT_TRACE_INCHES};
pub use fire::{gather_targets, throw_velocity, HitBodies, MeleeCharge};
pub use rts::{RtsIcons, RtsState, RtsWeapons, Third};
pub use spawn::ChosenClass;
pub use lockstep::Fingerprint;

use crate::host::{geom, objflags, LogicHost, ObjInfo};
use crate::hitscan::{self, HitResult, Shape, Target, INFANTRY_HIT_RADIUS};
use crate::impacts::Surface;
use crate::input::{buttons, FpsInput};
use crate::math::{basis_xy, dir_from_angles, Vec3};
use crate::movement::{self, Blocker, Body, MoveModifiers, MoveTuning, World};
use crate::units::{BO2_TO_ZH_DAMAGE, LOGIC_DT};
use crate::movement::Stance;
use crate::equipment::{EquipEvent, Equipment, Kind as EquipKind, Placed};
use crate::projectiles::{Outcome, Projectile};
use crate::units::inches;
use crate::building::{self, Hammer, Site};
use crate::weapons::idle_sway::{BreathIn, IdleSway};
use crate::weapons::view_kick::{self, ViewKick};
use crate::weapons::{Arsenal, Rng, Specialty, WeaponAction, MELEE_FX_WEAPON, MELEE_RANGE};
#[cfg(test)]
use crate::weapons::Class;

pub const MAX_PLAYERS: usize = 16;
// `Sim::combat_seen` keeps a bit per seat.
const _: () = assert!(MAX_PLAYERS <= u32::BITS as usize);

/// How long the field computer takes to come up before control returns to the RTS view: BO2's
/// scorestreak tablet pullout, then a moment on its screen.
pub const FIELD_COMPUTER_RAISE: f32 = crate::viewmodel::TABLET_PULLOUT_TIME + 0.35;
/// Taking control, the weapon is ready after the camera flies in, the tablet goes down and the
/// weapon comes up.
pub const TAKE_CONTROL_READY: f32 = crate::client::ENTER_TIME + crate::viewmodel::TABLET_PUTAWAY_TIME;
/// BO2 health regeneration: wait this long after the last hit, then regenerate. The values come
/// from the tuning pack (`data/ConquerOps/tuning.ini`); this alias is for the sim tests.
pub fn regen_delay() -> f32 {
    crate::tuning::get().regen_delay
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

crate::statehash::state_hash!(enum Mode { Fps, FieldComputer { t }, Rts });

/// A class name as the log carries it: a fixed inline buffer so [`EventKind`] stays `Copy` (the
/// events are copied out of the sim's ring, and a shipped class's name and a custom one's both fit
/// in [`NAME_MAX`](crate::classmenu::NAME_MAX) bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassName {
    bytes: [u8; crate::classmenu::NAME_MAX],
    len: u8,
}

crate::statehash::state_hash!(struct ClassName { bytes, len });

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
    /// A class picked mid-life after the grace period or his first combat: it is the player's
    /// chosen class now, the kit stays, and his next General's class screen opens on it (BO2's
    /// `MP_CHANGE_CLASS_NEXT_SPAWN`; every General is picked for, `spawn.rs`).
    ClassAtNextSpawn { general: u32, class: ClassName },
    FieldComputerOpened,
    /// Zombies: round `round` began (BO2's round chalk counts up; `sim/zombies.rs`).
    ZombieRound { round: u8 },
    /// Zombies: round `round`'s last zombie died; the wait before the next began.
    ZombieRoundOver { round: u8 },
    /// Zombies: a hit or kill on a zombie paid this seat `points` (`player_add_points`).
    ZombiePoints { points: u16 },
    /// Zombies: a survivor took power-up `kind` (its row in `zombies::PowerUps`; to every survivor).
    ZombiePowerUp { kind: u8 },
    /// Zombies: this seat bought from a wall for `cost` points (`sim/wallbuy.rs`).
    ZombiePurchase { cost: u16 },
    /// Zombies: survivor `general` went down (last stand, `sim/laststand.rs`), losing `points`.
    Downed { general: u32, points: u16 },
    /// Zombies: survivor `general` was revived by General `by` (to both seats).
    Revived { general: u32, by: u32 },
    /// Zombies: survivor `general` bled out.
    BledOut { general: u32 },
    ReturnedToRts { general: u32 },
    /// `left`: the dual-wield left gun's shot (BO2's `EV_FIRE_WEAPON_LEFT`). `spread`: the cone it
    /// flew in, degrees (`weapons::aim_spread`).
    Shot { origin: Vec3, end: Vec3, left: bool, spread: f32 },
    /// One pellet landed on something (a miss emits nothing): where, the direction it was going,
    /// what it hit, and the shooting weapon's BO2 impact bucket. The client turns it into BO2's
    /// impact FX (through `ZhboClientOut.hits`) and the surface's impact sound. `exit`: the round
    /// left a solid it penetrated here (BO2's impact flag 4: the type's exit row and family).
    Impact { at: Vec3, dir: Vec3, surface: Surface, head: bool, fatal: bool, impact_type: i32, exit: bool },
    /// A hit on an enemy: the hit marker, and `alert` the sound BO2 plays for it
    /// ([`crate::damage::HitAlert`], from the means and the inflictor).
    HitMarker { headshot: bool, kill: bool, alert: crate::damage::HitAlert },
    /// The last hit's victim wore a perk that took it: the hit marker's variant
    /// ([`crate::damage::PerkFeedback`]), after the hit's own `HitMarker` this step.
    PerkFeedback { perk: crate::damage::PerkFeedback },
    /// A shot's gun kick spring impulse (`weapons::recoil::GunKickSpring`). The view kick is the
    /// sim's own (`PlayerSim::view_kick`): it moves the aim.
    Recoil { gun_kick: (f32, f32) },
    /// Damage flinch (G7): the view kicks this far (degrees, yaw counter-clockwise and pitch up)
    /// by the side the blow came from (`weapons::recoil::flinch_kick`), and settles over
    /// `FlinchTime`.
    Flinch { yaw_deg: f32, pitch_deg: f32 },
    /// Hurt by `amount`; `dir` the last blow's way (BO2's `vDir`, zero when no blow is known): the
    /// third-person flinch's `dmgDirection`.
    Damaged { amount: f32, dir: Vec3 },
    ReloadStarted,
    Swapped,
    GeneralDied { general: u32 },
    /// Melee swing (hit or miss).
    Melee,
    /// The melee landed on someone (BO2's `meleeHitSound` moment).
    MeleeHit,
    /// Another player killed a General this player had hurt: BO2's assist (`score`), `level` 1, 2
    /// or 3 for `assist_25`, `assist_50`, `assist_75`, or the tactical his last hit was
    /// (`assist_concussion`, ...).
    Assist { level: u8, tactical: Option<score::Tactical> },
    /// This player's General was killed by a seat's damage: BO2's `dmgType` of the killing blow
    /// (its means of death and weapon, [`crate::death::DmgType::of`]) and the way it travelled
    /// (BO2's `vDir`; zero: none). Presentation: the death animation's `dmgType` and
    /// `dmgDirection` (`crate::death`). A death to anything else (Zero Hour's own weapons) has none.
    /// `by` is the killer's seat (the victim's own for a suicide), `how` and `weapon` the killing
    /// blow's means and weapon as BO2's `obituary` takes them (the kill feed, `hud::obituary`).
    KilledBy { dmg: crate::death::DmgType, dir: Vec3, by: Option<u8>, how: crate::damage::MeansOfDeath, weapon: Option<&'static crate::weapons::WeaponDef> },
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
    /// His General died and dropped this weapon (`dropweaponfordeath`), from `at`.
    WeaponDropped { def: &'static crate::weapons::WeaponDef, at: Vec3 },
    /// He picked a dropped weapon up with the use key (BO2's EV_ITEM_PICKUP): the def's
    /// `pickupSoundPlayer`, `pickupSound` for everyone else at `at`.
    WeaponPickedUp { def: &'static crate::weapons::WeaponDef, at: Vec3 },
    /// He walked over a dropped weapon and took its ammo (EV_AMMO_PICKUP): `ammoPickupSoundPlayer`,
    /// `ammoPickupSound` for everyone else.
    AmmoPickedUp { def: &'static crate::weapons::WeaponDef, at: Vec3 },
    /// He walked over a Scavenger pack and took it (`scavenger_think`): `fly_equipment_pickup_plr`
    /// and the `hud_scavenger_pickup` icon for him, `fly_equipment_pickup_npc` at `at` for others.
    ScavengerPickedUp { at: Vec3 },
    /// This player's round or strike met an Assault Shield (held, stowed or deployed) at `at`:
    /// no damage to its bearer; drawn as a metal impact, heard as the shield's.
    ShieldHit { at: Vec3, dir: Vec3, knife: bool, impact_type: i32 },
}

/// A blow a seat's General took ([`Sim::blows`]): BO2's `vDir` and `dmgType` of it, and whether
/// Zero Hour's own weapons dealt it (the engine's damage hook) rather than the sim.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Struck {
    pub victim: u32,
    pub dir: Vec3,
    pub dmg: crate::death::DmgType,
    /// BO2's means of death of it (the kill feed's icon for a death to Zero Hour's weapons).
    pub how: crate::damage::MeansOfDeath,
    pub engine: bool,
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
    /// The class's secondary reticle (BO2's `secondaryreticle`), carried when a `CustomClass`
    /// lands: the sidearm's optic lens (a pistol's Reflex) wears it.
    pub secondary_reticle: String,
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
    /// The logic frame the sender's machine showed when it sent [`PlayerSim::input`] (its
    /// `Sim::frame` then; 0: not reported). It travels in the input message, so every peer reads the
    /// same: the latest one's shots are resolved against the world of that frame ([`antilag`]), and
    /// the sender's client knows which of its inputs this record already holds (its prediction).
    pub seen_frame: u32,
    pressed_accum: u32,
    pub health: f32,
    pub max_health: f32,
    pub regen_wait: f32,
    /// BO2's `veryhurt`: his health fell to `healthoverlaycutoff` or under since he was last whole,
    /// so it comes back a share at a time instead of at once (`_healthoverlay.gsc`).
    pub very_hurt: bool,
    /// Black Hat: what it is aimed at (an object id) and how far along the hack is (seconds).
    pub hack: Option<(u32, f32)>,
    /// Under RTS control: the weapon lock, what the third slot does and the gun to go back to
    /// (`rts.rs`).
    pub rts: RtsState,
    /// A survivor General (`objflags::SURVIVOR`): BO2 Zombies' player, who spawns with Zombies'
    /// kit instead of a class ([`crate::zombies::survivor_kit`]) and wears the Zombies HUD.
    pub survivor: bool,
    /// BO2's action slots: what each holds (`weapons::actionslot`; the build tool's).
    pub action_slots: crate::weapons::actionslot::ActionSlots,
    /// The build tool (the hammer), when it is out.
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
    /// The live grenade in his use list (its Zero Hour object; 0: none): BO2's cursor-hint
    /// entity, which a throwback keeps while it stays within the outer radius (`throwback`).
    pub use_grenade: u32,
    /// The shellshock running on him in first person ([`crate::shellshock`]).
    pub shock: crate::shellshock::Shellshock,
    /// A Shock Charge's zap on him: its damage ticks and the hidden HUD ([`shock::Taser`]).
    pub taser: Option<shock::Taser>,
    /// BO2's `hasdonecombat` for this life: he has fired, thrown, launched or been hurt since he
    /// spawned (`spawn::is_combat`). A class change then waits for the next spawn.
    pub combat: bool,
    /// He has spawned: been in first person with a class (`Sim::spawn_in`; the Tactical Insertion
    /// is used then).
    pub spawned: bool,
}

crate::statehash::state_hash!(struct PlayerSim {
    general, class, reticle, reticle_color, camo, knife_camo, secondary_camo, secondary_reticle, mode, body, yaw, pitch,
    view_kick, idle, gun, fire_yaw, fire_pitch, arsenal, rng, input, seen_frame, pressed_accum, health, max_health, regen_wait, very_hurt, hack,
    rts, survivor, action_slots, hammer, site, building, optics, tracer_count, charge, use_grenade, shock, taser, combat, spawned,
});

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
        (crate::math::det::atan2(w.y, w.x), crate::math::det::asin(w.z.clamp(-1.0, 1.0)))
    }

    /// Each held weapon paired with the camo it wears: a picked-up weapon its own (the options it
    /// was dropped with, `sim::drops`), listed first; then the class's (`_class.gsc:585-682`): the
    /// primary `calcweaponoptions(class, 0)` (`primarycamo`), the sidearm `(class, 1)`
    /// (`secondarycamo`) and the melee weapon `(class, 2)` (`knifecamo`). The client hands these to
    /// the viewmodel draw (`nativevm::class_camos`), which takes the first pair for a weapon.
    pub fn held_camos(&self) -> Vec<(&str, &str)> {
        let kit = &self.arsenal.loadout;
        let picked = self.arsenal.weapons.iter().filter_map(|w| w.options.as_ref().map(|o| (w.main.id.as_str(), o.camo.as_str())));
        picked
            .chain([
                (kit.primary.as_str(), self.camo.as_str()),
                (kit.secondary.as_str(), self.secondary_camo.as_str()),
                (kit.melee.as_str(), self.knife_camo.as_str()),
            ])
            .collect()
    }

    /// Each weapon paired with the reticle its optic wears: a picked-up weapon's own first, then
    /// the primary's `primaryreticle` and the sidearm's `secondaryreticle`
    /// (`calcweaponoptions(class, 0/1)`, the same per-slot options as the camos). The client hands
    /// these to the viewmodel draw (`nativevm::class_reticles`).
    pub fn held_reticles(&self) -> Vec<(&str, &str)> {
        let kit = &self.arsenal.loadout;
        let picked = self.arsenal.weapons.iter().filter_map(|w| w.options.as_ref().map(|o| (w.main.id.as_str(), o.reticle.as_str())));
        picked.chain([(kit.primary.as_str(), self.reticle.as_str()), (kit.secondary.as_str(), self.secondary_reticle.as_str())]).collect()
    }

    /// The reticle of the weapon in hand: a picked-up weapon's own; else the sidearm's (or its
    /// alt mode's) when the sidearm is up, the primary's otherwise.
    pub fn held_reticle(&self) -> &str {
        if let Some(o) = &self.arsenal.current().options {
            return &o.reticle;
        }
        let current = &self.arsenal.current().def;
        let kit = &self.arsenal.loadout;
        let secondary = !kit.secondary.is_empty()
            && (current.id == kit.secondary || crate::weapons::catalog().get(&kit.secondary).is_some_and(|d| !d.alt.is_empty() && d.alt == current.id));
        if secondary {
            &self.secondary_reticle
        } else {
            &self.reticle
        }
    }

    /// The reticle colour of the weapon in hand: a picked-up weapon's own, else the class's.
    pub fn held_reticle_color(&self) -> &str {
        self.arsenal.current().options.as_ref().map_or(self.reticle_color.as_str(), |o| o.reticle_color.as_str())
    }

    pub fn is_first_person(&self) -> bool {
        matches!(self.mode, Mode::Fps | Mode::FieldComputer { .. })
    }

    /// Taken into first person but not spawned: no class yet. BO2 never lets a player play without
    /// a class: `_globallogic_ui.gsc`'s `menuclass` is the only path from the class menu to
    /// `spawnclient`, and its `back` response (`_menus.gsc` `onmenuresponse`) only closes the menu,
    /// leaving the player unspawned. Here the General stands behind the class screen: the sim
    /// takes none of his first-person input except the field-computer key, which declines the spawn
    /// and hands him straight back overhead (the client sends it for Escape on the screen).
    pub fn awaiting_class(&self) -> bool {
        matches!(self.mode, Mode::Fps) && self.class.is_none()
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
    /// (`nativebody::state_of`). In first person the sim's own stance, gait, sprint, ADS and the
    /// direction of his movement keys (`death::move_direction`) pick it. Under RTS control Zero
    /// Hour moves him and the sim does not step his body: he stands (the stance, aim and sprint
    /// his body kept from his last first person are not his overhead pose; Zero Hour has none of
    /// them), walking or running forward at `engine`'s speed when Zero Hour says he moves (its
    /// units turn to face their path), idle otherwise.
    pub fn body_state(&self, engine: Option<EngineMotion>) -> u16 {
        let a = &self.arsenal;
        let t = &a.tuning;
        let set = a.current().def.body_anims;
        if !self.is_first_person() {
            let m = engine.unwrap_or_default();
            return crate::nativebody::state_of(0, m.speed, false, 0.0, set, m.moving, 1, t);
        }
        let (forward, right) = self.input.move_axes();
        let direction = crate::death::move_direction(forward, right, t);
        crate::nativebody::state_of(self.stance_index(), self.body.horizontal_speed(), self.body.sprinting, a.ads, set, false, direction, t)
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
            secondary_reticle: String::new(),
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
            seen_frame: 0,
            pressed_accum: 0,
            health: 500.0,
            max_health: 500.0,
            regen_wait: 0.0,
            very_hurt: false,
            hack: None,
            rts: RtsState::default(),
            survivor: false,
            action_slots: crate::weapons::actionslot::ActionSlots::GENERAL,
            hammer: None,
            site: None,
            building: false,
            optics: Default::default(),
            tracer_count: 0,
            charge: None,
            use_grenade: 0,
            shock: Default::default(),
            taser: None,
            combat: false,
            spawned: true,
        }
    }
}

/// How Zero Hour moves a General under RTS control (the bridge's answer, `ZhboGeneralMotion`): the
/// drawable's `MOVING` condition and his horizontal speed (world units/s). Presentation only: it
/// picks his drawn body clip ([`PlayerSim::body_state`]) and nothing in the sim reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EngineMotion {
    pub general: u32,
    pub moving: bool,
    pub speed: f32,
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

// Tests copy a whole sim to change one field of it (`lockstep_tests.rs`).
#[cfg_attr(test, derive(Clone))]
pub struct Sim {
    pub players: Vec<Option<PlayerSim>>,
    /// Grenades and rockets in flight, with the player who threw them.
    pub projectiles: Vec<(usize, Projectile)>,
    /// Equipment that is down.
    pub equipment: Equipment,
    /// Smoke screens standing ([`crate::smoke`]).
    pub smokes: Vec<crate::smoke::Smoke>,
    /// The tuning pack's `[Smoke]` values (installed data, not hashed).
    pub smoke: crate::smoke::Params,
    pub tuning: MoveTuning,
    /// BO2's `bulletrange` (ZH units): how far a non-spread round is traced.
    pub bullet_range: f32,
    /// BO2's bullet penetration table and dvars (`crate::penetration`, from the tuning pack).
    pub penetration: crate::penetration::Penetration,
    /// The soldier's hit boxes per body state, when not the installed table's (the tests set
    /// their own).
    pub hit_body: Option<&'static crate::hitloc::Poses>,
    pub frame: u32,
    /// Each player seat's chosen class (BO2's `pers["class"]`, `spawn.rs`): his last pick. It
    /// outlives his Generals as what the next one's class screen opens on (every General is picked
    /// for; none comes in wearing it).
    pub chosen: Vec<Option<ChosenClass>>,
    pub events: Vec<Event>,
    /// This step's tracer bullets, for the host layer to draw per viewer and clear.
    pub tracers: Vec<ShotTracer>,
    /// The Generals carrying an Assault Shield, as the frame began (held or stowed; see
    /// [`crate::shield`]): the shots, melee strikes and blasts of this frame test them.
    pub shields: Vec<crate::shield::Bearer>,
    /// Deployed Assault Shields (`sim/deploy.rs`).
    pub deployed: Vec<deploy::Deployed>,
    /// Weapons lying in the world (`sim/drops.rs`).
    pub drops: Vec<drops::Dropped>,
    /// Scavenger packs lying in the world (`sim/scavenger.rs`).
    pub packs: Vec<scavenger::Pack>,
    /// BO2 Zombies' round engine (`sim/zombies.rs`), off but on a Zombies map.
    pub rounds: zombies::Rounds,
    /// Kills of seat Generals not yet read by their death, and whether each was friendly (a
    /// suicide or a teammate's: no drop).
    pub kills: Vec<drops::Kill>,
    /// The blows the seats' Generals took since the last step read them ([`Struck`]): the sim's
    /// own (scored after a step: a seat stepped before the blow sees his hurt next step) and Zero
    /// Hour's (the engine's damage hook, [`Sim::engine_blow`], before the step). Presentation
    /// only: they give a hurt its side (`Damaged`, `Flinch`) and a death to Zero Hour's weapons its
    /// `KilledBy`; nothing in the step reads them otherwise. Cleared as each step ends.
    pub blows: Vec<Struck>,
    /// The players (bit per seat) whose step emitted a combat event (`spawn::is_combat`, BO2's
    /// `hasdonecombat`), set by [`Sim::emit`]. The step reads this, never [`Sim::events`]: the
    /// events are presentation that each client drains at its own render cadence (a headless
    /// replay never does, and the buffer's cap then evicts the newest step's own events from an
    /// index window), so a sim decision taken from them diverged between peers.
    combat_seen: u32,
    /// Every event ever emitted (presentation bookkeeping, not hashed): the log tells a step's
    /// new events by it, since at the [`MAX_EVENTS`] cap the buffer's length no longer moves.
    pub emitted: u64,
    /// The step's rounds that hit a first-person General where the antilag put him (the frame the
    /// shooter saw, [`antilag`]): `(shooter's General, target, seen frame)`. Diagnostics for the log
    /// (the in-game multiplayer test checks a rewound hit lands), drained after each step; read by
    /// nothing in the step, so not hashed.
    pub antilag_hits: Vec<(u32, u32, u32)>,

    /// BO2's assists: each seat's General's attackers this life and each seat's count
    /// ([`score`]).
    pub score: score::Score,
    /// The rewindable world of the last frames ([`antilag`]): what a shot is resolved against.
    pub history: antilag::History,
    /// `AntilagMaxRewind` in logic frames.
    pub antilag_frames: u32,
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
            smoke: crate::smoke::Params::default(),
            tuning: MoveTuning::default(),
            bullet_range: crate::units::inches(crate::tuning::Tuning::default().bulletrange),
            penetration: crate::penetration::Penetration::default(),
            hit_body: None,
            frame: 0,
            chosen: vec![None; MAX_PLAYERS],
            events: Vec::new(),
            tracers: Vec::new(),
            shields: Vec::new(),
            deployed: Vec::new(),
            drops: Vec::new(),
            packs: Vec::new(),
            rounds: zombies::Rounds::default(),
            kills: Vec::new(),
            blows: Vec::new(),
            combat_seen: 0,
            emitted: 0,
            antilag_hits: Vec::new(),
            score: score::Score::default(),

            history: antilag::History::default(),
            antilag_frames: antilag::max_rewind_frames(crate::tuning::Tuning::default().antilag_max_rewind),
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

    /// The smoke screens standing as the engine's sight and path questions see them this frame.
    pub fn smoke_blockers(&self) -> Vec<crate::smoke::Blocker> {
        self.smokes.iter().filter(|s| s.stands(self.frame, &self.smoke)).map(|s| crate::smoke::Blocker { at: s.at, radius: s.radius(self.frame, &self.smoke), born: s.born }).collect()
    }

    /// A sim configured from the gameplay tuning pack (`data/ConquerOps/tuning.ini`, loaded by the
    /// bridge). The tests use `Sim::new`, whose defaults are the pack's own values.
    pub fn with_tuning(t: &crate::tuning::Tuning) -> Self {
        let mut sim = Self::new();
        sim.tuning = MoveTuning::from_tuning(t);
        sim.bullet_range = crate::units::inches(t.bulletrange);
        sim.penetration = crate::penetration::Penetration::from_tuning(t);
        sim.equipment.tuning = crate::equipment::EquipTuning::from_tuning(t);
        sim.equipment.sensor = crate::sensor::SensorTuning::from_tuning(t);
        sim.antilag_frames = antilag::max_rewind_frames(t.antilag_max_rewind);
        sim.smoke = crate::smoke::Params::from_tuning(t);
        sim
    }

    pub fn reset(&mut self) {
        *self = Sim::with_tuning(&crate::tuning::get());
    }

    fn emit(&mut self, player: usize, kind: EventKind) {
        if player < MAX_PLAYERS && spawn::is_combat(&kind) {
            self.combat_seen |= 1 << player;
        }
        if self.events.len() >= MAX_EVENTS {
            self.events.remove(0);
        }
        self.events.push(Event { player: player as i32, kind });
        self.emitted += 1;
    }

    pub fn player(&self, player: i32) -> Option<&PlayerSim> {
        self.players.get(usize::try_from(player).ok()?)?.as_ref()
    }

    /// Where the events emitted since `emitted` was `since` start in [`Sim::events`] (at the cap
    /// the oldest went from the front, so a buffer's length cannot tell).
    pub fn first_event_since(&self, since: u64) -> usize {
        let new = usize::try_from(self.emitted.saturating_sub(since)).unwrap_or(usize::MAX);
        self.events.len().saturating_sub(new)
    }

    /// Ids of Generals currently driven by first-person input (their AI and physics are suspended).
    pub fn controlled_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.players.iter().flatten().filter(|p| p.is_first_person()).map(|p| p.general)
    }

    /// `MSG_ZHBO_FPS_INPUT` without a seen frame (the sim's own tests, an old sender): latest
    /// input for a player's General, resolved against the world as it is.
    pub fn input(&mut self, player: i32, general: u32, input: FpsInput) {
        self.input_seen(player, general, input, 0);
    }

    /// `MSG_ZHBO_FPS_INPUT`: latest input for a player's General, sent while its machine showed
    /// logic frame `seen_frame` ([`PlayerSim::seen_frame`]). Several in one frame (the run-ahead
    /// shrank): the last one's held keys, move, look and seen frame, every one's presses.
    pub fn input_seen(&mut self, player: i32, general: u32, input: FpsInput, seen_frame: u32) {
        let Ok(idx) = usize::try_from(player) else { return };
        if let Some(Some(p)) = self.players.get_mut(idx) {
            if p.general == general {
                let input = input.sanitized();
                p.pressed_accum |= input.pressed;
                p.input = input;
                p.seen_frame = seen_frame;
            }
        }
    }

    /// Once per logic frame, after the frame's messages were dispatched. The damage the step deals
    /// is scored for assists ([`score`]).
    pub fn step(&mut self, host: &dyn LogicHost, frame: u32) {
        // Every kill the step deals is noted, so a death knows whether it drops its weapon.
        let witness = drops::Witness::new(host);
        let died = self.scored(&witness, |sim, host| sim.step_frame(host, frame));
        // A General whose record ended this step: his attacker list ends only now, after the
        // step's kills were scored. His record goes as his seat is stepped, which may be after
        // the seat that killed him in the same frame; scoring that kill needs his list.
        for general in died {
            self.score.forget(general);
        }
        self.drop_scavenger_packs(host, &witness.take_killings());
        self.note_kills(witness.take());
    }

    /// The frame's step; returns the Generals whose records ended in it (died).
    fn step_frame(&mut self, host: &dyn LogicHost, frame: u32) -> Vec<u32> {
        let mut died = Vec::new();
        self.frame = frame;
        self.shields = self.shield_bearers();
        self.equipment.shields.clone_from(&self.shields);
        // The Generals' perks against others' blasts (`sim::guard`), for this frame's blasts.
        self.equipment.guards.clear();
        self.equipment.guards.extend(self.players.iter().flatten().map(|p| guard::Guard::of(p.general, &p.arsenal, p.is_first_person(), p.body.eye_height)));
        for idx in 0..self.players.len() {
            // A pick that ended his wait behind the class screen spawns him now.
            self.spawn_in(host, idx);
            let Some(mut p) = self.players[idx].take() else { continue };
            self.combat_seen = 0;
            let keep = self.step_player(host, idx, &mut p, &mut died);
            if self.combat_seen & (1 << idx) != 0 {
                p.combat = true;
            }
            if keep {
                self.players[idx] = Some(p);
            }
        }
        self.step_projectiles(host);
        self.step_equipment(host);
        self.apply_shock_hits(host);
        self.step_deployed(host);
        self.step_drops(host);
        self.step_packs(host);
        self.step_zombies(host);
        let (frame, smoke) = (self.frame, self.smoke);
        self.smokes.retain(|s| s.stands(frame, &smoke));
        for s in &self.smokes {
            host.smoke(s.at, s.radius(frame, &smoke));
        }
        self.record_history();
        // The blows were read by this step's hurts and deaths (the step's own are scored next).
        self.blows.clear();
        died
    }

    /// The last blow `general` took: this step's own (the scoring host's record), else the ones
    /// since the last step ([`Sim::blows`]).
    fn last_blow(&self, host: &dyn LogicHost, general: u32) -> Option<Struck> {
        if let Some(b) = host.last_blow(general) {
            return Some(Struck { victim: general, dir: b.dir, dmg: crate::death::DmgType::of(b.how, b.weapon, false), how: b.how, engine: false });
        }
        self.blows.iter().rev().find(|b| b.victim == general).copied()
    }

    /// A landing's fall damage (`movement::land::crash_land`): BO2's server deals the event's
    /// percent of the player's max health, a 100% fall `FallDamageFullScale` of it, as
    /// `MOD_FALLING` from no attacker (`FUN_005d2a30` → `G_Damage`). BO2's max health is
    /// `playerMaxHealth` and the hit is truncated there (`(int)`), then taken as his share of his
    /// own Zero Hour health ([`crate::damage::general_zh`]), as every other blow on a General is.
    /// His own blow here: the scoring host reports a death by it like any other (`KilledBy`).
    fn fall_damage(&mut self, host: &dyn LogicHost, p: &PlayerSim, landing: movement::Landing) {
        if landing.damage == 0 {
            return;
        }
        let t = &p.arsenal.tuning;
        let share = if landing.damage >= 100 { t.fall_damage_full_scale } else { f32::from(landing.damage) / 100.0 };
        let amount = crate::damage::general_zh(t, (t.player_max_health * share).trunc(), p.max_health);
        if amount > 0.0 {
            host.damage(p.general, p.general, amount, crate::damage::Blow::bare(crate::damage::MeansOfDeath::Falling));
        }
    }

    /// Seat `idx`'s General died: when Zero Hour's own weapons dealt his last blow, report it as
    /// the sim reports its own kills (`KilledBy`, `score`), for his death animation.
    fn engine_kill(&mut self, host: &dyn LogicHost, idx: usize, general: u32) {
        if let Some(b) = self.last_blow(host, general).filter(|b| b.engine) {
            self.emit(idx, EventKind::KilledBy { dmg: b.dmg, dir: b.dir, by: None, how: b.how, weapon: None });
        }
    }

    /// Zero Hour's own damage to object `victim` (the engine's damage hook, between steps, in the
    /// engine's order): ABI damage kind `kind` ([`crate::damage::ZH_DAMAGE_TYPES`]) travelling along
    /// `dir`. A seat's General keeps it as a blow ([`Sim::blows`]): BO2's means of death for it from
    /// the tuning pack's `[ZeroHourDamage]` table, no BO2 weapon.
    pub fn engine_blow(&mut self, victim: u32, kind: i32, dir: Vec3) {
        let Some(p) = self.players.iter().flatten().find(|p| p.general == victim) else { return };
        let how = p.arsenal.tuning.zh_damage.means(kind);
        let dmg = crate::death::DmgType::of(how, None, !p.body.on_ground);
        self.blows.push(Struck { victim, dir, dmg, how, engine: true });
    }

    fn step_projectiles(&mut self, host: &dyn LogicHost) {
        let mut done = Vec::new();
        let mut bounces = Vec::new();
        let mut struck = Vec::new();
        let players: Vec<u32> = self.players.iter().flatten().map(|p| p.general).collect();
        for (i, (owner, proj)) in self.projectiles.iter_mut().enumerate() {
            // Its blast reaches each General through his guard (`sim::guard`).
            let hits = std::cell::RefCell::new(Vec::new());
            let guarded = guard::Guarded { host, guards: &self.equipment.guards, def: proj.def, stuck_to: proj.stuck_to, thrown: true, owner: proj.owner, at: None, hits: &hits };
            let outcome = proj.step_shielded(&guarded, &self.shields, &players);
            // What it did to first-person Generals, placed where it went off.
            let at = match outcome {
                Some(Outcome::Exploded { at, .. }) => Some(at),
                _ => None,
            };
            self.equipment.shock_hits.borrow_mut().extend(hits.take().into_iter().map(|h| guard::ShockHit { at: h.at.or(at), ..h }));
            if let (Some(at), crate::weapons::Effect::Emp) = (at, proj.def.effect) {
                self.equipment.shock_hits.borrow_mut().extend(shock::emp_hits(host, &self.players, proj.def, proj.owner, at));
            }
            if let Some((at, surface)) = proj.bounced {
                bounces.push((*owner, at, proj.def.bounce_sound(surface)));
            }
            if let Some(kill) = proj.struck {
                struck.push((*owner, kill));
            }
            if let Some(outcome) = outcome {
                done.push((i, *owner, outcome));
            }
        }
        for (owner, at, sound) in bounces {
            self.emit(owner, EventKind::Bounced { at, sound });
        }
        for (owner, kill) in struck {
            self.emit(owner, EventKind::HitMarker { headshot: false, kill, alert: crate::damage::HitAlert::by_entity(crate::damage::MeansOfDeath::Impact, false) });
        }
        for (i, owner, outcome) in done.iter().rev() {
            let (_, proj) = self.projectiles.remove(*i);
            match *outcome {
                Outcome::Exploded { at, hit_enemy, kill, surface } => {
                    let d = proj.def;
                    self.emit(*owner, EventKind::Exploded { at, sound: d.explode_sound.as_str(), impact_type: d.impact_type, surface });
                    self.deployed_caught_in_blast(host, *owner, d, at);
                    if proj.def.effect == crate::weapons::Effect::Smoke && crate::smoke::raised_on(surface) {
                        self.smokes.push(crate::smoke::Smoke { at, born: self.frame });
                    }
                    let mut ev = Vec::new();
                    self.equipment.caught_in_blast(host, *owner, proj.owner, at, proj.def.explosion_radius, proj.def.effect, &mut ev);
                    self.equipment_events(host, ev);
                    if hit_enemy {
                        self.emit(*owner, EventKind::HitMarker { headshot: false, kill, alert: crate::damage::HitAlert::by_entity(crate::damage::splash(d), false) });
                    }
                }
                Outcome::Dud { at, hit_enemy, kill, surface } => {
                    self.emit(*owner, EventKind::Dud { at, sound: proj.def.dud_sound.as_str(), surface });
                    if hit_enemy {
                        self.emit(*owner, EventKind::HitMarker { headshot: false, kill, alert: crate::damage::HitAlert::by_entity(crate::damage::strike(proj.def, true), false) });
                    }
                }
                Outcome::Hit { kill, .. } => {
                    if kill {
                        self.emit(*owner, EventKind::HitMarker { headshot: false, kill, alert: crate::damage::HitAlert::by_entity(crate::damage::strike(proj.def, false), false) });
                    }
                }
                Outcome::Spawned { at, stuck_to, hit_enemy, kill } => {
                    if hit_enemy {
                        self.emit(*owner, EventKind::HitMarker { headshot: false, kill, alert: crate::damage::HitAlert::by_entity(crate::damage::strike(proj.def, false), false) });
                    }
                    // A table without the spawned grenade has nothing to leave (noted once by
                    // `weapon_or_none`).
                    if let Some(def) = crate::weapons::weapon_or_none(&proj.def.spawns) {
                        let grenade = proj.spawned(def, at, stuck_to, host);
                        self.projectiles.push((*owner, grenade));
                    }
                }
                Outcome::Landed { at, stuck_to } => {
                    let yaw = crate::math::det::atan2(proj.vel.y, proj.vel.x);
                    let mut p = Placed::new(proj.def, *owner, proj.owner, proj.object, at, yaw, stuck_to);
                    // A timed throw's fuse runs on where it lies (the Sensor Grenade).
                    p.fuse = proj.fuse.is_finite().then_some(proj.fuse);
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
                        // The Shock Charge's hit is `taser_spike`'s (`_proximity_grenade.gsc:172-177`).
                        let taser = score::Tactical::of(def, false) == Some(score::Tactical::Proximity);
                        self.emit(owner, EventKind::HitMarker { headshot: false, kill, alert: crate::damage::HitAlert::by_entity(crate::damage::splash(def), taser) });
                    }
                }
                EquipEvent::Destroyed { by: Some(by), .. } => {
                    self.emit(by, EventKind::DestroyedEquipment);
                    // `_weaponobjects.gsc:652`'s `updatedamagefeedback()`: no means, no sound.
                    self.emit(by, EventKind::HitMarker { headshot: false, kill: false, alert: crate::damage::HitAlert::Silent });
                }
                EquipEvent::Destroyed { by: None, .. } => {}
                EquipEvent::Intercepted { owner, at } => self.emit(owner, EventKind::Intercepted { at }),
            }
        }
    }

    /// Hand a first-person General back to RTS control where he stands.
    fn return_to_rts(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim) {
        p.mode = Mode::Rts;
        // The RTS weapon is the one that was in hand.
        p.rts.lock = p.arsenal.active as i32;
        host.set_pose(p.general, p.body.pos, p.yaw, false);
        host.set_controlled(p.general, false);
        self.emit(idx, EventKind::ReturnedToRts { general: p.general });
    }

    /// One seat's step; false when his record ends (his General `died`, noted in `died`).
    fn step_player(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, died: &mut Vec<u32>) -> bool {
        let Some(info) = host.object(p.general, p.general) else {
            self.drop_for_death(host, idx, p, None);
            self.engine_kill(host, idx, p.general);
            died.push(p.general);
            self.emit(idx, EventKind::GeneralDied { general: p.general });
            return false;
        };
        if info.has(objflags::DEAD) {
            host.set_controlled(p.general, false);
            self.equipment.clear_owner(host, idx);
            self.drop_for_death(host, idx, p, Some(info.pos));
            self.engine_kill(host, idx, p.general);
            died.push(p.general);
            self.emit(idx, EventKind::GeneralDied { general: p.general });
            return false;
        }
        // Zero Hour gave him to another player: a network game hands a leaving player's assets to
        // a living ally (`onSelfDestruct` with `transferAssets`). This seat no longer commands
        // him; his new owner's own seat takes him over on its next report or take-control. Kept,
        // the old record would drive the General with the leaver's last input forever (or fight
        // the new owner's record for his pose). The leaver's equipment goes with him, as BO2
        // deletes a disconnecting player's weapon objects (`_weaponobjects.gsc`
        // `deleteweaponobjectson`, his insertion by `_tacticalinsertion.gsc` `watchdisconnect`);
        // his planted shield falls with the record (`step_deployed`, `_riotshield.gsc`
        // `deleteshieldonplayerdeathordisconnect`). Kept, it stayed his seat's: his C4 waited for a
        // detonator nobody holds, and its kills and his Claymores' were credited to a seat gone.
        if info.owner_player != idx as i32 {
            if p.is_first_person() {
                self.return_to_rts(host, idx, p);
            }
            self.equipment.clear_leaver(host, idx);
            return false;
        }
        p.shock.step();
        if p.is_first_person() {
            shock::step_taser(host, p);
        } else {
            p.taser = None;
        }
        if !p.is_first_person() {
            // Overhead: hits still count as combat (BO2's damage callback sets `hasdonecombat`),
            // and the health stays current for the next take-control. His body is drawn for
            // everyone as in first person, so the hurt is the same event, with the blow's way (his
            // pain voice and the third-person flinch by `dmgDirection`); no view flinch: there
            // is no first-person view to kick.
            if info.health < p.health - 1e-3 {
                p.combat = true;
                let dir = self.last_blow(host, p.general).map_or(Vec3::ZERO, |b| b.dir);
                self.emit(idx, EventKind::Damaged { amount: p.health - info.health, dir });
            }
            p.health = info.health;
            p.max_health = info.max_health;
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
        // A survivor in last stand lies with his pistol and nothing else (`laststand`).
        if self.is_downed(p.general) {
            Self::downed_input(p, &mut input);
        }
        // Prone, the view turns the body after it and is held within `bg_prone_yawcap` of it
        // (`FUN_008c29a0`, `FUN_008c2720`; the client caps its own view the same way).
        let (fwd, side) = input.move_axes();
        input.yaw = movement::prone_view_yaw(&mut p.body, input.yaw, fwd != 0.0 || side != 0.0, &self.tuning, LOGIC_DT);
        p.input.yaw = input.yaw;
        // The aim is the view plus the kick the last frame left (`FUN_00549fb0`).
        let (off_yaw, off_pitch) = p.aim_offset();
        p.yaw = crate::math::wrap_angle(input.yaw + off_yaw);
        p.pitch = (input.pitch + off_pitch).clamp(-crate::input::MAX_PITCH, crate::input::MAX_PITCH);
        (p.fire_yaw, p.fire_pitch) = p.fire_angles();

        let world = HostWorld { host, viewer: p.general };
        match p.mode {
            Mode::Fps if p.awaiting_class() => {
                if input.pressed & buttons::FIELD_COMPUTER != 0 {
                    // Escape on the spawn's class screen: no spawn, back overhead at once (there is
                    // nothing in hand to put away and no tablet to raise).
                    self.return_to_rts(host, idx, p);
                    return true;
                }
                // Standing behind the class screen: gravity only, nothing else of his input.
                let idle = FpsInput { yaw: input.yaw, pitch: input.pitch, ..Default::default() };
                movement::step(&mut p.body, &idle, MoveModifiers::default(), &self.tuning, &world, LOGIC_DT);
            }
            Mode::Fps => {
                // The use key held over a downed survivor revives him, before anything else takes
                // it (`laststand`).
                self.revive_input(p, &mut input);
                // The use key on a wall buy buys from it (`wallbuy`).
                self.wall_buy_input(host, idx, p, &mut input);
                // The use key on the Mystery Box pulls it, or takes its weapon (`magicbox`).
                self.magic_box_input(host, idx, p, &mut input);
                // The use key works (builds or repairs) at a structure in reach, held, instead of
                // opening the field computer.
                p.site = host.work_site(p.general, p.body.pos, building::REACH);
                p.building = p.site.is_some() && (input.held | input.pressed) & buttons::FIELD_COMPUTER != 0;
                // The use key next to the player's own deployed shield takes it back.
                let picked_up = p.site.is_none() && input.pressed & buttons::FIELD_COMPUTER != 0 && self.use_picks_up_shield(host, idx, p);
                if picked_up {
                    input.pressed &= !buttons::FIELD_COMPUTER;
                }
                // The use key with a dropped weapon in its use list swaps for it (BO2's +activate),
                // instead of opening the field computer. A weapon change is held back by an
                // offhand, a melee or an alt toggle (`FUN_008e0e60`): the press waits for them.
                // A live grenade in his use list (it outranks any weapon, `UseListGrenadeBonus`):
                // the use key picks it up to throw it back.
                p.use_grenade = self.throwback_target(host, p).map_or(0, |i| self.projectiles[i].1.object);
                if p.site.is_none() && input.pressed & buttons::FIELD_COMPUTER != 0 && p.use_grenade != 0 {
                    input.pressed &= !buttons::FIELD_COMPUTER;
                    self.use_grenade(host, idx, p);
                }
                if p.site.is_none() && input.pressed & buttons::FIELD_COMPUTER != 0 && self.use_target(p).is_some() {
                    input.pressed &= !buttons::FIELD_COMPUTER;
                    if p.arsenal.holds_switch_back() {
                        p.pressed_accum |= buttons::FIELD_COMPUTER;
                    } else {
                        self.use_drop(host, idx, p);
                    }
                }
                // The laptop is a weapon change (BO2's killstreak remotes are): an offhand, a melee
                // or an alt toggle under way holds it back, and the press is carried to the next
                // frame until they end (`FUN_008e0e60`, as the weapon-switch key is).
                if p.site.is_none() && input.pressed & buttons::FIELD_COMPUTER != 0 && p.arsenal.holds_switch_back() {
                    input.pressed &= !buttons::FIELD_COMPUTER;
                    p.pressed_accum |= buttons::FIELD_COMPUTER;
                }
                if p.site.is_none() && input.pressed & buttons::FIELD_COMPUTER != 0 {
                    p.mode = Mode::FieldComputer { t: 0.0 };
                    // The gun goes away: a reload in progress is cut short, like a swap, and a
                    // lunge's target lock goes with the melee (it would carry into an RTS order).
                    p.arsenal.put_away();
                    p.charge = None;
                    // Out of first person BO2 zeroes the kick (`FUN_00523070`).
                    p.view_kick = ViewKick::default();
                    p.idle = IdleSway::default();
                    p.gun = Default::default();
                    self.emit(idx, EventKind::FieldComputerOpened);
                } else {
                    let mods = MoveModifiers {
                        // A shellshock scales the run speed for its whole duration (`PM_CmdScale`).
                        speed_scale: p.arsenal.move_scale(p.body.stance == Stance::Prone) * p.shock.movement(),
                        block_sprint: p.arsenal.ads > 0.0 || p.arsenal.offhand.is_some() || p.arsenal.melee_left > 0.0 || p.building,
                        sprint_time_scale: p.arsenal.sprint_scale(),
                        move_faster: p.arsenal.move_faster(),
                        fall_height: p.arsenal.has_specialty(Specialty::FallHeight),
                        charge: self.melee_charge(host, p),
                    };
                    // The aim spread's update reads the player as this frame's move finds him
                    // (`FUN_008e0080` opens `Pmove`); a jump and a lowering stance land after it.
                    let (fwd, side) = input.move_axes();
                    let before = (p.body.on_ground, p.body.eye_height, p.body.horizontal_speed());
                    p.arsenal.spread_in = crate::weapons::aim_spread::SpreadIn {
                        view: (input.yaw, input.pitch),
                        moving: fwd != 0.0 || side != 0.0,
                        run_speed: self.tuning.run_speed(mods.move_faster) / crate::units::ZH_PER_INCH,
                        airborne: !p.body.on_ground,
                        aqs: p.idle.aqs,
                        aqs_left: p.idle.aqs_left,
                        // The step fills the stance and the ground speed.
                        ..p.arsenal.spread_in
                    };
                    let landing = movement::step(&mut p.body, &input, mods, &self.tuning, &world, LOGIC_DT);
                    if let Some(l) = landing {
                        self.fall_damage(host, p, l);
                    }
                    p.arsenal.spread_in.view_height = Some(p.body.eye_height / crate::units::DRAWN_ZH_PER_INCH);
                    let jumped = before.0 && !p.body.on_ground && p.body.vel.z > 0.0 && input.pressed & buttons::JUMP != 0;
                    let lowering = p.body.eye_height < before.1 - 1e-4;
                    let mut actions = Vec::new();
                    let stance = match p.body.stance {
                        Stance::Stand => 0,
                        Stance::Crouch => 1,
                        Stance::Prone => 2,
                    };
                    // The spread update's ground speed: the velocity the move started from.
                    let speed = before.2;
                    // The hammer, or building, takes the hands: the gun gets no input meanwhile.
                    let busy = self.step_building(host, idx, p, &input);
                    let (held, pressed) = if busy { (0, 0) } else { (input.held, input.pressed) };
                    // BO2's plant check, from the world, for the arsenal's plantable items.
                    p.arsenal.plant_ok = [true, false].map(|lethal| can_plant(p.arsenal.offhand_def(lethal), &p.body, p.yaw, p.pitch, p.general, host));
                    let prev_ads = p.arsenal.ads;
                    p.arsenal.step(held, pressed, p.body.sprinting, stance, speed, &mut p.rng, &mut actions);
                    // The move's events on the scale, after this frame's shots took theirs
                    // (`FUN_008b5000`'s jump add; `FUN_008c1120`: with `bg_ads`, a view height
                    // above its target opens it).
                    if jumped {
                        p.arsenal.aim_spread.jumped(&p.arsenal.tuning);
                    }
                    if lowering && p.arsenal.tuning.bg_ads != 0.0 {
                        p.arsenal.aim_spread.open();
                    }
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
                    // The other Generals' class perks (Cold Blooded against the optics).
                    let players = &self.players;
                    let perks = |id: u32| players.iter().flatten().find(|q| q.general == id).map_or(crate::weapons::Specialties::NONE, |q| q.arsenal.class_specialties());
                    p.optics.step(host, p.general, p.body.eye(), dir, p.arsenal.ads, p.arsenal.current().def, self.frame, &perks);
                }
            }
            Mode::FieldComputer { t } => {
                // Stand still while the laptop comes up; gravity still applies.
                let idle = FpsInput { yaw: input.yaw, pitch: input.pitch, ..Default::default() };
                movement::step(&mut p.body, &idle, MoveModifiers::default(), &self.tuning, &world, LOGIC_DT);
                let t = t + LOGIC_DT;
                if t >= FIELD_COMPUTER_RAISE {
                    self.return_to_rts(host, idx, p);
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
        // A dropped weapon's draw is keyed by its object (`Sim::dropped_poses`).
        let Some(p) = self.players.iter().flatten().find(|p| p.general == general) else { return self.dropped_camo(general, vm) };
        // A picked-up weapon wears the camo it was dropped with.
        let picked = p.arsenal.weapons.iter().filter_map(|w| w.options.as_ref().map(|o| (w.main, o.camo.as_str())));
        if let Some(c) = picked.filter_map(|(d, camo)| crate::nativebody::camo_for(d, camo, vm)).next() {
            return Some(c);
        }
        let primary = crate::weapons::catalog().get(&p.arsenal.loadout.primary)?;
        crate::nativebody::camo_for(primary, &p.camo, vm)
    }

    /// What each General's native third person needs this frame (Track R7): the weapon in hand
    /// (`WeaponDef::world_index`, -1 for none), the stance and the body clip the sim's movement
    /// state picks, the clip's clock and the pistol stance. `motion` is the engine's answer for
    /// the Generals under RTS control (Zero Hour walks them; the sim does not step their bodies),
    /// ignored while first person. `viewer` is the General this machine is looking
    /// through in first person: his body is hidden and the viewmodel draws his arms, so he gets no
    /// third person. Every other General does, in first person or not — in a network game the
    /// other players' Generals are in first person on their own machines, and here they must show
    /// their class weapon (attachments, camo) in their hands.
    pub fn native_poses(&self, motion: &[EngineMotion], viewer: Option<u32>) -> Vec<NativePose> {
        let time = self.frame as f32 * LOGIC_DT;
        self.players
            .iter()
            .flatten()
            .filter(|p| !(p.is_first_person() && Some(p.general) == viewer))
            .map(|p| {
                let a = &p.arsenal;
                let engine = motion.iter().find(|m| m.general == p.general).copied();
                // The field computer's tablet is the W3D body's (`world_poses`): no native weapon
                // or body meanwhile, so the W3D one shows it.
                let tablet = matches!(p.mode, Mode::FieldComputer { .. }) || (matches!(p.mode, Mode::Fps) && a.raise_left > a.raise_total + 1e-4);
                NativePose {
                    general: p.general,
                    weapon: if tablet { -1 } else { a.current().def.world_index() }, // content: presentation (native_poses: the third-person draw)
                    stance: if p.is_first_person() { p.stance_index() } else { 0 },
                    state: p.body_state(engine),
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
            .filter_map(|p| p.arsenal.weapons.iter().map(|w| w.def).find(|d| d.is_shield() && d.viewmodel >= 0).map(|d| (p.general, d.viewmodel))) // content: presentation (stowed_weapons: the third-person draw)
            .collect()
    }

    /// Each General's lethal and tactical as converted world weapons (`WeaponDef::viewmodel`, -1
    /// for none): the third-person body holds the one primed or thrown (`nativebody::OFFHAND_TAG`).
    pub fn offhand_weapons(&self) -> Vec<(u32, [i32; 2])> {
        self.players.iter().flatten().map(|p| (p.general, [p.arsenal.offhand_def(true).viewmodel, p.arsenal.offhand_def(false).viewmodel])).collect() // content: presentation (offhand_weapons: the third-person draw)
    }

    /// Every General's body state now (`nativebody::state_of`, from the sim's own stance, gait,
    /// sprint and ADS — never the render clock): the pose his hit boxes take in a trace. An
    /// overhead General stands (the engine's motion is presentation, so the sim poses him idle).
    pub(super) fn hit_states(&self) -> Vec<(u32, u16)> {
        self.players.iter().flatten().map(|p| (p.general, p.body_state(None))).collect()
    }

    /// The enemies `player`'s Sensor Grenades put on his compass ([`crate::sensor`]), and how
    /// strongly the minimap draws each.
    pub fn sensor_marks(&self, player: i32) -> Vec<(crate::sensor::Mark, f32)> {
        let Ok(idx) = usize::try_from(player) else { return Vec::new() };
        self.equipment.marks.of(idx).map(|m| (*m, m.alpha(&self.equipment.sensor))).collect()
    }

    /// The health he has and BO2's regeneration (`_healthoverlay.gsc` `playerhealthregen`): a hit
    /// restarts the wait (`playerHealthRegenTime`); once it is over he is whole again at once,
    /// unless his health fell to `healthoverlaycutoff` or under since he was last whole (very
    /// hurt), when it comes back `regenrate` of full every `RegenStep` from `veryhurttime` after the
    /// hit on.
    fn track_health(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, info: &ObjInfo) {
        let t = crate::tuning::get();
        p.max_health = info.max_health;
        if info.max_health > 0.0 && info.health >= info.max_health - 1e-3 {
            p.very_hurt = false;
        } else if info.max_health > 0.0 && info.health <= info.max_health * t.health_overlay_cutoff {
            p.very_hurt = true;
        }
        if info.health < p.health - 1e-3 {
            p.regen_wait = if p.very_hurt { t.regen_delay.max(t.very_hurt_time) } else { t.regen_delay };
            let amount = p.health - info.health;
            let dir = self.last_blow(host, p.general).map_or(Vec3::ZERO, |b| b.dir);
            self.emit(idx, EventKind::Damaged { amount, dir });
            p.arsenal.aim_spread.damaged(amount, info.max_health, &p.arsenal.tuning);
            let toughness = p.arsenal.class_specialties().has(Specialty::BulletFlinch);
            let flinch = crate::weapons::recoil::flinch_degrees(crate::damage::general_bo2(&t, amount, info.max_health), toughness, &p.arsenal.tuning);
            let (yaw_deg, pitch_deg) = crate::weapons::recoil::flinch_kick(flinch, dir, p.yaw, p.pitch);
            self.emit(idx, EventKind::Flinch { yaw_deg, pitch_deg });
        } else {
            p.regen_wait = (p.regen_wait - LOGIC_DT).max(0.0);
            if p.regen_wait <= 0.0 && info.health < info.max_health {
                let per_second = if p.very_hurt { info.max_health * t.regen_rate / t.regen_step.max(1e-3) } else { f32::INFINITY };
                let amount = (per_second * LOGIC_DT).min(info.max_health - info.health);
                host.heal(p.general, amount);
                if info.health + amount >= info.max_health - 1e-3 {
                    // Whole again: BO2 forgets who hurt him (`newhealth >= 1.0` ->
                    // `resetattackerlist`, `_healthoverlay.gsc:172-175`), so a later kill credits
                    // none of them an assist.
                    self.score.forget(p.general);
                } else {
                    // BO2 decays every attacker's damage by the health regained (`score`).
                    self.score.decay(p.general, crate::damage::general_bo2(&t, amount, info.max_health));
                }
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
        EventKind::ClassAtNextSpawn { .. } => Some("MP_CHANGE_CLASS_NEXT_SPAWN"),
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
        ClassAtNextSpawn { general, class } => {
            format!("player {}'s General {general} keeps his kit; the {class} class comes in at the next spawn (\"{}\")", e.player, event_hint(&e.kind).map_or("", hint_text))
        }
        FieldComputerOpened => format!("player {} opened the field computer", e.player),
        ZombieRound { round } => format!("player {} sees zombies round {round} begin", e.player),
        ZombieRoundOver { round } => format!("player {} sees zombies round {round} end", e.player),
        ZombiePoints { points } => format!("player {} scored {points} zombie points", e.player),
        ZombiePowerUp { kind } => format!("player {} sees power-up {} taken", e.player, crate::zombies::PowerUps::get().kinds.get(kind as usize).map_or("?", |k| k.name.as_str())),
        ZombiePurchase { cost } => format!("player {} bought from a wall for {cost} points", e.player),
        Downed { general, points } => format!("player {}'s survivor {general} is down ({points} points lost)", e.player),
        Revived { general, by } => format!("player {} sees survivor {general} revived by {by}", e.player),
        BledOut { general } => format!("player {}'s survivor {general} bled out", e.player),
        ReturnedToRts { general } => format!("player {} returned General {general} to RTS control", e.player),
        GeneralDied { general } => format!("player {}'s General {general} died", e.player),
        Assist { level, tactical } => format!("player {} scored an assist ({})", e.player, tactical.map_or_else(|| (level as u32 * 25).to_string(), |t| format!("{t:?}").to_ascii_lowercase())),
        HitMarker { headshot, kill: true, .. } => format!("player {} killed a target (headshot: {headshot})", e.player),
        Damaged { amount, dir } => format!("player {}'s General took {amount:.0} damage along ({:.2}, {:.2})", e.player, dir.normalized_or_zero().x, dir.normalized_or_zero().y),
        KilledBy { dmg, dir, by, how, weapon } => format!(
            "player {}'s General was killed: dmgType {} along ({:.2}, {:.2}); obituary: by {} with {} ({})",
            e.player,
            dmg.name(),
            dir.normalized_or_zero().x,
            dir.normalized_or_zero().y,
            by.map_or_else(|| "the world".to_string(), |s| format!("player {s}")),
            weapon.map_or("none", |w| w.id.as_str()),
            how.name()
        ),
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
        WeaponDropped { def, at } => format!("player {}'s General dropped his {} at ({:.0}, {:.0})", e.player, def.id, at.x, at.y),
        WeaponPickedUp { def, .. } => format!("player {} picked up the {}", e.player, def.id),
        AmmoPickedUp { def, .. } => format!("player {} took ammo from a dropped {}", e.player, def.id),
        ScavengerPickedUp { at } => format!("player {} took a Scavenger pack at ({:.0}, {:.0})", e.player, at.x, at.y),
        _ => return None,
    };
    Some(msg)
}

#[cfg(test)]
mod shield_tests;

#[cfg(test)]
pub(crate) mod tests;
