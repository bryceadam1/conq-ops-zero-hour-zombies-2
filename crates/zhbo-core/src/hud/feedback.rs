//! The HUD feedback state BO2's cgame keeps between frames beyond [`super::state::HudLui`]: the
//! kill feed ([`super::obituary`]), the hit marker's perk variant and the Shock Charge alert's
//! rate limit (`_damagefeedback.gsc`), what the crosshair was last over (the crosshair's team
//! colour, t6mp `FUN_004b52a0`) and each friendly name's fade (`FUN_004b3c90`), plus the client's
//! own random stream for the damage indicators' jitter. Client-side presentation only: nothing
//! here feeds the sim, and the world is only read (through the engine's logic host) to see what
//! the view can see.

use super::obituary::Feed;
use crate::damage::PerkFeedback;
use crate::host::{objflags, LogicHost};
use crate::math::Vec3;
use crate::sim::{PlayerSim, Sim, MAX_PLAYERS};
use crate::units::inches;

/// `cg_enemyNameFadeOut` / `cg_friendlyNameFadeOut` / `cg_friendlyNameFadeIn` (ms → s, G6 and
/// `FUN_004d2350`): how long the crosshair keeps a target's colour, and how a friendly name fades.
pub const ENEMY_NAME_FADE_OUT: f32 = 0.25;
pub const FRIENDLY_NAME_FADE_OUT: f32 = 1.5;
pub const FRIENDLY_NAME_FADE_IN: f32 = 0.0;
/// The crosshair's own world trace (`FUN_004b64a0`: 15000 along the view, `DAT_00c73500`), inches.
pub const CROSSHAIR_TRACE: f32 = 15000.0;
/// `cg_overheadNamesMaxDist` (inches) and `cg_canSeeFriendlyFrustumMinDistance`: names past the
/// first are never drawn; nearer than the second the frustum test is skipped (`FUN_004b2500`).
pub const OVERHEAD_NAMES_MAX_DIST: f32 = 10000.0;
pub const FRUSTUM_MIN_DIST: f32 = 100.0;
/// `FUN_004b2500`'s leeway behind the view plane (`DAT_00c73524`, inches).
pub const BEHIND_LEEWAY: f32 = 15.0;
/// The name's anchor over the head tag (`FUN_004b1b60`: tag + 10, `DAT_00c7340c`), inches.
pub const NAME_OVER_HEAD: f32 = 10.0;
/// `_damagefeedback.gsc` `playhitsound`: one alert per this many seconds.
pub const HIT_SOUND_GAP: f32 = 0.05;

/// What the crosshair was last over: an enemy or a friend, and when (the HUD clock).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Target {
    pub enemy: bool,
    pub at: Option<f32>,
}

impl Target {
    /// `FUN_004b52a0`'s colour for a weapon with `crosshairColorChange`: the enemy team's within
    /// `cg_enemyNameFadeOut` of the crosshair leaving an enemy, our team's within
    /// `cg_friendlyNameFadeOut` of leaving a friend, else white.
    pub fn colour(&self, now: f32) -> (f32, f32, f32) {
        match self.at {
            Some(t) if self.enemy && now - t < ENEMY_NAME_FADE_OUT => super::obituary::ENEMY_TEAM,
            Some(t) if !self.enemy && now - t < FRIENDLY_NAME_FADE_OUT => super::obituary::MY_TEAM,
            _ => (1.0, 1.0, 1.0),
        }
    }
}

/// One friendly name's fade (`FUN_004b3c90`'s per-client record): seen now, when it was first
/// seen this sighting and last seen.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NameFade {
    pub seen: bool,
    pub first: f32,
    pub last: Option<f32>,
}

impl NameFade {
    pub fn update(&mut self, visible: bool, now: f32) {
        if visible {
            if !self.seen {
                self.seen = true;
                self.first = now;
            }
            self.last = Some(now);
        } else {
            self.seen = false;
        }
    }

