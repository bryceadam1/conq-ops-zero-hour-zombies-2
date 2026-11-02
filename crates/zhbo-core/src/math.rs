//! Small vector math in Zero Hour world space: +Z is up, angles are radians, yaw 0 faces +X and
//! grows counter-clockwise (the same convention as `Object::getOrientation`).

use std::f32::consts::PI;
use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.0 };
    pub const UP: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 1.0 };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn length_xy(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn normalized_or_zero(self) -> Vec3 {
        let len = self.length();
        if len > 1e-6 {
            self * (1.0 / len)
        } else {
            Vec3::ZERO
        }
    }

    pub fn lerp(self, o: Vec3, t: f32) -> Vec3 {
        self + (o - self) * t
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Vec3) {
        *self = *self + o;
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl SubAssign for Vec3 {
    fn sub_assign(&mut self, o: Vec3) {
        *self = *self - o;
    }
}

impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}

/// Wrap an angle into (-PI, PI].
pub fn wrap_angle(mut a: f32) -> f32 {
    while a > PI {
        a -= 2.0 * PI;
    }
    while a <= -PI {
        a += 2.0 * PI;
    }
    a
}

/// Unit view direction for a yaw/pitch pair (pitch > 0 looks up).
pub fn dir_from_angles(yaw: f32, pitch: f32) -> Vec3 {
    let (sp, cp) = det::sin_cos(pitch);
    let (sy, cy) = det::sin_cos(yaw);
    Vec3::new(cp * cy, cp * sy, sp)
}

/// Horizontal forward and right vectors for a yaw.
pub fn basis_xy(yaw: f32) -> (Vec3, Vec3) {
    let (s, c) = det::sin_cos(yaw);
    (Vec3::new(c, s, 0.0), Vec3::new(s, -c, 0.0))
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Shortest-path angle interpolation.
pub fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    wrap_angle(a + wrap_angle(b - a) * t)
}

pub fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The transcendental functions of everything a logic step computes: a pure-Rust port of musl's
/// libm (the `libm` crate), so every lockstep peer computes the same bits. `f32::sin` and its kin
/// call the platform's C library (Apple's libm, the Windows UCRT with its CPU-feature paths,
/// glibc), whose last-place results differ between machines — a cross-platform or cross-CPU
/// desync the engine itself avoids with its own deterministic math (`USE_DETERMINISTIC_MATH`,
/// TheSuperHackers' GameMath). `+ - * /` and `sqrt` are IEEE-exact everywhere, and Rust never
/// fuses a multiply and an add on its own, so only these need routing. The sim's source files may
/// not call the `f32` methods (`the_sim_calls_no_platform_libm`, below).
pub mod det {
    #[inline]
    pub fn sin(x: f32) -> f32 {
        libm::sinf(x)
    }
    #[inline]
    pub fn cos(x: f32) -> f32 {
        libm::cosf(x)
    }
    /// `(sin x, cos x)`.
    #[inline]
    pub fn sin_cos(x: f32) -> (f32, f32) {
        libm::sincosf(x)
    }
    #[inline]
    pub fn tan(x: f32) -> f32 {
        libm::tanf(x)
    }
    #[inline]
    pub fn atan(x: f32) -> f32 {
        libm::atanf(x)
    }
    /// The angle of `(x, y)`, as `y.atan2(x)`.
    #[inline]
    pub fn atan2(y: f32, x: f32) -> f32 {
        libm::atan2f(y, x)
    }
    #[inline]
    pub fn asin(x: f32) -> f32 {
        libm::asinf(x)
    }
    #[inline]
    pub fn exp(x: f32) -> f32 {
        libm::expf(x)
    }
    /// `x` to the integer power `n` (`f32::powi` is a compiler builtin whose expansion is the
    /// backend's choice).
    #[inline]
    pub fn powi(x: f32, n: i32) -> f32 {
        libm::powf(x, n as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The presentation side of the core: per machine by design (the local client, the HUD, the
    /// class screens, the renderer passes, the conformance oracle, the C ABI's logging), so free
    /// to call the platform's libm. Everything else is reachable from a logic step and must go
    /// through [`det`].
    const PRESENTATION: &[&str] = &[
        "client/", "hud/", "classui/", "classmenu/", "lui/", "nativevm/", "nativebody/", "conformance/",
        "menu.rs", "ffi.rs", "sounds.rs", "soundlayers.rs",
    ];

    /// The lockstep's cross-platform guard: no sim-reachable source calls an `f32` transcendental
    /// method (the platform libm), only [`det`]. Test modules (a file's trailing `#[cfg(test)] mod
    /// … {`, or a `tests.rs`/`*_tests.rs` file) are skipped.
    #[test]
    fn the_sim_calls_no_platform_libm() {
        const CALLS: &[&str] = &[
            ".sin(", ".cos(", ".tan(", ".asin(", ".acos(", ".atan(", ".atan2(", ".sin_cos(", ".sinh(", ".cosh(",
            ".tanh(", ".exp(", ".exp2(", ".exp_m1(", ".ln(", ".ln_1p(", ".log2(", ".log10(", ".powi(",
            ".powf(", ".hypot(", ".cbrt(", "f32::sin", "f32::cos", "f32::tan", "f32::atan", "f32::exp", "f32::pow",
        ];
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        let mut dirs = vec![root.clone()];
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let path = e.path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|x| x == "rs") {
                    files.push(path);
                }
            }
        }
        let mut found = Vec::new();
        for path in &files {
            let rel = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
            let name = rel.rsplit('/').next().unwrap_or_default();
            if PRESENTATION.iter().any(|p| rel.starts_with(p)) || name == "tests.rs" || name.ends_with("_tests.rs") {
                continue;
            }
            let text = std::fs::read_to_string(path).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                // The file's own test module ends the scan.
                if line.trim() == "#[cfg(test)]" && lines.get(i + 1).is_some_and(|n| n.starts_with("mod ") && n.trim_end().ends_with('{')) {
                    break;
                }
                let code = line.split("//").next().unwrap_or_default();
                if let Some(call) = CALLS.iter().find(|c| code.contains(*c)) {
                    found.push(format!("{rel}:{}: `{call}` in `{}`", i + 1, line.trim()));
                }
            }
        }
        assert!(found.is_empty(), "sim-reachable code calls the platform libm (use crate::math::det):\n{}", found.join("\n"));
    }

    /// `det` is the same function the platform's libm approximates (to a few ulps), on the angles
    /// the sim feeds it.
    #[test]
    fn det_matches_the_platform_libm_to_the_ulp_scale() {
        for i in -200..=200 {
            let x = i as f32 * 0.0371;
            assert!((det::sin(x) - x.sin()).abs() <= 2e-7 * x.abs().max(1.0), "sin {x}");
            assert!((det::cos(x) - x.cos()).abs() <= 2e-7 * x.abs().max(1.0), "cos {x}");
            assert_eq!(det::sin_cos(x), (det::sin(x), det::cos(x)), "sin_cos {x}");
            assert!((det::atan(x) - x.atan()).abs() <= 2e-7, "atan {x}");
            assert!((det::atan2(x, 1.3) - x.atan2(1.3)).abs() <= 2e-7, "atan2 {x}");
            assert!((det::exp(x * 0.1) - (x * 0.1).exp()).abs() <= 1e-6 * (x * 0.1).exp(), "exp {x}");
        }
        assert_eq!(det::powi(0.5, 3), 0.125);
        assert_eq!(det::powi(1.5, 0), 1.0);
    }

    #[test]
    fn angles_and_basis_agree() {
        let d = dir_from_angles(0.0, 0.0);
        assert!((d.x - 1.0).abs() < 1e-6 && d.y.abs() < 1e-6);
        let (f, r) = basis_xy(std::f32::consts::FRAC_PI_2);
        assert!((f.y - 1.0).abs() < 1e-6, "yaw 90 faces +Y");
        assert!((r.x - 1.0).abs() < 1e-6, "right of +Y is +X");
        assert!((r.cross(f).z - 1.0).abs() < 1e-6, "right x forward = up");
    }

    #[test]
    fn wrap_and_lerp_angle_take_short_way() {
        assert!((wrap_angle(3.0 * PI) - PI).abs() < 1e-5);
        let mid = lerp_angle(PI - 0.1, -PI + 0.1, 0.5);
        assert!((mid.abs() - PI).abs() < 1e-4);
    }
}
