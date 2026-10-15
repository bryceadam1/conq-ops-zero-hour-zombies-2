//! Rigid transforms: a rotation quaternion (x, y, z, w) and a translation.

pub type Xf = ([f32; 4], [f32; 3]);

pub const IDENTITY: Xf = ([0.0, 0.0, 0.0, 1.0], [0.0; 3]);

pub fn qmul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

pub fn qconj(q: [f32; 4]) -> [f32; 4] {
    [-q[0], -q[1], -q[2], q[3]]
}

pub fn qnorm(q: [f32; 4]) -> [f32; 4] {
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if l > 1e-9 { q.map(|c| c / l) } else { [0.0, 0.0, 0.0, 1.0] }
}

pub fn qrot(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let p = qmul(qmul(q, [v[0], v[1], v[2], 0.0]), qconj(q));
    [p[0], p[1], p[2]]
}

/// `parent * local`.
pub fn compose(parent: Xf, local: Xf) -> Xf {
    let t = qrot(parent.0, local.1);
    (qnorm(qmul(parent.0, local.0)), [parent.1[0] + t[0], parent.1[1] + t[1], parent.1[2] + t[2]])
}

pub fn inverse(x: Xf) -> Xf {
    let q = qconj(x.0);
    let t = qrot(q, x.1);
    (q, [-t[0], -t[1], -t[2]])
}

pub fn apply(x: Xf, v: [f32; 3]) -> [f32; 3] {
    let r = qrot(x.0, v);
    [r[0] + x.1[0], r[1] + x.1[1], r[2] + x.1[2]]
}

/// BO2 angles in degrees (pitch about Y, yaw about Z, roll about X, applied roll first) as a
/// quaternion.
pub fn from_angles(angles: [f32; 3]) -> [f32; 4] {
    let half = |deg: f32| (deg.to_radians() * 0.5).sin_cos();
    let (sp, cp) = half(angles[0]);
    let (sy, cy) = half(angles[1]);
    let (sr, cr) = half(angles[2]);
    let qz = [0.0, 0.0, sy, cy];
    let qy = [0.0, sp, 0.0, cp];
    let qx = [sr, 0.0, 0.0, cr];
    qnorm(qmul(qmul(qz, qy), qx))
}
