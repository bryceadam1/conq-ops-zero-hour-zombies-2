//! Black Ops 2-style weapons: fire modes, magazines, reloads, ADS, spread bloom and damage falloff.
//!
//! Numbers are BO2-*inspired* multiplayer values (damage on a 100 HP scale, ranges in inches),
//! converted to Zero Hour units. They are tuning data written for this mod, not extracted files.
//!
//! Everything here runs inside Zero Hour's lockstep logic, so randomness comes from [`Rng`], which is
//! seeded identically on every client.

use crate::input::buttons;
use crate::units::{inches, LOGIC_DT};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireMode {
    Auto,
    Semi,
    Burst(u8),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponDef {
    pub id: &'static str,
    pub display_name: &'static str,
    pub mode: FireMode,
    pub rpm: f32,
    pub mag_size: u16,
    pub start_reserve: u16,
    pub reload_time: f32,
    pub reload_empty_time: f32,
    pub damage_near: f32,
    pub damage_far: f32,
    /// Full damage up to this distance (ZH units).
    pub range_near: f32,
    /// Minimum damage from this distance on.
    pub range_far: f32,
    /// Hitscan stops here.
    pub max_range: f32,
    pub head_multiplier: f32,
    /// Hip-fire cone half-angle in degrees (min, max bloom).
    pub hip_spread: (f32, f32),
    pub ads_spread: f32,
    pub bloom_per_shot: f32,
    pub bloom_decay_per_sec: f32,
    pub ads_time: f32,
    /// View zoom when fully aimed (FOV multiplier, < 1 zooms in).
    pub ads_fov_scale: f32,
    pub move_scale: f32,
    pub ads_move_scale: f32,
    pub recoil_pitch_deg: f32,
    pub recoil_yaw_deg: f32,
    pub raise_time: f32,
    /// Zero Hour WeaponTemplate used for muzzle flash, tracer and sound (no damage).
    pub zh_fx_weapon: &'static str,
}

impl WeaponDef {
    pub fn shot_interval(&self) -> f32 {
        60.0 / self.rpm
    }

    pub fn damage_at(&self, distance: f32) -> f32 {
        if distance <= self.range_near {
            self.damage_near
        } else if distance >= self.range_far {
            self.damage_far
        } else {
            let t = (distance - self.range_near) / (self.range_far - self.range_near);
            self.damage_near + (self.damage_far - self.damage_near) * t
        }
    }
}

pub const MTAR: WeaponDef = WeaponDef {
    id: "mtar",
    display_name: "MTAR",
    mode: FireMode::Auto,
    rpm: 750.0,
    mag_size: 30,
    start_reserve: 120,
    reload_time: 2.1,
    reload_empty_time: 2.8,
    damage_near: 40.0,
    damage_far: 30.0,
    range_near: inches(1000.0),
    range_far: inches(2000.0),
    max_range: 900.0,
    head_multiplier: 1.1,
    hip_spread: (3.0, 7.0),
    ads_spread: 0.15,
    bloom_per_shot: 0.5,
    bloom_decay_per_sec: 8.0,
    ads_time: 0.25,
    ads_fov_scale: 0.75,
    move_scale: 1.0,
    ads_move_scale: 0.6,
    recoil_pitch_deg: 0.55,
    recoil_yaw_deg: 0.25,
    raise_time: 0.5,
    zh_fx_weapon: "ZHBO_MTAR_FX",
};

pub const B23R: WeaponDef = WeaponDef {
    id: "b23r",
    display_name: "B23R",
    mode: FireMode::Burst(3),
    rpm: 937.0,
    mag_size: 15,
    start_reserve: 60,
    reload_time: 1.5,
    reload_empty_time: 1.9,
    damage_near: 40.0,
    damage_far: 25.0,
    range_near: inches(500.0),
    range_far: inches(1100.0),
    max_range: 600.0,
    head_multiplier: 1.1,
    hip_spread: (2.0, 5.0),
    ads_spread: 0.3,
    bloom_per_shot: 0.6,
    bloom_decay_per_sec: 9.0,
    ads_time: 0.15,
    ads_fov_scale: 0.85,
    move_scale: 1.07,
    ads_move_scale: 0.7,
    recoil_pitch_deg: 0.8,
    recoil_yaw_deg: 0.3,
    raise_time: 0.35,
    zh_fx_weapon: "ZHBO_B23R_FX",
};

/// The General's loadout: primary, secondary.
pub const LOADOUT: [&WeaponDef; 2] = [&MTAR, &B23R];

/// BO2 knife melee: a one-hit kill on infantry within lunge range.
pub const MELEE_DAMAGE: f32 = 135.0;
pub const MELEE_RANGE: f32 = inches(64.0);
pub const MELEE_COOLDOWN: f32 = 0.9;
pub const MELEE_FX_WEAPON: &str = "ZHBO_Knife_FX";

/// xorshift32: tiny, deterministic, identical on every client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(seed.max(1))
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform in [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform in [-1, 1).
    pub fn signed(&mut self) -> f32 {
        self.next_f32() * 2.0 - 1.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeaponState {
    pub def: &'static WeaponDef,
    pub ammo: u16,
    pub reserve: u16,
    /// Seconds until the next round may fire (can go negative within a frame for high RPM).
    pub cooldown: f32,
    pub reload_left: f32,
    pub burst_left: u8,
    pub bloom: f32,
}

impl WeaponState {
    pub fn new(def: &'static WeaponDef) -> Self {
        Self {
            def,
            ammo: def.mag_size,
            reserve: def.start_reserve,
            cooldown: 0.0,
            reload_left: 0.0,
            burst_left: 0,
            bloom: 0.0,
        }
    }

    pub fn is_reloading(&self) -> bool {
        self.reload_left > 0.0
    }

    pub fn can_reload(&self) -> bool {
        !self.is_reloading() && self.ammo < self.def.mag_size && self.reserve > 0
    }

    pub fn start_reload(&mut self) {
        if self.can_reload() {
            self.reload_left = if self.ammo == 0 { self.def.reload_empty_time } else { self.def.reload_time };
            self.burst_left = 0;
        }
    }

    fn finish_reload(&mut self) {
        let need = self.def.mag_size - self.ammo;
        let take = need.min(self.reserve);
        self.ammo += take;
        self.reserve -= take;
    }

    /// Current cone half-angle in degrees for a given ADS fraction.
    pub fn spread_deg(&self, ads: f32) -> f32 {
        let (min, max) = self.def.hip_spread;
        let hip = (min + self.bloom).min(max);
        hip + (self.def.ads_spread - hip) * ads
    }
}

/// What the weapon layer wants the logic to do this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WeaponAction {
    /// Fire one round with the given cone (degrees) and random offsets in [-1, 1).
    Fire { spread_deg: f32, jitter: (f32, f32), recoil: (f32, f32) },
    Melee,
    DryFire,
    ReloadStarted,
    ReloadFinished,
    Swapped,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arsenal {
    pub weapons: Vec<WeaponState>,
    pub active: usize,
    pub raise_left: f32,
    pub ads: f32,
    pub melee_cooldown: f32,
    held_last: u32,
}

impl Arsenal {
    pub fn new() -> Self {
        Self {
            weapons: LOADOUT.iter().map(|d| WeaponState::new(d)).collect(),
            active: 0,
            raise_left: LOADOUT[0].raise_time,
            ads: 0.0,
            melee_cooldown: 0.0,
            held_last: 0,
        }
    }

    pub fn current(&self) -> &WeaponState {
        &self.weapons[self.active]
    }

    pub fn current_mut(&mut self) -> &mut WeaponState {
        &mut self.weapons[self.active]
    }

    pub fn is_busy(&self) -> bool {
        self.raise_left > 0.0 || self.current().is_reloading()
    }

    /// Movement speed multiplier from the held weapon and ADS state.
    pub fn move_scale(&self) -> f32 {
        let d = self.current().def;
        d.move_scale + (d.ads_move_scale - d.move_scale) * self.ads
    }

    /// Advance one logic frame. `sprinting` blocks firing and ADS.
    pub fn step(&mut self, held: u32, pressed: u32, sprinting: bool, rng: &mut Rng, out: &mut Vec<WeaponAction>) {
        let dt = LOGIC_DT;
        let pressed = pressed | (held & !self.held_last);
        self.held_last = held;

        self.melee_cooldown = (self.melee_cooldown - dt).max(0.0);
        self.raise_left = (self.raise_left - dt).max(0.0);

        for w in &mut self.weapons {
            w.bloom = (w.bloom - w.def.bloom_decay_per_sec * dt).max(0.0);
        }

        // Swap.
        if pressed & buttons::SWAP_WEAPON != 0 && self.weapons.len() > 1 {
            self.current_mut().reload_left = 0.0; // swapping cancels a reload, like BO2
            self.current_mut().burst_left = 0;
            self.active = (self.active + 1) % self.weapons.len();
            self.raise_left = self.current().def.raise_time;
            self.ads = 0.0;
            out.push(WeaponAction::Swapped);
        }

        // Melee interrupts everything except a raise.
        if pressed & buttons::MELEE != 0 && self.melee_cooldown <= 0.0 && self.raise_left <= 0.0 {
            self.melee_cooldown = MELEE_COOLDOWN;
            self.current_mut().burst_left = 0;
            out.push(WeaponAction::Melee);
            return;
        }

        // Reload progress / requests.
        {
            let w = self.current_mut();
            if w.reload_left > 0.0 {
                w.reload_left -= dt;
                if w.reload_left <= 0.0 {
                    w.reload_left = 0.0;
                    w.finish_reload();
                    out.push(WeaponAction::ReloadFinished);
                }
            }
        }
        if pressed & buttons::RELOAD != 0 && self.current().can_reload() {
            self.current_mut().start_reload();
            out.push(WeaponAction::ReloadStarted);
        }

        // ADS blend.
        let def = self.current().def;
        let wants_ads = held & buttons::ADS != 0 && !sprinting && self.raise_left <= 0.0 && !self.current().is_reloading();
        let rate = dt / def.ads_time.max(1e-3);
        self.ads = if wants_ads { (self.ads + rate).min(1.0) } else { (self.ads - rate).max(0.0) };

        // Firing.
        let ads = self.ads;
        let raise_left = self.raise_left;
        let w = self.current_mut();
        w.cooldown -= dt;
        let trigger = match def.mode {
            FireMode::Auto => held & buttons::FIRE != 0,
            FireMode::Semi => pressed & buttons::FIRE != 0,
            FireMode::Burst(n) => {
                if pressed & buttons::FIRE != 0 && w.burst_left == 0 {
                    w.burst_left = n;
                }
                w.burst_left > 0
            }
        };
        if !trigger || sprinting || raise_left > 0.0 || w.is_reloading() {
            if w.cooldown < 0.0 {
                w.cooldown = 0.0;
            }
            if sprinting || w.is_reloading() {
                w.burst_left = 0;
            }
            return;
        }
        if w.ammo == 0 {
            w.burst_left = 0;
            if pressed & buttons::FIRE != 0 {
                if w.can_reload() {
                    w.start_reload();
                    out.push(WeaponAction::ReloadStarted);
                } else {
                    out.push(WeaponAction::DryFire);
                }
            }
            if w.cooldown < 0.0 {
                w.cooldown = 0.0;
            }
            return;
        }
        let mut fired_semi = false;
        while w.cooldown <= 0.0 && w.ammo > 0 {
            if matches!(def.mode, FireMode::Semi) && fired_semi {
                break;
            }
            let spread = w.spread_deg(ads);
            let jitter = (rng.signed(), rng.signed());
            let recoil = (def.recoil_pitch_deg * (0.8 + 0.4 * rng.next_f32()), def.recoil_yaw_deg * rng.signed());
            out.push(WeaponAction::Fire { spread_deg: spread, jitter, recoil });
            w.ammo -= 1;
            w.bloom += def.bloom_per_shot;
            w.cooldown += def.shot_interval();
            fired_semi = true;
            if let FireMode::Burst(_) = def.mode {
                w.burst_left = w.burst_left.saturating_sub(1);
                if w.burst_left == 0 {
                    // BO2 bursts have a short gap before the next burst.
                    w.cooldown += def.shot_interval() * 1.5;
                    break;
                }
            }
        }
        if w.ammo == 0 && w.reserve > 0 {
            // BO2 auto-reloads when the magazine empties.
            w.start_reload();
            out.push(WeaponAction::ReloadStarted);
        }
    }
}

impl Default for Arsenal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> Arsenal {
        let mut a = Arsenal::new();
        a.raise_left = 0.0;
        a
    }

    fn frames(a: &mut Arsenal, held: u32, n: usize, rng: &mut Rng) -> Vec<WeaponAction> {
        let mut out = Vec::new();
        for _ in 0..n {
            a.step(held, 0, false, rng, &mut out);
        }
        out
    }

    fn shots(actions: &[WeaponAction]) -> usize {
        actions.iter().filter(|a| matches!(a, WeaponAction::Fire { .. })).count()
    }

    #[test]
    fn auto_fire_matches_rpm() {
        let mut a = ready();
        let mut rng = Rng::new(1);
        let out = frames(&mut a, buttons::FIRE, 60, &mut rng); // 2 seconds
        let fired = shots(&out);
        // 750 RPM for 2 s = 25 rounds, but the 30-round mag allows that.
        assert!((24..=26).contains(&fired), "fired {fired}");
    }

    #[test]
    fn empty_mag_auto_reloads_and_refills() {
        let mut a = ready();
        let mut rng = Rng::new(2);
        let out = frames(&mut a, buttons::FIRE, 90, &mut rng);
        assert_eq!(shots(&out), 30);
        assert!(out.contains(&WeaponAction::ReloadStarted));
        let out = frames(&mut a, 0, 100, &mut rng);
        assert!(out.contains(&WeaponAction::ReloadFinished));
        assert_eq!(a.current().ammo, 30);
        assert_eq!(a.current().reserve, 90);
    }

    #[test]
    fn burst_fires_three_per_press() {
        let mut a = ready();
        let mut rng = Rng::new(3);
        let mut out = Vec::new();
        a.step(buttons::SWAP_WEAPON, 0, false, &mut rng, &mut out);
        a.raise_left = 0.0;
        out.clear();
        a.step(buttons::FIRE, buttons::FIRE, false, &mut rng, &mut out);
        for _ in 0..20 {
            a.step(buttons::FIRE, 0, false, &mut rng, &mut out);
        }
        assert_eq!(shots(&out), 3, "one burst while holding");
    }

    #[test]
    fn sprinting_blocks_fire_and_ads() {
        let mut a = ready();
        let mut rng = Rng::new(4);
        let mut out = Vec::new();
        for _ in 0..30 {
            a.step(buttons::FIRE | buttons::ADS, 0, true, &mut rng, &mut out);
        }
        assert_eq!(shots(&out), 0);
        assert_eq!(a.ads, 0.0);
    }

    #[test]
    fn ads_tightens_spread_and_falloff_is_monotonic() {
        let mut a = ready();
        let mut rng = Rng::new(5);
        let hip = a.current().spread_deg(0.0);
        frames(&mut a, buttons::ADS, 15, &mut rng);
        assert_eq!(a.ads, 1.0);
        assert!(a.current().spread_deg(a.ads) < hip * 0.2);
        let d = &MTAR;
        assert_eq!(d.damage_at(0.0), 40.0);
        assert!(d.damage_at(250.0) < 40.0 && d.damage_at(250.0) > 30.0);
        assert_eq!(d.damage_at(10_000.0), 30.0);
    }

    #[test]
    fn rng_is_deterministic() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
        let f = Rng::new(7).next_f32();
        assert!((0.0..1.0).contains(&f));
    }
}
