//! The HUD feedback state BO2's cgame keeps between frames beyond [`super::state::HudLui`]: the
//! kill feed ([`super::obituary`]), the hit marker's perk variant and the Shock Charge alert's
//! rate limit (`_damagefeedback.gsc`), what the crosshair was last over (the crosshair's team
//! colour, t6mp `FUN_004b52a0`) and each General's nameplate with its fade (`FUN_004b3c90`), plus the client's
//! own random stream for the damage indicators' jitter. Client-side presentation only: nothing
//! here feeds the sim, and the world is only read (through the engine's logic host) to see what
//! the view can see.

use super::obituary::Feed;
use crate::damage::PerkFeedback;
use crate::host::{objflags, LogicHost};
#[cfg(test)]
use crate::host::ObjInfo;
use crate::math::Vec3;
use crate::sim::{PlayerSim, Sim};
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
/// How far the nameplates look for Generals (Zero Hour units): the whole of any map.
pub const NAMEPLATE_QUERY: f32 = 1.0e6;
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

/// One General's nameplate: whose he is (`owner`, a Zero Hour player), his owner's house colour,
/// where the name hangs and its fade.
///
/// Deliberate divergence from BO2 (user's ruling, team colours): BO2 names only friends, within
/// `cg_overheadNamesMaxDist`, in its team colour; here every other General carries his owner's
/// name in his owner's Zero Hour house colour, at any range, friend or foe, so a General's side
/// can be told at a glance. BO2's sight rule stays (in front of the view, nothing in between), and
/// Zero Hour's shroud hides an enemy's as it hides him.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nameplate {
    pub general: u32,
    pub owner: i32,
    pub color: u32,
    pub at: Vec3,
    pub fade: NameFade,
}

/// One name's fade (`FUN_004b3c90`'s per-client record): seen now, when it was first
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
    pub names: Vec<Nameplate>,
    rng: u32,
}

impl Default for Feedback {
    fn default() -> Self {
        Feedback { obits: Feed::default(), marker: None, taser_at: f32::NEG_INFINITY, target: Target::default(), names: Vec::new(), rng: 0x9E37_79B9 }
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

    /// Look at the world from `eye` along `dir` for the crosshair's target and every other
    /// General's nameplate, at `now`: `me` is the presented seat.
    pub fn look(&mut self, host: &dyn LogicHost, sim: &Sim, me: usize, eye: Vec3, dir: Vec3, now: f32) {
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
        // The nameplates (`FUN_004b3c90`'s fade, BO2's sight test): every other General, as
        // [`Nameplate`] says, wherever on the map he is.
        let mut all = Vec::new();
        host.query(eye, NAMEPLATE_QUERY, p.general, &mut all);
        for plate in &mut self.names {
            plate.fade.seen = false;
        }
        for o in all.iter().filter(|o| o.has(objflags::GENERAL) && o.id != p.general && !o.has(objflags::DEAD)) {
            let at = sim.players.iter().flatten().find(|q| q.general == o.id && q.is_first_person()).map_or(o.pos + Vec3::new(0.0, 0.0, o.height + inches(NAME_OVER_HEAD)), name_anchor);
            let visible = !o.has(objflags::SHROUDED) && name_visible(eye, dir, at, o.id, p.general, &targets, &ground);
            let plate = match self.names.iter().position(|n| n.general == o.id) {
                Some(k) => &mut self.names[k],
                None => {
                    self.names.push(Nameplate { general: o.id, owner: o.owner_player, color: o.color, at, fade: NameFade::default() });
                    self.names.last_mut().unwrap()
                }
            };
            plate.owner = o.owner_player;
            plate.color = o.color;
            plate.at = at;
            plate.fade.update(visible, now);
        }
        // A General gone from the world fades from where he was last seen, then is forgotten.
        for plate in &mut self.names {
            if !all.iter().any(|o| o.id == plate.general && !o.has(objflags::DEAD)) {
                plate.fade.update(false, now);
            }
        }
        self.names.retain(|n| n.fade.seen || n.fade.alpha(now) > 0.0);
    }
}

/// Where a General's name hangs: over his head (`FUN_004b1b60`'s head tag + 10).
pub fn name_anchor(q: &PlayerSim) -> Vec3 {
    q.body.eye() + Vec3::new(0.0, 0.0, inches(NAME_OVER_HEAD))
}

/// `FUN_004b2500` + `FUN_004b3690`: in front of the view (15 inches' leeway, then strictly) and
/// nothing between the eye and his head (a friendly's trace must run clear; the viewer's own body
/// is not in the way). BO2's `cg_overheadNamesMaxDist` cap is not applied ([`Nameplate`]).
fn name_visible(eye: Vec3, dir: Vec3, at: Vec3, general: u32, viewer: u32, targets: &[crate::hitscan::Target], ground: &dyn Fn(f32, f32) -> f32) -> bool {
    let d = at - eye;
    let ahead = d.dot(dir);
    if ahead < 0.0 || ahead + inches(BEHIND_LEEWAY) < 0.0 {
        return false;
    }
    let dist = d.length();
    if dist <= 1e-3 {
        return false;
    }
    let way = d * (1.0 / dist);
    let blockers: Vec<crate::hitscan::Target> = targets.iter().filter(|t| t.id != general).copied().collect();
    matches!(crate::hitscan::trace(eye, way, dist, &blockers, viewer, ground), crate::hitscan::HitResult::Nothing { .. })
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
    fn every_other_general_wears_his_owners_name_and_house_colour_at_any_range() {
        use crate::sim::fakehost::{general, soldier, FakeHost};
        let host = FakeHost::default();
        host.add(ObjInfo { pos: Vec3::ZERO, ..general(1, 0) });
        // Far past BO2's cg_overheadNamesMaxDist, an enemy: named all the same.
        host.add(ObjInfo { pos: Vec3::new(inches(OVERHEAD_NAMES_MAX_DIST) * 3.0, 0.0, 0.0), color: 0xFFFF_2020, ..general(2, 1) });
        // Under the local player's shroud: no name.
        host.add(ObjInfo { pos: Vec3::new(400.0, 300.0, 0.0), flags: objflags::INFANTRY | objflags::GENERAL | objflags::SHROUDED, ..general(3, 2) });
        // Behind a soldier standing in the line of sight: no name (BO2's trace).
        host.add(ObjInfo { pos: Vec3::new(400.0, -300.0, 0.0), ..general(4, 1) });
        host.add(ObjInfo { pos: Vec3::new(200.0, -150.0, 0.0), height: 40.0, major_radius: 30.0, ..soldier(5, 1, 0.0) });
        let mut sim = Sim::new();
        let mut me = PlayerSim::spawn_for_tests();
        me.general = 1;
        sim.players = vec![Some(me)];
        let mut f = Feedback::default();
        let eye = Vec3::new(0.0, 0.0, 6.0);
        let anchor_of = |id: u32| {
            let o = host.get(id);
            o.pos + Vec3::new(0.0, 0.0, o.height + inches(NAME_OVER_HEAD))
        };
        for (id, shown) in [(2, true), (3, false), (4, false)] {
            let dir = anchor_of(id) - eye;
            f.look(&host, &sim, 0, eye, dir, 1.0);
            let plate = f.names.iter().find(|n| n.general == id);
            assert_eq!(plate.is_some_and(|n| n.fade.alpha(1.0) > 0.0), shown, "General {id}");
        }
        let far = f.names.iter().find(|n| n.general == 2).unwrap();
        assert_eq!((far.owner, far.color), (1, 0xFFFF_2020));
        assert!(f.names.iter().all(|n| n.general != 1), "never his own");
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
