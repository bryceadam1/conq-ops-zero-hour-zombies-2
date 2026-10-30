//! The HUD's own clock and the little state BO2's HUD elements keep between frames: when the
//! current weapon came up (the weapon label's selection window), the low-health overlay's pulse
//! (Ghidra G7), the damage-direction slots (G7), and the match tallies the scoreboard shows.
//! Client-side presentation only: nothing here feeds the sim.

use crate::sim::PlayerSim;

/// G7: the low-health overlay (`CG_PulseLowHealthOverlay`, `t6mp` 0x004dabe0) and its dvars.
pub const PULSE_START: f32 = 0.66;
pub const PULSE_STOP: f32 = 0.9;
pub const PHASE_MS: [f32; 3] = [150.0, 320.0, 400.0];
pub const PEAKS: [f32; 4] = [1.0, 1.0, 1.0, 0.3];
pub const END_MS: f32 = 700.0;
pub const REGEN_PAUSE_MS: f32 = 8000.0;

/// G7: the damage-direction indicators: 8 slots, 2000 ms each, ±10° of jitter.
pub const INDICATOR_SLOTS: usize = 8;
pub const INDICATOR_MS: f32 = 2000.0;

/// The low-health overlay's state machine (G7 formulas, alpha lerped from `from` to `to` over the
/// current phase).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LowHealth {
    pub active: bool,
    pub phase: u8,
    pub pulses: u8,
    pub from: f32,
    pub to: f32,
    pub phase_start: f32,
    pub phase_ms: f32,
    pub pulse_start: f32,
    pub last_frac: f32,
}

impl LowHealth {
    /// The overlay's alpha at `now` (seconds).
    pub fn alpha(&self, now: f32) -> f32 {
        if self.phase_ms <= 0.0 {
            return self.to;
        }
        let t = ((now - self.phase_start) * 1000.0 / self.phase_ms).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * t
    }

    /// Advance with the health fraction `frac` at `now`.
    pub fn tick(&mut self, frac: f32, now: f32) {
        if frac < self.last_frac && frac < PULSE_START {
            self.pulse_start = now;
            self.pulses = 0;
        }
        self.last_frac = frac;
        if frac > PULSE_STOP && self.active {
            self.active = false;
            self.from = self.alpha(now);
            self.to = 0.0;
            self.phase_start = now;
            self.phase_ms = END_MS;
            self.phase = 0;
            return;
        }
        if now < self.phase_start + self.phase_ms / 1000.0 {
            return;
        }
        if frac < PULSE_START || self.active {
            self.active = true;
            self.from = self.to;
            self.phase_start = now;
            // Each phase heads for the current peak (`phaseTwo/Three_toAlphaMultiplier` are 1;
            // where `phaseOne_toAlphaAdd` applies is unsettled in G7, so it is not added).
            let n = (self.pulses as usize).min(PEAKS.len() - 1);
            self.to = PEAKS[n].clamp(0.0, 1.0);
            match self.phase {
                0 => {
                    self.phase_ms = PHASE_MS[0];
                    self.phase = 1;
                }
                1 => {
                    self.phase_ms = PHASE_MS[1];
                    self.phase = 2;
                }
                _ => {
                    self.phase_ms = PHASE_MS[2];
                    self.phase = 0;
                    let cycle = PHASE_MS.iter().sum::<f32>() * 3.0;
                    if now * 1000.0 >= self.pulse_start * 1000.0 + REGEN_PAUSE_MS - cycle && (self.pulses as usize) < PEAKS.len() {
                        self.pulses += 1;
                    }
                }
            }
        }
    }
}

/// One damage-direction slot: when it started (seconds) and the world yaw the damage came from.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Indicator {
    pub start: f32,
    pub yaw_deg: f32,
    pub live: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HudLui {
    /// Seconds since the client started (presentation only).
    pub clock: f32,
    /// A hash of the weapon in hand and when it came up.
    pub weapon_key: u64,
    pub weapon_since: f32,
    pub low_health: LowHealth,
    pub indicators: [Indicator; INDICATOR_SLOTS],
    /// The local player's kills and deaths this match (the scoreboard row).
    pub kills: u32,
    pub deaths: u32,
    /// BO2's zoom direction (`cg+0x4ae38`, [`crate::weapons::scope::zooming_in`]): picks the
    /// weapon's in- or out-window for the reticle zoom.
    pub ads_zoom_in: bool,
    /// This frame's viewmodel is not drawn: the reticle zoom is up, tested with the direction as it
    /// stood *before* this frame's update (`FUN_005374c0` tests, then `FUN_0052e170` updates).
    pub gun_under_overlay: bool,
}

