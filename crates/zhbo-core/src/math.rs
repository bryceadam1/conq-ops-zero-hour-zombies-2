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

    pub fn with_z(self, z: f32) -> Vec3 {
        Vec3::new(self.x, self.y, z)
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
    let (sp, cp) = pitch.sin_cos();
    let (sy, cy) = yaw.sin_cos();
    Vec3::new(cp * cy, cp * sy, sp)
}

/// Horizontal forward and right vectors for a yaw.
pub fn basis_xy(yaw: f32) -> (Vec3, Vec3) {
    let (s, c) = yaw.sin_cos();
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

#[cfg(test)]
mod tests {
    use super::*;

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
