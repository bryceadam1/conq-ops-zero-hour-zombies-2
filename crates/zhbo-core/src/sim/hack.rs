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
        let Some(held_for) = p.arsenal.hacking() else {
            p.hack = None;
            return;
        };
        let tool = p.arsenal.hacking_def();
        let eye = p.body.eye();
        let dir = dir_from_angles(p.yaw, p.pitch);
        // Equipment first, then the nearest drone in the aim cone.
        let target = match self.equipment.aimed_at(host, p.general, eye, dir) {
            Some(i) => Some((self.equipment.placed[i].object, self.equipment.tuning.hack_lock_equipment)),
            None => {
                let mut objs = Vec::new();
                host.query(eye, self.equipment.tuning.hack_range, p.general, &mut objs);
                objs.into_iter()
                    .filter(|o| o.has(objflags::DRONE) && o.has(objflags::ENEMY) && !o.has(objflags::DEAD))
                    .filter_map(|o| {
                        let to = o.pos + Vec3::new(0.0, 0.0, o.height * 0.5) - eye;
                        let d = to.length();
                        (d > 1e-3 && to.dot(dir) / d >= HACK_COS).then_some((o.id, d))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
                    .map(|(id, _)| (id, self.equipment.tuning.hack_lock_drone))
            }
        };
        let Some((target, lock)) = target else {
            p.hack = None;
            return;
        };
        // BO2's hack (`_hacker_tool.gsc`): the script starts the lock `hackertoollockstart` after
        // the pull-back and needs `lock` seconds of sight on one target (a new target starts over);
        // the tool then fires once it is also pulled back (`iHoldFireTime`), and the hack lands
        // when it leaves the hand (`iFireDelay`, `hacker_tool_fired`).
        let fire = tool.map_or(0.0, |d| d.timing.fire_delay);
        let needed = self.equipment.tuning.hack_lock_start + lock + fire;
        let pulled_back = tool.is_none_or(|d| held_for >= d.timing.hold_fire_time + fire - 1e-6);
        let progress = match p.hack {
            Some((t, done)) if t == target => done + LOGIC_DT,
            _ => LOGIC_DT,
        };
        if progress >= needed - 1e-6 && pulled_back {
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
            self.emit(idx, EventKind::Hacking { progress: (progress / needed.max(1e-6)).min(1.0) });
        }
    }
}