fn hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

impl HudLui {
    pub fn tick(&mut self, dt: f32, p: Option<&PlayerSim>) {
        self.clock += dt;
        if let Some(p) = p {
            let key = hash(&p.arsenal.current().def.id);
            if key != self.weapon_key {
                self.weapon_key = key;
                self.weapon_since = self.clock;
            }
            let frac = if p.max_health > 0.0 { p.health / p.max_health } else { 1.0 };
            self.low_health.tick(frac, self.clock);
            let ads = p.arsenal.ads;
            self.gun_under_overlay = crate::weapons::scope::reticle_zoom(p.arsenal.current_weapon(), ads, self.ads_zoom_in).is_some();
            self.ads_zoom_in = crate::weapons::scope::zooming_in(self.ads_zoom_in, ads);
        } else {
            self.gun_under_overlay = false;
        }
    }

    /// Milliseconds since the current weapon came up.
    pub fn weapon_ms(&self) -> f32 {
        (self.clock - self.weapon_since) * 1000.0
    }

    /// Damage from world yaw `yaw_deg`: refill the oldest slot (G7). `jitter` in -0.5..0.5 is
    /// the engine's `random() - 0.5`, ±10°.
    pub fn damage_from(&mut self, yaw_deg: f32, jitter: f32) {
        let slot = (0..INDICATOR_SLOTS)
            .min_by(|&a, &b| {
                let ka = if self.indicators[a].live { self.indicators[a].start } else { f32::NEG_INFINITY };
                let kb = if self.indicators[b].live { self.indicators[b].start } else { f32::NEG_INFINITY };
                ka.total_cmp(&kb)
            })
            .unwrap_or(0);
        self.indicators[slot] = Indicator { start: self.clock, yaw_deg: (yaw_deg + jitter * 20.0).rem_euclid(360.0), live: true };
    }

    /// The live indicators with their alpha: opaque for the first half, then linear to 0 (G7).
    pub fn live_indicators(&self) -> Vec<(f32, f32)> {
        self.indicators
            .iter()
            .filter(|i| i.live)
            .filter_map(|i| {
                let age = (self.clock - i.start) * 1000.0;
                (age > 0.0 && age < INDICATOR_MS).then(|| (i.yaw_deg, (2.0 - 2.0 * age / INDICATOR_MS).min(1.0)))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_health_pulses_below_two_thirds_and_fades_above_nine_tenths() {
        let mut lh = LowHealth { last_frac: 1.0, ..Default::default() };
        lh.tick(0.5, 0.0);
        assert!(lh.active);
        assert_eq!((lh.phase, lh.phase_ms), (1, 150.0));
        assert_eq!(lh.alpha(0.075), 0.5, "halfway up the 150 ms first pulse");
        lh.tick(0.5, 0.15);
        assert_eq!((lh.phase, lh.phase_ms), (2, 320.0));
        lh.tick(0.95, 0.2);
        assert!(!lh.active);
        assert_eq!(lh.phase_ms, END_MS);
        assert_eq!(lh.alpha(0.2 + 0.7), 0.0);
    }

    #[test]
    fn indicators_last_two_seconds_and_fade_in_the_second() {
        let mut h = HudLui::default();
        h.damage_from(90.0, 0.0);
        h.clock = 0.5;
        assert_eq!(h.live_indicators(), vec![(90.0, 1.0)]);
        h.clock = 1.5;
        assert_eq!(h.live_indicators(), vec![(90.0, 0.5)]);
        h.clock = 2.1;
        assert!(h.live_indicators().is_empty());
        // Nine hits keep eight slots, the oldest replaced.
        for k in 0..9 {
            h.clock = 3.0 + k as f32 * 0.01;
            h.damage_from(k as f32, 0.0);
        }
        assert_eq!(h.indicators.iter().filter(|i| i.live).count(), INDICATOR_SLOTS);
        assert!(h.indicators.iter().all(|i| i.yaw_deg != 0.0), "the first of the nine was replaced");
    }
}
