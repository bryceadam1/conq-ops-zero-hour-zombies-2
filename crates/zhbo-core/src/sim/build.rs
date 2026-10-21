/*
** Conquer Ops - Zero Hour gameplay core: building in first person: the hammer, the site search and legal placement.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/

use super::*;

impl Sim {
    pub(super) fn step_building(&mut self, host: &dyn LogicHost, idx: usize, p: &mut PlayerSim, input: &FpsInput) -> bool {
        // In hand at the start of the frame: its input (the click that placed, say) is not the gun's.
        let had_hammer = p.hammer.is_some();
        if input.pressed & buttons::HAMMER != 0 {
            p.hammer = match p.hammer {
                Some(_) => None,
                None => Some(Hammer::default()),
            };
            self.emit(idx, EventKind::Hammer { up: p.hammer.is_some() });
        } else if p.hammer.is_some() && input.pressed & (buttons::SWAP_WEAPON | buttons::LETHAL | buttons::TACTICAL) != 0 {
            // Reaching for a weapon puts the hammer away.
            p.hammer = None;
            self.emit(idx, EventKind::Hammer { up: false });
        }
        if let Some(mut h) = p.hammer {
            h.raise = (h.raise + LOGIC_DT).min(building::HAMMER_RAISE);
            let options = host.build_options(p.general);
            let n = options.len();
            let mut placed = false;
            if n > 0 {
                if !h.picked {
                    h.picked = true;
                    h.choice = options.iter().position(|o| o.status == building::status::OK).unwrap_or(0);
                }
                if input.pressed & buttons::ADS != 0 {
                    h.choice = (h.choice + 1) % n;
                }
                if input.pressed & buttons::ROTATE_LEFT != 0 {
                    h.turn = crate::math::wrap_angle(h.turn + building::ROTATE_STEP);
                }
                if input.pressed & buttons::ROTATE_RIGHT != 0 {
                    h.turn = crate::math::wrap_angle(h.turn - building::ROTATE_STEP);
                }
                h.choice = h.choice.min(n - 1);
                (h.at, h.angle) = placement(host, p, options[h.choice].radius, h.turn);
                if input.pressed & buttons::FIRE != 0 && h.raise >= building::HAMMER_RAISE && !p.building {
                    let o = options[h.choice];
                    let (pos, angle) = (h.at, h.angle);
                    let status = if o.status != building::status::OK {
                        o.status
                    } else if host.can_place(p.general, o.template, pos, angle) && host.place(p.general, o.template, pos, angle) != 0 {
                        -1
                    } else {
                        building::status::BLOCKED
                    };
                    if status == -1 {
                        placed = true;
                        host.log(&format!("player {idx} placed {} turned {:.0} degrees", o.name(), h.turn.to_degrees()));
                        self.emit(idx, EventKind::BuildPlaced { template: o.template });
                    } else {
                        host.log(&format!("player {idx} can't build {} (status {status}, ${})", o.name(), o.cost));
                        self.emit(idx, EventKind::BuildRefused { status });
                    }
                }
            }
            if placed {
                // One structure per hammer: it goes away once placed (bring it up again for another).
                p.hammer = None;
                self.emit(idx, EventKind::Hammer { up: false });
            } else {
                p.hammer = Some(h);
            }
        }
        if p.building {
            if let Some(site) = p.site {
                let percent = host.work(p.general, site.id, building::FPS_BUILD_RATE);
                if percent < 0.0 {
                    p.building = false;
                    if input.pressed & buttons::FIELD_COMPUTER != 0 {
                        self.emit(idx, EventKind::BuildRefused { status: building::status::BUSY });
                    }
                } else if percent >= 100.0 {
                    p.building = false;
                    p.site = None;
                    self.emit(idx, EventKind::WorkDone { kind: site.kind });
                } else {
                    p.site = Some(Site { percent, ..site });
                    self.emit(idx, EventKind::Working { kind: site.kind, percent });
                }
            }
        }
        had_hammer || p.hammer.is_some() || p.building
    }
}

pub const PLANT_DISTANCE: f32 = inches(30.0);
/// [`World`] backed by the engine: terrain from `TheTerrainLogic`, solid footprints from structures
/// and ground vehicles.
pub struct HostWorld<'a> {
    pub host: &'a dyn LogicHost,
    pub viewer: u32,
}

impl World for HostWorld<'_> {
    fn ground_height(&self, x: f32, y: f32) -> f32 {
        self.host.ground_height(x, y)
    }

    fn is_walkable(&self, x: f32, y: f32) -> bool {
        self.host.is_walkable(x, y)
    }

    fn extent(&self) -> (Vec3, Vec3) {
        self.host.extent()
    }

    fn blockers(&self, pos: Vec3, radius: f32, out: &mut Vec<Blocker>) {
        let mut objs = Vec::new();
        self.host.query(pos, radius, self.viewer, &mut objs);
        objs.sort_by_key(|o| o.id);
        for o in objs.iter().filter(|o| o.id != self.viewer) {
            if o.has(objflags::DEAD) || o.has(objflags::NO_COLLIDE) || o.has(objflags::AIRBORNE) {
                continue;
            }
            if !(o.has(objflags::STRUCTURE) || o.has(objflags::VEHICLE)) {
                continue;
            }
            let top = o.pos.z + o.height.max(1.0);
            out.push(match o.geom_type {
                geom::BOX => Blocker::Box { center: o.pos, half_x: o.major_radius, half_y: o.minor_radius, yaw: o.yaw, top },
                _ => Blocker::Circle { center: o.pos, radius: o.major_radius, top },
            });
        }
    }
}

/// Where the hammer puts a structure with a footprint of `radius`, and which way it faces: its
/// front towards him, turned by `turn`.
pub fn placement(host: &dyn LogicHost, p: &PlayerSim, radius: f32, turn: f32) -> (Vec3, f32) {
    let pos = building::place_point(p.body.eye(), dir_from_angles(p.yaw, p.pitch), radius, |x, y| host.ground_height(x, y));
    (pos, crate::math::wrap_angle(p.yaw + std::f32::consts::PI + turn))
}