    /// The name's alpha: fading in over `cg_friendlyNameFadeIn` from the first sight, full while
    /// seen, then down over `cg_friendlyNameFadeOut` from the last.
    pub fn alpha(&self, now: f32) -> f32 {
        let Some(last) = self.last else { return 0.0 };
        let since = now - last;
        if since >= FRIENDLY_NAME_FADE_OUT {
            return 0.0;
        }
        let shown = last - self.first;
        if shown < FRIENDLY_NAME_FADE_IN {
            return shown / FRIENDLY_NAME_FADE_IN;
        }
        (FRIENDLY_NAME_FADE_OUT - since) / FRIENDLY_NAME_FADE_OUT
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Feedback {
    pub obits: Feed,
    /// The hit marker's material: BO2's last `setshader` (`None`: `damage_feedback`).
    pub marker: Option<PerkFeedback>,
    /// When the last rate-limited alert played (the HUD clock).
    pub taser_at: f32,
    pub target: Target,
    pub names: [NameFade; MAX_PLAYERS],
    rng: u32,
}

impl Default for Feedback {
    fn default() -> Self {
        Feedback { obits: Feed::default(), marker: None, taser_at: f32::NEG_INFINITY, target: Target::default(), names: [NameFade::default(); MAX_PLAYERS], rng: 0x9E37_79B9 }
    }
}

impl Feedback {
    /// The engine's `random() - 0.5` for the damage indicator's jitter (`FUN_004e8ab0`), from the
    /// client's own stream (never the sim's).
    pub fn jitter(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    }

    /// A rate-limited alert may play at `now` (`playhitsound`'s `hitsoundtracker`).
    pub fn take_alert(&mut self, now: f32) -> bool {
        if now - self.taser_at < HIT_SOUND_GAP {
            return false;
        }
        self.taser_at = now;
        true
    }

    /// Look at the world from `eye` along `dir` for the crosshair's target and every allied seat's
    /// name, at `now`: `me` is the presented seat, `ally(seat)` whether a seat is on his side.
    #[allow(clippy::too_many_arguments)]
    pub fn look(&mut self, host: &dyn LogicHost, sim: &Sim, me: usize, eye: Vec3, dir: Vec3, ally: &dyn Fn(usize) -> bool, now: f32) {
        let Some(p) = sim.players.get(me).and_then(|p| p.as_ref()) else { return };
        let dir = dir.normalized_or_zero();
        let range = inches(CROSSHAIR_TRACE);
        let mut objs = Vec::new();
        host.query(eye, range, p.general, &mut objs);
        objs.retain(|o| !o.has(objflags::DEAD) && !o.has(objflags::NO_COLLIDE));
        let targets: Vec<crate::hitscan::Target> = objs.iter().map(crate::hitscan::solid).collect();
        let ground = |x: f32, y: f32| host.ground_height(x, y);
        // The crosshair: the first thing along the view, if it is someone (not a structure).
        if let crate::hitscan::HitResult::Object { id, .. } = crate::hitscan::trace(eye, dir, range, &targets, p.general, &ground) {
            if let Some(o) = objs.iter().find(|o| o.id == id).filter(|o| !o.has(objflags::STRUCTURE)) {
                if o.has(objflags::ENEMY) || o.has(objflags::ALLY) {
                    self.target = Target { enemy: o.has(objflags::ENEMY), at: Some(now) };
                }
            }
        }
        // The names (`FUN_004b3c90`): every other seat's General on his side.
        for (seat, fade) in self.names.iter_mut().enumerate() {
            let other = sim.players.get(seat).and_then(|q| q.as_ref()).filter(|_| seat != me && ally(seat));
            let visible = other.is_some_and(|q| name_visible(eye, dir, q, &targets, &ground));
            fade.update(visible, now);
        }
    }
}

/// Where a General's name hangs: over his head (`FUN_004b1b60`'s head tag + 10).
pub fn name_anchor(q: &PlayerSim) -> Vec3 {
    q.body.eye() + Vec3::new(0.0, 0.0, inches(NAME_OVER_HEAD))
}

/// `FUN_004b2500` + `FUN_004b3690`: in front of the view (15 inches' leeway, then strictly), within
/// `cg_overheadNamesMaxDist`, and nothing between the eye and his head (a friendly's trace must
/// run clear).
fn name_visible(eye: Vec3, dir: Vec3, q: &PlayerSim, targets: &[crate::hitscan::Target], ground: &dyn Fn(f32, f32) -> f32) -> bool {
    let at = name_anchor(q);
    let d = at - eye;
    let ahead = d.dot(dir);
    if ahead < 0.0 || ahead + inches(BEHIND_LEEWAY) < 0.0 {
        return false;
    }
    let dist = d.length();
    if dist > inches(OVERHEAD_NAMES_MAX_DIST) || dist <= 1e-3 {
        return false;
    }
    let way = d * (1.0 / dist);
    let blockers: Vec<crate::hitscan::Target> = targets.iter().filter(|t| t.id != q.general).copied().collect();
    matches!(crate::hitscan::trace(eye, way, dist, &blockers, 0, ground), crate::hitscan::HitResult::Nothing { .. })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_shows_while_seen_and_fades_over_a_second_and_a_half() {
        let mut f = NameFade::default();
        assert_eq!(f.alpha(0.0), 0.0);
        f.update(true, 1.0);
        assert_eq!(f.alpha(1.0), 1.0, "cg_friendlyNameFadeIn 0: at once");
        f.update(false, 1.5);
        assert!((f.alpha(1.0 + 0.75) - 0.5).abs() < 1e-5);
        assert_eq!(f.alpha(2.6), 0.0);
    }

    #[test]
    fn the_crosshair_keeps_an_enemys_colour_a_quarter_second_and_a_friends_a_second_and_a_half() {
        let enemy = Target { enemy: true, at: Some(0.0) };
        assert_eq!(enemy.colour(0.2), super::super::obituary::ENEMY_TEAM);
        assert_eq!(enemy.colour(0.3), (1.0, 1.0, 1.0));
        let friend = Target { enemy: false, at: Some(0.0) };
        assert_eq!(friend.colour(1.4), super::super::obituary::MY_TEAM);
        assert_eq!(friend.colour(1.6), (1.0, 1.0, 1.0));
        assert_eq!(Target::default().colour(0.0), (1.0, 1.0, 1.0));
    }

    #[test]
    fn the_jitter_is_the_engines_half_range_and_the_alert_waits_its_gap() {
        let mut f = Feedback::default();
        for _ in 0..1000 {
            let j = f.jitter();
            assert!((-0.5..0.5).contains(&j));
        }
        assert!(f.take_alert(1.0));
        assert!(!f.take_alert(1.04));
        assert!(f.take_alert(1.06));
    }
}
