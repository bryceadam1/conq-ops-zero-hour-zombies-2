/*
** Conquer Ops - Zero Hour gameplay core: the Black Hat: aim, hack, capture.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::*;

impl Sim {
    pub(super) fn step_hack(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim) {
        if p.arsenal.hacking().is_none() {
            p.hack = None;
            return;
        }
        let eye = p.body.eye();
        let dir = dir_from_angles(p.yaw, p.pitch);
        // Equipment first, then the nearest drone in the aim cone.
        let target = match self.equipment.aimed_at(host, p.general, eye, dir) {
            Some(i) => Some((self.equipment.placed[i].object, HACK_TIME)),
            None => {
                let mut objs = Vec::new();
                host.query(eye, HACK_RANGE, p.general, &mut objs);
                objs.into_iter()
                    .filter(|o| o.has(objflags::DRONE) && o.has(objflags::ENEMY) && !o.has(objflags::DEAD))
                    .filter_map(|o| {
                        let to = o.pos + Vec3::new(0.0, 0.0, o.height * 0.5) - eye;
                        let d = to.length();
                        (d > 1e-3 && to.dot(dir) / d >= HACK_COS).then_some((o.id, d))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
                    .map(|(id, _)| (id, HACK_DRONE_TIME))
            }
        };
        let Some((target, needed)) = target else {
            p.hack = None;
            return;
        };
        let progress = match p.hack {
            Some((t, done)) if t == target => done + LOGIC_DT,
            _ => LOGIC_DT,
        };
        if progress >= needed {
            if let Some(i) = self.equipment.placed.iter().position(|e| e.object == target) {
                self.equipment.hacked(host, i, idx, p.general);
            } else {
                host.capture(target, p.general);
            }
            p.arsenal.hack_done();
            p.hack = None;
            self.emit(idx, EventKind::Hacked);
        } else {
            p.hack = Some((target, progress));
            self.emit(idx, EventKind::Hacking { progress: progress / needed });
        }
    }
}
