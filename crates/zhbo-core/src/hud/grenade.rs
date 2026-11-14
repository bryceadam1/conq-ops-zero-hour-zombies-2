//! BO2's grenade danger indicator and its friendly names over the heads of allies.
//!
//! * Grenades (t6mp `FUN_004ae7b0` collects, `FUN_004ae5b0` ranges, `FUN_004aea40`/`FUN_004ae300`/
//!   `FUN_004ae0b0` draw): a live projectile of a grenade, projectile or mine def with
//!   `timedDetonation` whose `projExplosion` is grenade, heavy or fire; an enemy's or the player's
//!   own, never an ally's; resting, stuck or slower than `bg_maxGrenadeIndicatorSpeed` (100 in/s);
//!   within its range (`iIndicatorRadius`, else the larger of `cg_hudGrenadeIconMaxRangeFrag` 250
//!   and its explosion radius); no lower than 104 under the eye and no higher than the player box's
//!   top (70 over his feet). Up to 32 at once. Each draws its icon (`indicatorIcon`, else
//!   `hud_grenadeicon`, 25 × 25) 50 units out from the centre towards the grenade (480-line space),
//!   and `hud_grenadepointer` (25 × 12, pivot 12, 27) turned with it, white, pulsing at 1.7 Hz:
//!   `clamp(1.075 + 0.775 sin(2π·1.7·t), 0, 1)`.
//! * Names (`FUN_004b1b60`): an allied General's name over his head while seen ([`super::feedback`]
//!   keeps the fade), `cg_overheadNamesSize` 0.5 of the 48-line menu font, full size within
//!   `cg_overheadNamesNearDist` 64 inches, down to `cg_overheadNamesFarScale` 0.7 at
//!   `cg_overheadNamesFarDist` 512, centred, in our team's colour over its shadow.

use super::Prim;
use crate::math::Vec3;
use crate::sim::{PlayerSim, Sim};
use crate::units::inches;
use crate::weapons::WeapType;

/// The `cg_hudGrenade*` dvars (`FUN_004d2350`), in BO2's 640 × 480 virtual screen.
pub const ICON_W: f32 = 25.0;
pub const ICON_H: f32 = 25.0;
pub const ICON_OFFSET: f32 = 50.0;
pub const POINTER_W: f32 = 25.0;
pub const POINTER_H: f32 = 12.0;
pub const POINTER_PIVOT: (f32, f32) = (12.0, 27.0);
/// `cg_hudGrenadeIconMaxRangeFrag` (inches).
pub const MAX_RANGE_FRAG: f32 = 250.0;
/// `bg_maxGrenadeIndicatorSpeed` (inches/s).
pub const MAX_SPEED: f32 = 100.0;
/// `FUN_004ae5b0`'s height window: under the eye (`DAT_00b12974`) and the player box's top over
/// his feet (`DAT_00bd2448`), inches.
pub const BELOW_EYE: f32 = 104.0;
pub const BOX_TOP: f32 = 70.0;
/// `DAT_00fbe178 < 0x20`.
pub const MAX_SHOWN: usize = 32;
/// `projExplosion` grenade, heavy, fire (`weapProjExposion_t`).
const KINDS: [u8; 3] = [0, 6, 7];
/// `FUN_004aea40`'s pulse: `time · 2π/1000 · 1.7`, alpha `· 0.775 + 1.075`.
const PULSE_HZ: f32 = 1.7;

/// `cg_overheadNamesSize`, `NearDist`, `FarDist`, `FarScale` (inches).
pub const NAME_SIZE: f32 = 0.5;
pub const NAME_NEAR: f32 = 64.0;
pub const NAME_FAR: f32 = 512.0;
pub const NAME_FAR_SCALE: f32 = 0.7;
/// IW's menu font height per text scale, in 480-line units (as `hud::center`'s hints).
const FONT_480: f32 = 48.0;

/// One grenade to show: where it is and its icon's material.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub at: Vec3,
    pub icon: String,
}

/// The grenades the indicator shows to `me` (the presented seat, his General `p`); `ally(seat)`
/// is whether a seat is on his side.
pub fn collect(sim: &Sim, me: usize, p: &PlayerSim, ally: &dyn Fn(usize) -> bool) -> Vec<Shown> {
    let eye = p.body.eye();
    let mut out = Vec::new();
    for (seat, g) in &sim.projectiles {
        let d = g.def;
        let ind = &d.indicator;
        if !matches!(d.traits.weap_type, WeapType::Grenade | WeapType::Projectile | WeapType::Mine) || !d.timed_detonation || !KINDS.contains(&ind.proj_explosion) {
            continue;
        }
        if *seat != me && ally(*seat) {
            continue;
        }
        let still = g.resting || g.stuck_to.is_some() || g.vel.length() <= inches(MAX_SPEED);
        if !still {
            continue;
        }
        let range = if ind.radius > 0.0 { inches(ind.radius) } else { inches(MAX_RANGE_FRAG).max(d.explosion_radius) };
        if (g.pos - eye).length() > range {
            continue;
        }
        let dz = g.pos.z - eye.z;
        if dz < -inches(BELOW_EYE) || g.pos.z > p.body.pos.z + inches(BOX_TOP) {
            continue;
        }
        out.push(Shown { at: g.pos, icon: if ind.icon.is_empty() { "hud_grenadeicon".to_string() } else { ind.icon.clone() } });
        if out.len() == MAX_SHOWN {
            break;
        }
    }
    out
}

