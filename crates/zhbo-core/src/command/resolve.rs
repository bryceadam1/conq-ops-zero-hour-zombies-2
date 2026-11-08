//! A parsed command to concrete orders (docs/COMMANDS.md sections 3, 6 and 7): the selection as
//! object ids, each order as a [`CmdOrder`] the bridge sends as the overhead view would.
//!
//! Built (the build order's steps 2 and 3): base sets (types, class words, `units`), adjectives,
//! the `near` filter, superlatives and quantities, `and`/`except`, control groups, team names,
//! `them` and `that`; targets (`me`, `there`, `that`, object queries, offsets); the direct orders
//! (move, force move, attack-move, stop, attack, force attack, the guards, scatter, form up,
//! cheer, the beacons, `name them`, `make them group`, with `then` queueing moves); the right-click
//! verbs, each checked against what the engine's right-click would do; and the command buttons'
//! verbs (`build`, `upgrade`, `use`, `purchase`, `cancel`, `unload`, `switch to`, `fire`, `sell`,
//! …), from the buttons the bridge lists, refused with the button's own availability. Cluster
//! filters and place names are refused with a message saying they come later.
//!
//! Resolution reads only the [`World`] snapshot, so it never feeds the sim. Ties break the same way
//! every time (rule 5): by the stated order, then the distance to the General, then the object id.

use super::ast::*;
use super::print::{print_group, print_order};
use super::vocab::Vocabulary;
use super::world::{availability as av, button as btn, cmdflags, context as ctx, orderkind, relation as rel, CmdButton, CmdObject, CmdOrder, World, CLASSES};
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
    /// The groups' link distance and the central region's share of the map's half-size.
    pub cluster_link: f32,
    pub central_share: f32,
    /// The terrain's height at (x, y), for points on the ground.
    pub ground: &'a dyn Fn(f32, f32) -> f32,
}

/// A resolved command.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// Each order with the ids it goes to, in order.
    pub orders: Vec<(CmdOrder, Vec<u32>)>,
    /// The units the command was for (the next command's `them`).
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
    // The selection said, if any. Without one, each verb implies its own (section 7): the button
    // verbs that produce or use the player's things look across all his objects, the rest take the
    // last selection.
    let mut selection = match &cmd.selection {
        Some(sel) => Some(r.selection(sel)?),
        None => None,
    };
    let mut orders: Vec<(CmdOrder, Vec<u32>)> = Vec::new();
    let mut teams = Vec::new();
    for (i, order) in cmd.orders.iter().enumerate() {
        match order {
            Order::NameThem(name) => {
                let sel = selection.get_or_insert(r.them()?);
                teams.push((name.clone(), sel.clone()));
            }
            Order::MakeGroup(n) => {
                let sel = selection.get_or_insert(r.them()?).clone();
                orders.push((CmdOrder { kind: orderkind::MAKE_GROUP, arg: *n as i32, ..order_default() }, sel));
            }
            Order::Verb { verb, thing, target } => {
                let mut planned = r.verb(verb, thing.as_ref(), target.as_deref(), selection.as_deref())?;
                if i > 0 {
                    if planned.iter().any(|(o, _)| o.kind != orderkind::MOVE) {
                        return Err(format!("only a move can follow 'then' for now, not '{}'", print_order(order, vocab)));
                    }
                    planned.iter_mut().for_each(|(o, _)| o.queued = 1);
                }
                if selection.is_none() {
                    let mut acting: Vec<u32> = Vec::new();
                    for id in planned.iter().flat_map(|(_, ids)| ids.iter().copied()) {
                        if !acting.contains(&id) {
                            acting.push(id);
                        }
                    }
                    selection = Some(acting);
                }
                orders.extend(planned);
            }
        }
    }
    let selection = selection.unwrap_or_default();
    let text = super::print(cmd, vocab);
    let summary = match selection.len() {
        0 => text,
        1 => format!("1 unit: {text}"),
        n => format!("{n} units: {text}"),
    };
    Ok(Plan { orders, selection, teams, summary })
}

