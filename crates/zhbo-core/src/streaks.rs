//! Black Ops II scorestreaks: points for kills fill the streak bar, each streak in the loadout is
//! earned when the points pass its cost, and earned streaks are kept until used (points start over
//! when the General dies, as in BO2).
//!
//! Streaks in Conquer Ops:
//! * UAV: enemies show on the minimap, even under Zero Hour's shroud, for 30 seconds.
//! * Sentry Gun: an automated BO2 sentry turret on the General's side, for 90 seconds.
//! * Death Machine and War Machine: BO2's handheld scorestreak weapons, until their ammo runs out.
//!
//! Costs are BO2's (in points; a kill is 100). Hardline ("Earn Scorestreaks faster") scales the
//! points earned.

use crate::units::inches;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Streak {
    Uav,
    SentryGun,
    DeathMachine,
    WarMachine,
}

/// Points for a kill, BO2's team deathmatch value.
pub const KILL_POINTS: u32 = 100;
/// Hardline: points earned are scaled by this.
pub const HARDLINE_POINTS: f32 = 1.25;
/// How long a UAV and a Sentry Gun last.
pub const UAV_TIME: f32 = 30.0;
pub const SENTRY_TIME: f32 = 90.0;
/// The Sentry Gun is set down this far in front of the General.
pub const SENTRY_DISTANCE: f32 = inches(48.0);
/// The Zero Hour object of the Sentry Gun (`zhbo_assets` makes it from BO2's turret model).
pub const SENTRY_OBJECT: &str = "ZHBO_SentryGun";

impl Streak {
    pub const ALL: [Streak; 4] = [Streak::Uav, Streak::SentryGun, Streak::DeathMachine, Streak::WarMachine];

    /// BO2's cost in points.
    pub fn cost(self) -> u32 {
        match self {
            Streak::Uav => 425,
            Streak::SentryGun => 800,
            Streak::DeathMachine => 850,
            Streak::WarMachine => 900,
        }
    }

    /// BO2's key for it (`KILLSTREAK_<key>`, `hud_ks_*` art).
    pub fn key(self) -> &'static str {
        match self {
            Streak::Uav => "RADAR",
            Streak::SentryGun => "AUTO_TURRET",
            Streak::DeathMachine => "MINIGUN",
            Streak::WarMachine => "M32",
        }
    }

    /// The name BO2 shows.
    pub fn name(self) -> &'static str {
        match self {
            Streak::Uav => "UAV",
            Streak::SentryGun => "Sentry Gun",
            Streak::DeathMachine => "Death Machine",
            Streak::WarMachine => "War Machine",
        }
    }

    /// The weapon a handheld streak gives.
    pub fn weapon(self) -> Option<&'static str> {
        match self {
            Streak::DeathMachine => Some("minigun_mp"),
            Streak::WarMachine => Some("m32_mp"),
            _ => None,
        }
    }

    /// BO2's sting when it is earned (`mpl_killstreak_*`, converted under this name).
    pub fn earned_sound(self) -> &'static str {
        match self {
            Streak::Uav => "ZHBO_BO2_Streak_RADAR",
            Streak::SentryGun => "ZHBO_BO2_Streak_AUTO_TURRET",
            Streak::DeathMachine => "ZHBO_BO2_Streak_MINIGUN",
            Streak::WarMachine => "ZHBO_BO2_Streak_M32",
        }
    }

    /// The HUD icon (BO2's `hud_ks_*` art, converted).
    pub fn icon(self) -> String {
        format!("ZHBO_BO2_Streak_{}", self.key())
    }
}

/// A player's scorestreak progress.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Streaks {
    /// Points since the General last died.
    pub score: u32,
    /// Earned and not yet used, oldest first.
    pub earned: Vec<Streak>,
    /// Which earned streak the use key fires.
    pub selected: usize,
    /// Seconds of UAV left.
    pub uav_left: f32,
}

impl Streaks {
    /// Add points; returns the streaks of `loadout` this earned.
    pub fn add_points(&mut self, points: u32, hardline: bool, loadout: &[Streak]) -> Vec<Streak> {
        let points = if hardline { (points as f32 * HARDLINE_POINTS).round() as u32 } else { points };
        let before = self.score;
        self.score += points;
        let new: Vec<Streak> = loadout.iter().copied().filter(|s| before < s.cost() && self.score >= s.cost()).collect();
        if !new.is_empty() {
            self.earned.extend(&new);
            // BO2 selects the newest one.
            self.selected = self.earned.len() - 1;
        }
        new
    }

    /// The streak the use key would fire.
    pub fn current(&self) -> Option<Streak> {
        self.earned.get(self.selected).copied()
    }

    pub fn next(&mut self) {
        if !self.earned.is_empty() {
            self.selected = (self.selected + 1) % self.earned.len();
        }
    }

    /// Use the selected streak (it is gone from the earned list).
    pub fn take(&mut self) -> Option<Streak> {
        let s = self.current()?;
        self.earned.remove(self.selected);
        self.selected = self.earned.len().saturating_sub(1);
        Some(s)
    }

    /// The General died: points start over; earned streaks are kept.
    pub fn died(&mut self) {
        self.score = 0;
    }

    /// The next loadout streak still to earn this life, and its cost.
    pub fn next_goal(&self, loadout: &[Streak]) -> Option<Streak> {
        loadout.iter().copied().filter(|s| s.cost() > self.score).min_by_key(|s| s.cost())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaks_are_earned_at_their_cost_and_kept_until_used() {
        let loadout = [Streak::Uav, Streak::SentryGun, Streak::DeathMachine];
        let mut s = Streaks::default();
        for _ in 0..4 {
            assert!(s.add_points(KILL_POINTS, false, &loadout).is_empty());
        }
        assert_eq!(s.add_points(KILL_POINTS, false, &loadout), vec![Streak::Uav]);
        assert_eq!(s.next_goal(&loadout), Some(Streak::SentryGun));
        s.died();
        assert_eq!(s.score, 0);
        assert_eq!(s.take(), Some(Streak::Uav), "kept after death");
        assert_eq!(s.take(), None);
    }

    #[test]
    fn hardline_earns_faster() {
        let loadout = [Streak::Uav];
        let mut s = Streaks::default();
        let mut kills = 0;
        while s.earned.is_empty() {
            s.add_points(KILL_POINTS, true, &loadout);
            kills += 1;
        }
        assert_eq!(kills, 4, "425 points at 125 a kill");
    }
}
