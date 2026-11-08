//! A parsed command to concrete orders (docs/COMMANDS.md sections 3, 6 and 7): the selection as
//! object ids, each order as a [`CmdOrder`] the bridge sends as the overhead view would.
//!
//! Built so far (the build order's step 2): base sets (types, class words, `units`), adjectives,
//! the `near` filter, superlatives and quantities, `and`/`except`, control groups, team names,
//! `them` and `that`; targets (`me`, `there`, `that`, object queries, offsets); and the orders
//! move, force move, attack-move, stop, attack, force attack, guard, `name them` and `make them
//! group`, with `then` queueing moves. Everything else is refused with a message saying so.
//!
//! Resolution reads only the [`World`] snapshot, so it never feeds the sim. Ties break the same way
//! every time (rule 5): by the stated order, then the distance to the General, then the object id.

use super::ast::*;
use super::print::{print_group, print_order};
use super::vocab::Vocabulary;
use super::world::{cmdflags, orderkind, relation as rel, CmdObject, CmdOrder, World, CLASSES};
use crate::math::Vec3;

/// What the player commanded before: the last selection (`them`) and the team names.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Memory {
    pub last: Vec<u32>,
    pub teams: Vec<(String, Vec<u32>)>,
}

impl Memory {
    pub fn team_names(&self) -> Vec<String> {
        self.teams.iter().map(|(n, _)| n.clone()).collect()
    }
}

/// The tuning and the ground a command resolves with.
pub struct Settings<'a> {
    pub near_radius: f32,
    pub offset_distance: f32,
    /// The terrain's height at (x, y), for points on the ground.
    pub ground: &'a dyn Fn(f32, f32) -> f32,
}

/// A resolved command.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// Each order with the ids it goes to, in order.
    pub orders: Vec<(CmdOrder, Vec<u32>)>,
    /// The selection (the next command's `them`).
    pub selection: Vec<u32>,
    /// Team names this command saves.
    pub teams: Vec<(String, Vec<u32>)>,
    /// What was done, for the echo line: "3 units: rangers move to me".
    pub summary: String,
}

/// How far the crosshair reaches for `there` and `that`, Zero Hour units.
const CROSSHAIR_RANGE: f32 = 4000.0;
/// The ray march's step for `there`.
const CROSSHAIR_STEP: f32 = 4.0;

pub fn resolve(cmd: &Command, world: &World, memory: &Memory, vocab: &Vocabulary, settings: &Settings) -> Result<Plan, String> {
    let r = Resolver { world, memory, vocab, settings };
    let selection = match &cmd.selection {
        Some(sel) => r.selection(sel)?,
        // Implied: the last selection, for the orders built so far (production, construction and
        // powers, which imply their own, come with those verbs).
        None => {
            let them = r.alive(&memory.last);
            if them.is_empty() {
                return Err("say who: no units were commanded yet".into());
            }
            them
        }
    };
    let mut orders = Vec::new();
    let mut teams = Vec::new();
    for (i, order) in cmd.orders.iter().enumerate() {
        match order {
            Order::NameThem(name) => teams.push((name.clone(), selection.clone())),
            Order::MakeGroup(n) => orders.push((CmdOrder { kind: orderkind::MAKE_GROUP, arg: *n as i32, ..CmdOrder::default() }, selection.clone())),
            Order::Verb { verb, target, .. } => {
                let mut o = r.verb(verb, target.as_ref(), &selection)?;
                if i > 0 {
                    if o.kind != orderkind::MOVE {
                        return Err(format!("only a move can follow 'then' for now, not '{}'", print_order(order, vocab)));
                    }
                    o.queued = 1;
                }
                orders.push((o, selection.clone()));
            }
        }
    }
    let text = super::print(cmd, vocab);
    let n = selection.len();
    let summary = format!("{n} {}: {text}", if n == 1 { "unit" } else { "units" });
    Ok(Plan { orders, selection, teams, summary })
}

struct Resolver<'a> {
    world: &'a World,
    memory: &'a Memory,
    vocab: &'a Vocabulary,
    settings: &'a Settings<'a>,
}

/// What a target resolved to.
enum Aim {
    Object(u32, Vec3),
    Point(Vec3),
}

