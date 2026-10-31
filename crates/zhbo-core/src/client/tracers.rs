//! Which tracer the local viewer sees of each tracer bullet, and where it is drawn from.
//!
//! The sim queues the bullets that draw a tracer ([`ShotTracer`]: shooter, weapon, end) on BO2's
//! per-shooter countdown; the asset is the viewer's. BO2's `Tracer_Spawn` (`FUN_007c6150`) asks
//! `FUN_004bbf10` whether the shooter is on the local client's team (a non-zero team equal to the
//! local client's) and hands that to `FUN_008eb420`, which walks to the `enemyTracerType` for any
//! other shooter. Zero Hour has players, not BO2 teams, so the viewer's side is the local player's:
//! his own shots and his allies' are friendly, everyone else's (enemies and neutrals, as BO2's
//! team-less shooters) enemy.
//!
//! Each tracer flies from the shooter's flash bone (the FX weapon's `FireFX`, `fire_fx`); the
//! viewer's own first-person General is skipped by the bridge (`cbFireFx`).
//!
//! Presentation only, outside the lockstep: the FX weapons deal no damage, and which one plays
//! differs per machine by design (docs/CONTRACT.md).

use crate::host::{objflags, LogicHost};
use crate::sim::ShotTracer;

/// Who is looking: the local player, the General he commands (0: none yet) and the General
/// whose eyes the camera is in (0: the overhead view).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Viewer {
    pub player: i32,
    pub general: u32,
    pub first_person: u32,
}

/// Draw one logic step's tracer bullets as `viewer` sees them, through `fire_fx`. Empties `shots`.
pub fn play(host: &dyn LogicHost, viewer: &Viewer, shots: &mut Vec<ShotTracer>) {
    for s in shots.drain(..) {
        let fx_weapon = choose(s.def, enemy_of(host, viewer, s.shooter));
        if !fx_weapon.is_empty() {
            host.fire_fx(s.shooter, fx_weapon, s.target, s.end);
        }
    }
}

/// Whether `shooter` is not on the viewer's side (`FUN_004bbf10` false): friendly only when the
/// viewer's player owns him or is allied to his owner. A shooter that is gone is no friend.
pub fn enemy_of(host: &dyn LogicHost, viewer: &Viewer, shooter: u32) -> bool {
    match host.object(shooter, viewer.general) {
        Some(o) => !(o.owner_player == viewer.player || o.has(objflags::ALLY)),
        None => true,
    }
}

/// The FX weapon for a viewer the shooter is `enemy` to or not: the def's enemy tracer (set for
/// every def with a tracer by the converter) or its friendly one.
pub fn choose(def: &'static crate::weapons::WeaponDef, enemy: bool) -> &'static str {
    if enemy && !def.enemy_tracer_fx_weapon.is_empty() {
        &def.enemy_tracer_fx_weapon
    } else {
        &def.tracer_fx_weapon
    }
}
