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
//! Others' tracers fly from their soldier's flash bone (the FX weapon's `FireFX`, `fire_fx`). The
//! viewer's own first-person shots draw too: the same countdown (`FUN_00539800` hands
//! `FUN_00538e40` its draw flag set for every shooter), from the viewmodel's flash tag
//! (`FUN_00535f80`'s tracer start for the predicted local player), so they are queued for the
//! bridge's viewmodel pass ([`ViewTracers::frame`], `zhbo_viewmodel_tracer`) instead.
//! `cg_firstPersonTracerChance` is registered (`FUN_004d2350`, default 0) but never read in t6mp,
//! so no chance roll is ported (no client random stream is needed).
//!
//! Presentation only, outside the lockstep: the FX weapons deal no damage, and which one plays
//! differs per machine by design (docs/CONTRACT.md).

use crate::host::{objflags, LogicHost};
use crate::math::Vec3;
use crate::sim::ShotTracer;

/// Who is looking: the local player, the General he commands (0: none yet) and the General
/// whose eyes the camera is in (0: the overhead view).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Viewer {
    pub player: i32,
    pub general: u32,
    pub first_person: u32,
}

/// One tracer of the viewer's own gun: the FX weapon whose `FireFX` draws it, to the bullet's end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OwnTracer {
    pub fx_weapon: &'static str,
    pub end: Vec3,
}

/// The client's side of the tracers: who views, and the own gun's tracers waiting for the
/// viewmodel pass.
#[derive(Clone, Debug, Default)]
pub struct ViewTracers {
    pub viewer: Viewer,
    /// Queued by the logic steps since the last rendered frame.
    pub pending: Vec<OwnTracer>,
    /// This rendered frame's, for the bridge to draw from the viewmodel's flash tag.
    pub frame: Vec<OwnTracer>,
}

impl ViewTracers {
    /// Draw this step's tracer bullets for the viewer: others' through `fire_fx`, the viewer's own
    /// first-person ones queued for the viewmodel pass. Empties `shots`.
    pub fn play(&mut self, host: &dyn LogicHost, shots: &mut Vec<ShotTracer>) {
        for s in shots.drain(..) {
            let fx_weapon = choose(s.def, enemy_of(host, &self.viewer, s.shooter));
            if fx_weapon.is_empty() {
                continue;
            }
            if self.viewer.first_person != 0 && s.shooter == self.viewer.first_person {
                self.pending.push(OwnTracer { fx_weapon, end: s.end });
            } else {
                host.fire_fx(s.shooter, fx_weapon, s.target, s.end);
            }
        }
    }

    /// A rendered frame starts: the own tracers queued since the last one become this frame's
    /// (none outside first person: there is no viewmodel to draw them from).
    pub fn begin_frame(&mut self, first_person: bool) {
        self.frame.clear();
        if first_person {
            std::mem::swap(&mut self.frame, &mut self.pending);
        } else {
            self.pending.clear();
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