/// The pulse's alpha at `now` seconds.
pub fn pulse(now: f32) -> f32 {
    (1.075 + 0.775 * (now * std::f32::consts::TAU * PULSE_HZ).sin()).clamp(0.0, 1.0)
}

/// Draw the indicators: the view at `eye` facing `view_yaw` (radians, counter-clockwise).
pub fn draw(prims: &mut Vec<Prim>, w: f32, h: f32, eye: Vec3, view_yaw: f32, shown: &[Shown], now: f32) {
    let u = h / 480.0;
    let (cx, cy) = (w * 0.5, h * 0.5);
    let argb = super::frame::argb((1.0, 1.0, 1.0), pulse(now));
    for g in shown {
        let d = g.at - eye;
        // The grenade's yaw off the view, degrees counter-clockwise (to the left).
        let a = (d.y.atan2(d.x) - view_yaw).to_degrees();
        let (sin, cos) = a.to_radians().sin_cos();
        let (px, py) = (cx - sin * ICON_OFFSET * u, cy - cos * ICON_OFFSET * u);
        // The pointer, turned by −a about the point (clockwise positive on screen).
        let (rs, rc) = (-a).to_radians().sin_cos();
        let turn = |x: f32, y: f32| (px + (x * rc - y * rs) * u, py + (x * rs + y * rc) * u);
        let (x0, x1, y0, y1) = (-POINTER_PIVOT.0, POINTER_W - POINTER_PIVOT.0, -POINTER_PIVOT.1, POINTER_H - POINTER_PIVOT.1);
        let corners = [turn(x0, y0), turn(x1, y0), turn(x1, y1), turn(x0, y1)];
        prims.push(Prim::Quad { name: crate::lui::materials::image("hud_grenadepointer"), corners, argb, fallback: Vec::new() });
        let (iw, ih) = (ICON_W * u, ICON_H * u);
        prims.push(Prim::Image { name: crate::lui::materials::image(&g.icon), x: px - iw * 0.5, y: py - ih * 0.5, w: iw, h: ih, argb, fallback: Vec::new() });
    }
}

/// The name's scale at `dist` (Zero Hour units) from the eye.
pub fn name_scale(dist: f32) -> f32 {
    let (near, far) = (inches(NAME_NEAR), inches(NAME_FAR));
    if dist <= near {
        1.0
    } else if dist >= far {
        NAME_FAR_SCALE
    } else {
        let t = (dist - near) / (far - near);
        t * NAME_FAR_SCALE + (1.0 - t)
    }
}

/// Draw one General's name at screen point `(x, y)` (his head), `dist` away, in `color`, at `alpha`.
pub fn draw_name(prims: &mut Vec<Prim>, h: f32, x: f32, y: f32, dist: f32, name: &str, color: (f32, f32, f32), alpha: f32) {
    if alpha <= 0.0 || name.is_empty() {
        return;
    }
    let size = FONT_480 * NAME_SIZE * name_scale(dist) * h / 480.0;
    let width = crate::lui::font::width(crate::lui::font::Family::Condensed, name, size);
    let top = y - size;
    let shadow = h / 480.0;
    let a = super::frame::argb((0.0, 0.0, 0.0), alpha);
    crate::lui::font::text(prims, crate::lui::font::Family::Condensed, name, x - width * 0.5 + shadow, top + shadow, size, crate::lui::font::Align::Left, a);
    let c = super::frame::argb(color, alpha);
    crate::lui::font::text(prims, crate::lui::font::Family::Condensed, name, x - width * 0.5, top, size, crate::lui::font::Align::Left, c);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_sits_fifty_lines_out_towards_the_grenade() {
        let mut prims = Vec::new();
        let shown = vec![Shown { at: Vec3::new(0.0, 10.0, 0.0), icon: "hud_grenadeicon".into() }];
        // Facing +X, the grenade on the left (+Y, 90° counter-clockwise): left of the centre.
        draw(&mut prims, 640.0, 480.0, Vec3::ZERO, 0.0, &shown, 0.0);
        let Prim::Image { x, y, w, h, .. } = &prims[1] else { panic!("{prims:?}") };
        assert!(((x + w * 0.5) - 270.0).abs() < 1e-3 && ((y + h * 0.5) - 240.0).abs() < 1e-3, "{x} {y}");
        // The pointer points outward: its far edge beyond the icon, left of it.
        let Prim::Quad { corners, .. } = &prims[0] else { panic!() };
        assert!(corners.iter().all(|c| c.0 < 270.0 - 14.0), "{corners:?}");
    }

    #[test]
    fn the_pulse_and_the_name_scale_follow_the_dvars() {
        assert_eq!(pulse(0.0), 1.0);
        assert!(pulse(0.75 / PULSE_HZ) < 0.31);
        assert_eq!(name_scale(0.0), 1.0);
        assert!((name_scale(inches(512.0)) - 0.7).abs() < 1e-6);
        assert!((name_scale(inches(288.0)) - 0.85).abs() < 1e-5);
    }
}
