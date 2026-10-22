/*
** Conquer Ops - Zero Hour gameplay core: a weapon's feel: gun kick, sway, viewmodel offsets, crosshair.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

//! The parts of BO2's `WeaponDef` that decide how a gun feels rather than what it does: the gun
//! kick spring (Ghidra G5), view sway and the viewmodel's stance/movement offsets (G4) and the
//! weapon's own crosshair (G6). Values are BO2's own, converted from the player's install into the
//! weapon table; distances stay in BO2's inches (the viewmodel lives in BO2's units) and angles in
//! degrees.

/// The gun kick of one aim state (`f{Ads,Hip}GunKick*`; G5): the per-shot velocity impulse and
/// the spring that brings the gun back.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GunKick {
    pub pitch: (f32, f32),
    pub yaw: (f32, f32),
    pub accel: f32,
    pub speed_max: f32,
    pub speed_decay: f32,
    pub static_decay: f32,
    /// The first this-many bullets of a string kick at `reduced_percent` percent.
    pub reduced_bullets: f32,
    pub reduced_percent: f32,
}

impl GunKick {
    pub(crate) fn format(&self) -> String {
        nums(&[
            self.pitch.0,
            self.pitch.1,
            self.yaw.0,
            self.yaw.1,
            self.accel,
            self.speed_max,
            self.speed_decay,
            self.static_decay,
            self.reduced_bullets,
            self.reduced_percent,
        ])
    }
    pub(crate) fn from_nums(n: &[f32]) -> GunKick {
        GunKick {
            pitch: (n[0], n[1]),
            yaw: (n[2], n[3]),
            accel: n[4],
            speed_max: n[5],
            speed_decay: n[6],
            static_decay: n[7],
            reduced_bullets: n[8],
            reduced_percent: n[9],
        }
    }
}

/// View sway (`sway*` hip, `adsSway*` aimed; G4). BO2's aimed set has no horizontal/vertical
/// scales; the hip ones apply there too.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sway {
    pub max_angle: f32,
    pub lerp_speed: f32,
    pub pitch_scale: f32,
    pub yaw_scale: f32,
    pub horiz_scale: f32,
    pub vert_scale: f32,
}

impl Sway {
    pub(crate) fn format(&self) -> String {
        nums(&[self.max_angle, self.lerp_speed, self.pitch_scale, self.yaw_scale, self.horiz_scale, self.vert_scale])
    }
    pub(crate) fn from_nums(n: &[f32]) -> Sway {
        Sway { max_angle: n[0], lerp_speed: n[1], pitch_scale: n[2], yaw_scale: n[3], horiz_scale: n[4], vert_scale: n[5] }
    }
}

/// The viewmodel's stance and movement offsets (inches) and rotations (degrees), their blend
/// rates and minimum speeds, and the ADS pitch and bob factor (G4, `FUN_008efa70`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewPos {
    pub stand_move: [f32; 3],
    pub stand_rot: [f32; 3],
    pub ducked_ofs: [f32; 3],
    pub ducked_move: [f32; 3],
    pub ducked_rot: [f32; 3],
    pub prone_ofs: [f32; 3],
    pub prone_move: [f32; 3],
    pub prone_rot: [f32; 3],
    pub strafe_move: [f32; 3],
    pub strafe_rot: [f32; 3],
    pub sprint_ofs: [f32; 3],
    pub sprint_rot: [f32; 3],
    pub sprint_bob: [f32; 2],
    pub move_rate: f32,
    pub prone_move_rate: f32,
    pub rot_rate: f32,
    pub prone_rot_rate: f32,
    pub stand_min_speed: f32,
    pub ducked_min_speed: f32,
    pub prone_min_speed: f32,
    pub ads_aim_pitch: f32,
    pub ads_bob_factor: f32,
}

/// The weapon's own crosshair (G6): the side and centre materials, their sizes and minimum
/// offset (640x480 units), the side position share, the ADS fade fractions and flags.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reticle {
    pub center: String,
    pub side: String,
    pub center_size: f32,
    pub side_size: f32,
    pub min_ofs: f32,
    pub side_pos: f32,
    pub ads_in_frac: f32,
    pub ads_out_frac: f32,
    pub keep_when_ads: bool,
    pub color_change: bool,
}

impl Reticle {
    pub(crate) fn format(&self) -> String {
        nums(&[
            self.center_size,
            self.side_size,
            self.min_ofs,
            self.side_pos,
            self.ads_in_frac,
            self.ads_out_frac,
            self.keep_when_ads as u8 as f32,
            self.color_change as u8 as f32,
        ])
    }
    pub(crate) fn set_nums(&mut self, n: &[f32]) {
        (self.center_size, self.side_size, self.min_ofs, self.side_pos) = (n[0], n[1], n[2], n[3]);
        (self.ads_in_frac, self.ads_out_frac) = (n[4], n[5]);
        (self.keep_when_ads, self.color_change) = (n[6] != 0.0, n[7] != 0.0);
    }
    /// `center side` with `-` for a missing material.
    pub(crate) fn format_art(&self) -> String {
        let w = |s: &str| if s.is_empty() { "-".to_string() } else { s.to_string() };
        format!("{} {}", w(&self.center), w(&self.side))
    }
    pub(crate) fn set_art(&mut self, text: &str) {
        let mut words = text.split_whitespace().map(|w| if w == "-" { String::new() } else { w.to_string() });
        self.center = words.next().unwrap_or_default();
        self.side = words.next().unwrap_or_default();
    }
}

/// Space-separated numbers, the table's format.
pub(crate) fn nums(v: &[f32]) -> String {
    v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ")
}

pub(crate) fn v3(n: &[f32]) -> [f32; 3] {
    [n[0], n[1], n[2]]
}

/// BO2 optics whose personalization art is the `mtl_t6_attach_optic_*_reticle_*` set
/// (`lua-dump/assets/code_post_gfx_mp.tsv:1073-1104`; materials `:2023-2054`).
pub const RETICLE_OPTICS: &[&str] = &["reflex", "holo", "acog"];

/// The weapon's first reflex/holo/acog optic.
pub fn optic_of(attachments: &[String]) -> Option<&'static str> {
    RETICLE_OPTICS.iter().copied().find(|o| attachments.iter().any(|a| a.as_str() == *o))
}

/// The stock reticle image for `reticle_0..10` through `optic`: `reticle_0` is `_01`,
/// `reticle_10` is `_11` (`cacreticles.lua:1150-1240,1390-1470`).
pub fn reticle_art(optic: &str, id: &str) -> Option<String> {
    let n: u32 = id.strip_prefix("reticle_")?.parse().ok()?;
    (n <= 10 && RETICLE_OPTICS.contains(&optic)).then(|| format!("mtl_t6_attach_optic_{optic}_reticle_{:02}", n + 1))
}
