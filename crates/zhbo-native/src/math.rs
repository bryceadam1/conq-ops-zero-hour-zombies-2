//! Rigid transforms (a rotation quaternion `x, y, z, w` and a translation) and the 4x4 matrices the
//! renderer takes (column-major, as WGSL reads `mat4x4<f32>`).

pub type Quat = [f32; 4];
pub type Vec3 = [f32; 3];
pub type Xf = (Quat, Vec3);
/// Column-major: `m[col * 4 + row]`.
pub type Mat4 = [f32; 16];

pub const QUAT_IDENTITY: Quat = [0.0, 0.0, 0.0, 1.0];
pub const IDENTITY: Xf = (QUAT_IDENTITY, [0.0; 3]);
pub const MAT4_IDENTITY: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

pub fn qmul(a: Quat, b: Quat) -> Quat {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

pub fn qconj(q: Quat) -> Quat {
    [-q[0], -q[1], -q[2], q[3]]
}

pub fn qnorm(q: Quat) -> Quat {
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if l > 1e-9 { q.map(|c| c / l) } else { QUAT_IDENTITY }
}

pub fn qrot(q: Quat, v: Vec3) -> Vec3 {
    let p = qmul(qmul(q, [v[0], v[1], v[2], 0.0]), qconj(q));
    [p[0], p[1], p[2]]
}

/// Normalized lerp along the shorter arc (what BO2's animation blending uses between keys).
pub fn nlerp(a: Quat, b: Quat, t: f32) -> Quat {
    let dot: f32 = (0..4).map(|i| a[i] * b[i]).sum();
    let s = if dot < 0.0 { -1.0 } else { 1.0 };
    qnorm([0, 1, 2, 3].map(|i| a[i] * (1.0 - t) + b[i] * s * t))
}

pub fn lerp3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

pub fn add3(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub3(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `parent * local`.
pub fn compose(parent: Xf, local: Xf) -> Xf {
    (qnorm(qmul(parent.0, local.0)), add3(parent.1, qrot(parent.0, local.1)))
}

pub fn inverse(x: Xf) -> Xf {
    let q = qconj(x.0);
    let t = qrot(q, x.1);
    (q, [-t[0], -t[1], -t[2]])
}

pub fn apply(x: Xf, p: Vec3) -> Vec3 {
    add3(qrot(x.0, p), x.1)
}

/// The matrix of a rigid transform.
pub fn mat4(x: Xf) -> Mat4 {
    let [qx, qy, qz, qw] = x.0;
    let (xx, yy, zz) = (qx * qx, qy * qy, qz * qz);
    let (xy, xz, yz) = (qx * qy, qx * qz, qy * qz);
    let (wx, wy, wz) = (qw * qx, qw * qy, qw * qz);
    [
        1.0 - 2.0 * (yy + zz),
        2.0 * (xy + wz),
        2.0 * (xz - wy),
        0.0,
        2.0 * (xy - wz),
        1.0 - 2.0 * (xx + zz),
        2.0 * (yz + wx),
        0.0,
        2.0 * (xz + wy),
        2.0 * (yz - wx),
        1.0 - 2.0 * (xx + yy),
        0.0,
        x.1[0],
        x.1[1],
        x.1[2],
        1.0,
    ]
}

/// `a * b`.
pub fn mat4_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    out
}

pub fn mat4_point(m: &Mat4, p: Vec3) -> [f32; 4] {
    [0, 1, 2, 3].map(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
}

/// Rotation about an axis (radians).
pub fn axis_angle(axis: Vec3, angle: f32) -> Quat {
    let (s, c) = (angle * 0.5).sin_cos();
    qnorm([axis[0] * s, axis[1] * s, axis[2] * s, c])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &[f32], b: &[f32]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn matrix_agrees_with_the_transform() {
        let x = (axis_angle([0.3, 0.5, 0.8], 1.1), [4.0, -2.0, 7.5]);
        let p = [1.5, -3.0, 2.0];
        let m = mat4(x);
        assert!(close(&mat4_point(&m, p)[..3], &apply(x, p)));
        let y = (axis_angle([1.0, 0.0, 0.0], -0.4), [0.0, 1.0, 0.0]);
        let both = mat4_mul(&mat4(x), &mat4(y));
        assert!(close(&mat4_point(&both, p)[..3], &apply(compose(x, y), p)));
        assert!(close(&apply(compose(inverse(x), x), p), &p));
    }
}