/// An order with nothing set: no button slot.
fn order_default() -> CmdOrder {
    CmdOrder { slot: -1, ..CmdOrder::default() }
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
        let near = match &q.filters.near {
            Some(p) => Some(self.point(p)?),
            None => None,
        };
        let candidates: Vec<&CmdObject> = if selection {
            self.selectable().collect()
        } else {
            self.world.objects.iter().filter(|o| o.id != self.world.general && o.flags & cmdflags::CONTAINED == 0 && relation_matches(relation, o.relation)).collect()
        };
        let mut found: Vec<&CmdObject> = candidates
            .into_iter()
            .filter(|o| self.base_matches(&q.base, o, selection))
            .filter(|o| q.adjectives.iter().all(|&a| adjective_matches(a, o)))
            .filter(|o| near.is_none_or(|at| (o.pos - at).length_xy() <= self.settings.near_radius))
            .collect();
        if q.filters.region.is_some() || q.filters.my_group {
            found = self.in_group(found, q.filters.region, q.filters.my_group)?;
        }
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

    /// The candidates in the named group: clustered among themselves (with the General as one
    /// more point for `in my group`), the group named by its compass region from the map's centre.
    fn in_group<'o>(&self, found: Vec<&'o CmdObject>, region: Option<Region>, mine: bool) -> Result<Vec<&'o CmdObject>, String> {
        let mut points: Vec<Vec3> = found.iter().map(|o| o.pos).collect();
        points.push(self.world.general_pos);
        let general = points.len() - 1;
        let groups = super::cluster::clusters(&points, self.settings.cluster_link);
        let mut keep: Vec<usize> = (0..found.len()).collect();
        if mine {
            let g = groups.iter().find(|g| g.contains(&general)).cloned().unwrap_or_default();
            keep.retain(|i| g.contains(i));
            if keep.is_empty() {
                return Err("none of them is in your group".into());
            }
        }
        if let Some(r) = region {
            // Named among the groups of the candidates themselves (the General's point aside).
            let own: Vec<Vec<usize>> = groups.iter().map(|g| g.iter().copied().filter(|&i| i != general).collect::<Vec<_>>()).filter(|g| !g.is_empty()).collect();
            let names = super::cluster::names(&own, &points, self.world.map_centre, self.settings.central_share * self.world.map_radius);
            let picked: Vec<usize> = own.iter().zip(&names).filter(|(_, n)| **n == r).flat_map(|(g, _)| g.iter().copied()).collect();
            if picked.is_empty() {
                return Err(format!("there is no {} group", r.word()));
            }
            keep.retain(|i| picked.contains(i));
        }
        Ok(keep.into_iter().map(|i| found[i]).collect())
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
            Point::Place(name) => match self.world.places.iter().find(|(n, _)| n == name) {
                Some(&(_, at)) => Ok(Aim::Point(Vec3::new(at.x, at.y, (self.settings.ground)(at.x, at.y)))),
                None => Err(format!("this map has no place called {name}")),
            },
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
            .filter(|o| o.id != self.world.general && o.flags & cmdflags::CONTAINED == 0)
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

    /// The last selection still alive, for an order that names nobody.
    fn them(&self) -> Result<Vec<u32>, String> {
        let them = self.alive(&self.memory.last);
        if them.is_empty() {
            return Err("say who: no units were commanded yet".into());
        }
        Ok(them)
    }

    /// One verb's orders, each with the ids it goes to. `selection`: the units the command named,
    /// if it named any.
    fn verb(&self, verb: &str, thing: Option<&Thing>, target: Option<&Target>, selection: Option<&[u32]>) -> Result<Vec<(CmdOrder, Vec<u32>)>, String> {
        let aim = match target {
            Some(t) => Some(self.location(&t.location)?),
            None => None,
        };
        let facing = target.and_then(|t| t.facing);
        let end = match target.and_then(|t| t.end.as_ref()) {
            Some(l) => Some(self.location(l)?),
            None => None,
        };
        if end.is_some() && verb != "build" {
            return Err(format!("only a building is built from one place to another, not '{verb}'"));
        }
        match verb {
            "build" | "upgrade" | "use" | "purchase" | "switch to" | "fire" | "sell" | "overcharge" | "hack internet" | "depart"
            | "cancel construction" | "evacuate" | "combat drop" => return self.press(verb, thing, aim.as_ref(), facing, end.as_ref(), selection),
            "cancel" => return self.cancel(thing, selection),
            "unload" => return self.unload(thing, selection),
            _ => {}
        }
        let sel = match selection {
            Some(s) => s.to_vec(),
            None => self.them()?,
        };
        // The right-click's context modes that are also buttons (the Terrorist's car bomb, the
        // hijacker's, the saboteur's): the button where the selection has it.
        let mode = match verb {
            "hijack" => Some(btn::HIJACK_VEHICLE),
            "car-bomb" => Some(btn::CONVERT_TO_CARBOMB),
            "sabotage" => Some(btn::SABOTAGE_BUILDING),
            _ => None,
        };
        if let (Some(code), Some(aim)) = (mode, aim.as_ref()) {
            let owners: Vec<&CmdButton> = self.owned_buttons(&sel, |b| b.code == code).collect();
            if !owners.is_empty() {
                return owners.iter().map(|b| self.button_order(b, Some(aim), verb)).collect();
            }
        }
        let order = self.direct(verb, aim.as_ref(), &sel)?;
        Ok(vec![(order, sel)])
    }

    /// The orders that go straight to the engine's messages, and the right-click verbs.
    fn direct(&self, verb: &str, aim: Option<&Aim>, sel: &[u32]) -> Result<CmdOrder, String> {
        let at = |kind: i32, aim: &Aim| match aim {
            Aim::Object(_, p) | Aim::Point(p) => CmdOrder { kind, pos: *p, ..order_default() },
        };
        let on = |kind: i32, aim: &Aim| match aim {
            Aim::Object(id, p) => Ok(CmdOrder { kind, target: *id, pos: *p, ..order_default() }),
            Aim::Point(_) => Err(format!("'{verb}' needs something to aim at, not a spot")),
        };
        // GuardMode (GameCommon.h): normal, without pursuit, flying units only.
        let guard = |mode: i32| -> Result<CmdOrder, String> {
            let mut o = match aim {
                Some(a @ Aim::Object(..)) => on(orderkind::GUARD_OBJECT, a)?,
                Some(a) => at(orderkind::GUARD_POSITION, a),
                // Guard with no target: where they stand (their centre).
                None => CmdOrder { kind: orderkind::GUARD_POSITION, pos: self.centre(sel), ..order_default() },
            };
            o.arg = mode;
            Ok(o)
        };
        // The right-click verbs: what `evaluateContextCommand` may do for each.
        let context = |allowed: i32| -> Result<CmdOrder, String> {
            match aim {
                Some(a) => Ok(CmdOrder { arg: allowed, ..on(orderkind::CONTEXT, a)? }),
                None => Err(format!("'{verb}' needs a target")),
            }
        };
        match (verb, aim) {
            ("stop", _) => Ok(CmdOrder { kind: orderkind::STOP, ..order_default() }),
            ("scatter", _) => Ok(CmdOrder { kind: orderkind::SCATTER, ..order_default() }),
            ("form up", _) => Ok(CmdOrder { kind: orderkind::FORM_UP, ..order_default() }),
            ("cheer", _) => Ok(CmdOrder { kind: orderkind::CHEER, ..order_default() }),
            ("remove beacon", _) => Ok(CmdOrder { kind: orderkind::REMOVE_BEACON, ..order_default() }),
            ("place beacon", Some(a)) => Ok(at(orderkind::PLACE_BEACON, a)),
            ("move", Some(a)) => Ok(at(orderkind::MOVE, a)),
            ("force move", Some(a)) => Ok(at(orderkind::FORCE_MOVE, a)),
            ("attack-move", Some(a)) => Ok(at(orderkind::ATTACK_MOVE, a)),
            ("attack", Some(a)) => on(orderkind::ATTACK, a),
            ("force attack", Some(a @ Aim::Object(..))) => on(orderkind::FORCE_ATTACK, a),
            ("force attack", Some(a)) => Ok(at(orderkind::FORCE_ATTACK_GROUND, a)),
            ("guard", _) => guard(0),
            ("guard without pursuit", _) => guard(1),
            ("guard against aircraft", _) => guard(2),
            // On open ground, the right-click at the spot.
            ("act on", Some(a @ Aim::Point(_))) => Ok(CmdOrder { arg: ctx::ANY, ..at(orderkind::CONTEXT, a) }),
            ("act on", _) => context(ctx::ANY),
            ("enter" | "garrison" | "hijack", _) => context(ctx::ENTER),
            ("capture", _) => context(ctx::CAPTURE | ctx::SPECIAL_POWER),
            ("repair", _) => context(ctx::REPAIR),
            ("resume building", _) => context(ctx::RESUME_CONSTRUCTION),
            ("get repaired", _) => context(ctx::GET_REPAIRED),
            ("get healed", _) => context(ctx::GET_HEALED),
            ("dock" | "deliver supplies", _) => context(ctx::DOCK),
            ("car-bomb", _) => context(ctx::CARBOMB | ctx::ENTER),
            ("sabotage", _) => context(ctx::ENTER | ctx::SPECIAL_POWER | ctx::OTHER),
            ("salvage", _) => context(ctx::SALVAGE),
            ("disable" | "steal cash", _) => context(ctx::HACK | ctx::SPECIAL_POWER),
            // A rally point is the right-click on open ground with a factory selected.
            ("set rally point", Some(a)) => Ok(CmdOrder { arg: ctx::RALLY_POINT, ..at(orderkind::CONTEXT, a) }),
            _ => Err(format!("'{verb}' needs a target")),
        }
    }

    // --------------------------------------------------------------------------------------------
    // Command buttons
    // --------------------------------------------------------------------------------------------

    /// The buttons of `owners` (and the player's own sets) that pass `keep`, never a hidden or
    /// script-only one.
    fn owned_buttons<'b>(&'b self, owners: &'b [u32], keep: impl Fn(&CmdButton) -> bool + 'b) -> impl Iterator<Item = &'b CmdButton> + 'b {
        self.world.buttons.iter().filter(move |b| {
            (b.owner == 0 || owners.contains(&b.owner)) && b.availability != av::HIDDEN && b.options & super::world::option::SCRIPT_ONLY == 0 && keep(b)
        })
    }

    /// Every object of the player's that has buttons (the scope of an order that implies its own
    /// units), nearest first.
    fn all_owners(&self) -> Vec<u32> {
        self.sorted(self.world.objects.iter().filter(|o| o.relation == rel::OWN && o.flags & cmdflags::CONTAINED == 0).collect())
    }

    /// A button verb: the buttons it means, who presses them, and their targets.
    fn press(&self, verb: &str, thing: Option<&Thing>, aim: Option<&Aim>, facing: Option<Direction>, end: Option<&Aim>, selection: Option<&[u32]>) -> Result<Vec<(CmdOrder, Vec<u32>)>, String> {
        let name = thing.map(|t| t.name.as_str());
        let templates = name.map(|n| self.world.templates(n)).unwrap_or(&[]);
        let named: Vec<*const CmdButton> = name.map(|n| self.world.named_buttons(n).map(|b| b as *const _).collect()).unwrap_or_default();
        // The buttons the verb presses (`actions::BUTTONS`), and for the verbs with a thing, the
        // ones that name it: a build by its template, the rest by the name the button gives.
        let codes = super::actions::button_codes(verb);
        let wanted = |b: &CmdButton| -> bool {
            codes.contains(&b.code)
                && match verb {
                    "build" => templates.contains(&b.tmpl),
                    "upgrade" | "use" | "purchase" | "switch to" | "fire" => named.contains(&(b as *const _)),
                    _ => true,
                }
        };
        // Who may press: the units named; else, for the verbs that make or use the player's
        // things, all his objects; else the last selection.
        let produces = matches!(verb, "build" | "upgrade" | "use" | "purchase");
        let scope: Vec<u32> = match selection {
            Some(s) => s.to_vec(),
            None if produces => self.all_owners(),
            None => self.them()?,
        };
        let what = || match thing {
            Some(t) => format!("{verb} {}", t.name),
            None => verb.to_string(),
        };
        let candidates: Vec<&CmdButton> = self.owned_buttons(&scope, wanted).collect();
        if candidates.is_empty() {
            return Err(if selection.is_some() { format!("none of them can {}", what()) } else { format!("nothing of yours can {}", what()) });
        }
        let ready: Vec<&CmdButton> = candidates.iter().copied().filter(|b| matches!(b.availability, av::AVAILABLE | av::ACTIVE)).collect();
        if ready.is_empty() {
            let worst = |code: i32| candidates.iter().any(|b| b.availability == code);
            return Err(if worst(av::CANT_AFFORD) {
                format!("you can't afford to {}", what())
            } else if worst(av::NOT_READY) {
                format!("{} isn't ready", what())
            } else {
                format!("{} isn't available yet", what())
            });
        }
        let order_of = |b: &CmdButton| scope.iter().position(|&id| id == b.owner).unwrap_or(usize::MAX);
        let mut ready = ready;
        ready.sort_by_key(|b| (order_of(b), b.slot));
        let count = thing.map_or(1, |t| t.count.max(1)) as usize;
        let first = ready[0];
        if end.is_some() && first.code != btn::DOZER_CONSTRUCT {
            return Err(format!("only a building is built from one place to another, not {}", what()));
        }
        match first.code {
            btn::UNIT_BUILD => {
                // Each unit at the producer with the shortest queue, ties to the nearer.
                let mut queues: Vec<(&CmdButton, i32)> = ready.iter().filter(|b| b.code == btn::UNIT_BUILD).map(|&b| (b, self.world.object(b.owner).map_or(0, |o| o.queue))).collect();
                let mut out = Vec::new();
                for _ in 0..count {
                    let Some(i) = (0..queues.len()).min_by_key(|&i| (queues[i].1, order_of(queues[i].0))) else { break };
                    queues[i].1 += 1;
                    out.push(self.button_order(queues[i].0, None, verb)?);
                }
                Ok(out)
            }
            btn::DOZER_CONSTRUCT => {
                if count > 1 {
                    return Err("build one building at a time".into());
                }
                // The builder: the nearest idle one, else the nearest.
                let idle = |b: &CmdButton| self.world.object(b.owner).is_some_and(|o| o.flags & cmdflags::IDLE != 0);
                let builders: Vec<&CmdButton> = ready.iter().copied().filter(|b| b.code == btn::DOZER_CONSTRUCT).collect();
                let builder = builders.iter().copied().find(|b| idle(b)).or_else(|| builders.first().copied()).unwrap_or(first);
                let at = match aim {
                    Some(Aim::Object(_, p) | Aim::Point(p)) => *p,
                    None => self.world.object(builder.owner).map_or(self.world.general_pos, |o| o.pos),
                };
                // Without `facing`, the bridge takes the template's own placement angle (NaN).
                let angle = facing.map_or(f32::NAN, |d| {
                    let v = compass(d);
                    v.y.atan2(v.x)
                });
                let mut order = CmdOrder { kind: orderkind::CONSTRUCT, tmpl: builder.tmpl, pos: at, angle, ..order_default() };
                if let Some(Aim::Object(_, p) | Aim::Point(p)) = end {
                    order.end = *p;
                    order.arg = 1;
                }
                Ok(vec![(order, vec![builder.owner])])
            }
            btn::PLAYER_UPGRADE | btn::PURCHASE_SCIENCE | btn::SPECIAL_POWER | btn::SPECIAL_POWER_FROM_SHORTCUT | btn::SPECIAL_POWER_CONSTRUCT
            | btn::SPECIAL_POWER_CONSTRUCT_FROM_SHORTCUT => {
                // One press: the player's own set first (a shortcut fires from the most ready
                // building), then the shortest queue, then the nearest.
                let pick = ready.iter().copied().min_by_key(|b| (b.owner != 0, self.world.object(b.owner).map_or(0, |o| o.queue), order_of(b))).unwrap_or(first);
                Ok(vec![self.button_order(pick, aim, verb)?])
            }
            // Every unit that has it presses it (an object upgrade, a weapon mode, sell, evacuate,
            // …); with no units named, only the nearest for an upgrade.
            _ => {
                let mut out = Vec::new();
                let mut seen = Vec::new();
                for b in ready {
                    if seen.contains(&b.owner) {
                        continue;
                    }
                    seen.push(b.owner);
                    out.push(self.button_order(b, aim, verb)?);
                    if selection.is_none() && produces {
                        break;
                    }
                }
                Ok(out)
            }
        }
    }

    /// One press of `b`, at the target it asks for.
    fn button_order(&self, b: &CmdButton, aim: Option<&Aim>, verb: &str) -> Result<(CmdOrder, Vec<u32>), String> {
        let (wants_object, wants_point) = b.needs();
        let mut o = CmdOrder { kind: orderkind::BUTTON, slot: b.slot, set: b.set, tmpl: b.tmpl, ..order_default() };
        match aim {
            None if wants_object || wants_point => return Err(format!("'{verb}' needs a target ({})", if wants_point { "a spot or a unit" } else { "a unit or a building" })),
            None => {}
            Some(_) if !wants_object && !wants_point => return Err(format!("'{verb}' takes no target")),
            Some(Aim::Point(_)) if !wants_point => return Err(format!("'{verb}' needs a unit or a building to aim at, not a spot")),
            Some(Aim::Object(id, p)) => {
                o.target = if wants_object { *id } else { 0 };
                o.pos = *p;
            }
            Some(Aim::Point(p)) => o.pos = *p,
        }
        let ids = if b.owner == 0 { Vec::new() } else { vec![b.owner] };
        Ok((o, ids))
    }

    /// `cancel X`: the last X queued, at the first producer (named, or any of the player's) that
    /// has one. The bridge looks in each in turn.
    fn cancel(&self, thing: Option<&Thing>, selection: Option<&[u32]>) -> Result<Vec<(CmdOrder, Vec<u32>)>, String> {
        let Some(t) = thing else { return Err("cancel what?".into()) };
        let scope: Vec<u32> = match selection {
            Some(s) => s.to_vec(),
            None => self.all_owners(),
        };
        let templates = self.world.templates(&t.name);
        if !templates.is_empty() {
            // The producers that build it, each with its own template of that name.
            let producers: Vec<&CmdButton> = self.owned_buttons(&scope, |b| b.code == btn::UNIT_BUILD && templates.contains(&b.tmpl)).collect();
            let Some(first) = producers.first() else { return Err(format!("nothing of yours builds {}", t.name)) };
            let ids: Vec<u32> = producers.iter().map(|b| b.owner).collect();
            return Ok(vec![(CmdOrder { kind: orderkind::CANCEL, tmpl: first.tmpl, ..order_default() }, ids)]);
        }
        let upgrades: Vec<&CmdButton> = self.world.named_buttons(&t.name).filter(|b| matches!(b.code, btn::PLAYER_UPGRADE | btn::OBJECT_UPGRADE) && scope.contains(&b.owner)).collect();
        let Some(first) = upgrades.first() else { return Err(format!("nothing of yours researches {}", t.name)) };
        let ids: Vec<u32> = upgrades.iter().filter(|b| b.slot == first.slot).map(|b| b.owner).collect();
        Ok(vec![(CmdOrder { kind: orderkind::CANCEL, slot: first.slot, ..order_default() }, ids)])
    }

    /// `unload [N] X`: that many of the X riding in the named containers (or the last selection).
    fn unload(&self, thing: Option<&Thing>, selection: Option<&[u32]>) -> Result<Vec<(CmdOrder, Vec<u32>)>, String> {
        let Some(t) = thing else { return Err("unload what?".into()) };
        let containers: Vec<u32> = match selection {
            Some(s) => s.to_vec(),
            None => self.them()?,
        };
        let templates = self.world.templates(&t.name);
        let riders: Vec<&CmdObject> = self.world.objects.iter().filter(|o| o.flags & cmdflags::CONTAINED != 0 && containers.contains(&o.container) && templates.contains(&o.tmpl)).collect();
        if riders.is_empty() {
            return Err(format!("no {} riding in them", t.name));
        }
        Ok(riders.iter().take(t.count.max(1) as usize).map(|o| (CmdOrder { kind: orderkind::UNLOAD, target: o.id, ..order_default() }, vec![o.container])).collect())
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