impl Resolver<'_> {
    // --------------------------------------------------------------------------------------------
    // Selections
    // --------------------------------------------------------------------------------------------

    fn selection(&self, sel: &Selection) -> Result<Vec<u32>, String> {
        let mut ids: Vec<u32> = Vec::new();
        for g in &sel.include {
            for id in self.group(g)? {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        for g in &sel.exclude {
            let out = self.group(g)?;
            ids.retain(|id| !out.contains(id));
        }
        if ids.is_empty() {
            return Err("nobody is left to command".into());
        }
        Ok(ids)
    }

    fn group(&self, g: &Group) -> Result<Vec<u32>, String> {
        let ids = match g {
            Group::Query(q) => {
                let found = self.query(Relation::Any, q, true)?;
                if found.is_empty() {
                    return Err(format!("you have no {}", print_group(g, self.vocab)));
                }
                found
            }
            Group::ControlGroup(n) => {
                let ids: Vec<u32> = self.sorted(self.selectable().filter(|o| o.groups & (1 << n) != 0).collect());
                if ids.is_empty() {
                    return Err(format!("group {n} is empty"));
                }
                ids
            }
            Group::Team(name) => {
                let ids = self.memory.teams.iter().find(|(n, _)| n == name).map(|(_, ids)| self.alive(ids)).unwrap_or_default();
                if ids.is_empty() {
                    return Err(format!("{name} has nobody left"));
                }
                ids
            }
            Group::Them => {
                let ids = self.alive(&self.memory.last);
                if ids.is_empty() {
                    return Err("'them': no units were commanded yet".into());
                }
                ids
            }
            Group::That => match self.crosshair_object()? {
                o if o.relation == rel::OWN && o.flags & cmdflags::SELECTABLE != 0 => vec![o.id],
                _ => return Err("that isn't one of your units".into()),
            },
        };
        Ok(ids)
    }

    /// The player's own objects a selection can take.
    fn selectable(&self) -> impl Iterator<Item = &CmdObject> {
        self.world.objects.iter().filter(|o| o.relation == rel::OWN && o.flags & cmdflags::SELECTABLE != 0 && o.flags & cmdflags::GENERAL == 0)
    }

    /// `ids` that are still the player's to command, in their order.
    fn alive(&self, ids: &[u32]) -> Vec<u32> {
        ids.iter().copied().filter(|&id| self.selectable().any(|o| o.id == id)).collect()
    }

    /// The objects a query matches, in the order its superlative (or, without one, the distance to
    /// the General) puts them, cut to its quantity. `selection`: the player's selectable units,
    /// else the objects of `relation`.
    fn query(&self, relation: Relation, q: &Query, selection: bool) -> Result<Vec<u32>, String> {
        if q.filters.region.is_some() || q.filters.my_group {
            return Err("group filters ('in the … group', 'in my group') come in a later step".into());
        }
        let near = match &q.filters.near {
            Some(p) => Some(self.point(p)?),
            None => None,
        };
        let candidates: Vec<&CmdObject> = if selection {
            self.selectable().collect()
        } else {
            self.world.objects.iter().filter(|o| o.id != self.world.general && relation_matches(relation, o.relation)).collect()
        };
        let mut found: Vec<&CmdObject> = candidates
            .into_iter()
            .filter(|o| self.base_matches(&q.base, o, selection))
            .filter(|o| q.adjectives.iter().all(|&a| adjective_matches(a, o)))
            .filter(|o| near.is_none_or(|at| (o.pos - at).length_xy() <= self.settings.near_radius))
            .collect();
        let me = self.world.general_pos;
        let dist = |o: &CmdObject| (o.pos - me).length_xy();
        found.sort_by(|a, b| {
            let primary = match q.superlative {
                None | Some(Superlative::Nearest) => dist(a).total_cmp(&dist(b)),
                Some(Superlative::Farthest) => dist(b).total_cmp(&dist(a)),
                Some(Superlative::Most(c)) => {
                    let along = |o: &CmdObject| match c {
                        Cardinal::North => o.pos.y,
                        Cardinal::South => -o.pos.y,
                        Cardinal::East => o.pos.x,
                        Cardinal::West => -o.pos.x,
                    };
                    along(b).total_cmp(&along(a))
                }
                Some(Superlative::Weakest) => a.health.total_cmp(&b.health),
                Some(Superlative::Healthiest) => b.health.total_cmp(&a.health),
            };
            primary.then(dist(a).total_cmp(&dist(b))).then(a.id.cmp(&b.id))
        });
        let n = match q.quantity {
            Quantity::All => found.len(),
            Quantity::One => 1,
            Quantity::Count(n) => n as usize,
            Quantity::Half => found.len().div_ceil(2),
        };
        Ok(found.into_iter().take(n).map(|o| o.id).collect())
    }

    fn base_matches(&self, base: &Base, o: &CmdObject, selection: bool) -> bool {
        match base {
            Base::Type(name) => self.world.templates(name).contains(&o.tmpl),
            Base::Class(word) => CLASSES.iter().find(|(s, _, _)| s == word).is_some_and(|&(_, _, flag)| o.flags & flag != 0),
            // `units` in a selection is Zero Hour's select-all; as a target, any unit (not a
            // building).
            Base::Units if selection => o.flags & cmdflags::SELECT_ALL != 0,
            Base::Units => o.flags & cmdflags::STRUCTURE == 0,
        }
    }

    fn sorted(&self, mut objs: Vec<&CmdObject>) -> Vec<u32> {
        let me = self.world.general_pos;
        objs.sort_by(|a, b| (a.pos - me).length_xy().total_cmp(&(b.pos - me).length_xy()).then(a.id.cmp(&b.id)));
        objs.into_iter().map(|o| o.id).collect()
    }

    // --------------------------------------------------------------------------------------------
    // Targets
    // --------------------------------------------------------------------------------------------

    /// A point's position on the ground.
    fn point(&self, p: &Point) -> Result<Vec3, String> {
        Ok(match self.aim(p)? {
            Aim::Object(_, at) | Aim::Point(at) => at,
        })
    }

    fn aim(&self, p: &Point) -> Result<Aim, String> {
        match p {
            Point::Me => Ok(Aim::Object(self.world.general, self.world.general_pos)),
            Point::There => self.crosshair_ground().map(Aim::Point),
            Point::Object(ObjectRef::That) => self.crosshair_object().map(|o| Aim::Object(o.id, o.pos)),
            Point::Object(ObjectRef::Query { relation, query }) => {
                let found = self.query(*relation, query, false)?;
                let id = *found.first().ok_or_else(|| {
                    let what = Point::Object(ObjectRef::Query { relation: *relation, query: query.clone() });
                    format!("can't see {}", super::print::print_point(&what, self.vocab))
                })?;
                let at = self.world.object(id).map_or(Vec3::ZERO, |o| o.pos);
                Ok(Aim::Object(id, at))
            }
            Point::Place(name) => Err(format!("place names ('{name}') come in a later step")),
        }
    }

    /// A target location: the point, moved by its offset (on the ground).
    fn location(&self, loc: &Location) -> Result<Aim, String> {
        let aim = self.aim(&loc.point)?;
        let Some(offset) = loc.offset else { return Ok(aim) };
        let at = match aim {
            Aim::Object(_, at) | Aim::Point(at) => at,
        };
        let d = self.settings.offset_distance;
        let me = self.world.general_pos;
        let facing = Vec3::new(self.world.general_yaw.cos(), self.world.general_yaw.sin(), 0.0);
        // `behind`/`in front of`: the General's own facing for himself; for anything else, the
        // line from him to it (behind the war factory is its far side from him).
        let away = if matches!(loc.point, Point::Me) { facing } else { horizontal_unit(at - me).unwrap_or(facing) };
        let moved = match offset {
            Offset::Near => at,
            Offset::Behind if matches!(loc.point, Point::Me) => at - away * d,
            Offset::Behind => at + away * d,
            Offset::InFrontOf if matches!(loc.point, Point::Me) => at + away * d,
            Offset::InFrontOf => at - away * d,
            Offset::Of(dir) => at + compass(dir) * d,
        };
        Ok(Aim::Point(Vec3::new(moved.x, moved.y, (self.settings.ground)(moved.x, moved.y))))
    }

    /// Where the crosshair's ray first goes under the ground.
    fn crosshair_ground(&self) -> Result<Vec3, String> {
        let (eye, dir) = (self.world.eye, self.world.dir);
        let mut t = 0.0;
        while t < CROSSHAIR_RANGE {
            t += CROSSHAIR_STEP;
            let p = eye + dir * t;
            if p.z <= (self.settings.ground)(p.x, p.y) {
                return Ok(Vec3::new(p.x, p.y, (self.settings.ground)(p.x, p.y)));
            }
        }
        Err("the crosshair isn't on the ground".into())
    }

    /// The object the crosshair's ray meets first (its bounding sphere), the General aside.
    fn crosshair_object(&self) -> Result<&CmdObject, String> {
        let (eye, dir) = (self.world.eye, self.world.dir);
        self.world
            .objects
            .iter()
            .filter(|o| o.id != self.world.general)
            .filter_map(|o| {
                let to = o.pos + Vec3::new(0.0, 0.0, o.radius * 0.5) - eye;
                let t = to.dot(dir);
                let miss = (to - dir * t).length();
                (t > 0.0 && t < CROSSHAIR_RANGE && miss <= o.radius.max(5.0)).then_some((t, o))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.id.cmp(&b.1.id)))
            .map(|(_, o)| o)
            .ok_or_else(|| "the crosshair isn't on anything".into())
    }

    // --------------------------------------------------------------------------------------------
    // Orders
    // --------------------------------------------------------------------------------------------

    fn verb(&self, verb: &str, target: Option<&Target>, selection: &[u32]) -> Result<CmdOrder, String> {
        let aim = match target {
            Some(t) => Some(self.location(&t.location)?),
            None => None,
        };
        let at = |kind: i32, aim: &Aim| match aim {
            Aim::Object(_, p) | Aim::Point(p) => CmdOrder { kind, pos: *p, ..CmdOrder::default() },
        };
        let on = |kind: i32, aim: &Aim| match aim {
            Aim::Object(id, p) => Ok(CmdOrder { kind, target: *id, pos: *p, ..CmdOrder::default() }),
            Aim::Point(_) => Err(format!("'{verb}' needs something to aim at, not a spot")),
        };
        match (verb, aim.as_ref()) {
            ("stop", _) => Ok(CmdOrder { kind: orderkind::STOP, ..CmdOrder::default() }),
            ("move", Some(a)) => Ok(at(orderkind::MOVE, a)),
            ("force move", Some(a)) => Ok(at(orderkind::FORCE_MOVE, a)),
            ("attack-move", Some(a)) => Ok(at(orderkind::ATTACK_MOVE, a)),
            ("attack", Some(a)) => on(orderkind::ATTACK, a),
            ("force attack", Some(a @ Aim::Object(..))) => on(orderkind::FORCE_ATTACK, a),
            ("force attack", Some(a)) => Ok(at(orderkind::FORCE_ATTACK_GROUND, a)),
            ("guard", Some(a @ Aim::Object(..))) => on(orderkind::GUARD_OBJECT, a),
            ("guard", Some(a)) => Ok(at(orderkind::GUARD_POSITION, a)),
            // Guard with no target: where they stand (their centre).
            ("guard", None) => Ok(CmdOrder { kind: orderkind::GUARD_POSITION, pos: self.centre(selection), ..CmdOrder::default() }),
            _ => Err(format!("'{verb}' comes in a later step")),
        }
    }

    fn centre(&self, ids: &[u32]) -> Vec3 {
        let pts: Vec<Vec3> = ids.iter().filter_map(|&id| self.world.object(id)).map(|o| o.pos).collect();
        if pts.is_empty() {
            return self.world.general_pos;
        }
        let sum = pts.iter().fold(Vec3::ZERO, |s, &p| s + p);
        let c = sum * (1.0 / pts.len() as f32);
        Vec3::new(c.x, c.y, (self.settings.ground)(c.x, c.y))
    }
}

fn relation_matches(want: Relation, have: i32) -> bool {
    match want {
        Relation::Any => true,
        Relation::Own => have == rel::OWN,
        Relation::Enemy => have == rel::ENEMY,
        Relation::Allied => have == rel::ALLY,
        Relation::Neutral => have == rel::NEUTRAL,
        Relation::Civilian => have == rel::CIVILIAN,
    }
}

fn adjective_matches(a: Adjective, o: &CmdObject) -> bool {
    match a {
        Adjective::Idle => o.flags & cmdflags::IDLE != 0,
        Adjective::Damaged => o.health < 1.0,
        Adjective::Veteran => o.veterancy == 1,
        Adjective::Elite => o.veterancy == 2,
        Adjective::Heroic => o.veterancy == 3,
        Adjective::Garrisoned => o.flags & cmdflags::STRUCTURE != 0 && o.flags & cmdflags::LOADED != 0,
        Adjective::Loaded => o.flags & cmdflags::STRUCTURE == 0 && o.flags & cmdflags::LOADED != 0,
    }
}

/// A compass direction on the radar's map: north is +y (the radar's top, `W3DRadar::radarToPixel`
/// inverts y), east +x.
fn compass(d: Direction) -> Vec3 {
    let h = std::f32::consts::FRAC_1_SQRT_2;
    let (x, y) = match d {
        Direction::North => (0.0, 1.0),
        Direction::South => (0.0, -1.0),
        Direction::East => (1.0, 0.0),
        Direction::West => (-1.0, 0.0),
        Direction::Northeast => (h, h),
        Direction::Northwest => (-h, h),
        Direction::Southeast => (h, -h),
        Direction::Southwest => (-h, -h),
    };
    Vec3::new(x, y, 0.0)
}

fn horizontal_unit(v: Vec3) -> Option<Vec3> {
    let l = v.length_xy();
    (l > 1e-3).then(|| Vec3::new(v.x / l, v.y / l, 0.0))
}
