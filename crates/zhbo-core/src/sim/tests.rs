use super::*;
use crate::input::pack_move;
use std::cell::RefCell;
use crate::weapons::WeaponDef;

/// A structure the fake host offers to work on.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FakeSite {
    pub id: u32,
    pub template: u32,
    pub at: Vec3,
    pub angle: f32,
    pub percent: f32,
    pub kind: i32,
    /// Someone else is repairing it.
    pub busy: bool,
}

/// A flat world with a set of objects; records engine calls.
#[derive(Default)]
pub struct FakeHost {
    pub objects: RefCell<Vec<ObjInfo>>,
    pub controlled: RefCell<Vec<u32>>,
    pub fx: RefCell<Vec<(u32, String, u32)>>,
    pub damage_log: RefCell<Vec<(u32, u32, f32)>>,
    pub disabled: RefCell<Vec<(u32, i32, f32)>>,
    pub spawned: RefCell<Vec<(u32, String)>>,
    pub destroyed: RefCell<Vec<u32>>,
    pub played_fx: RefCell<Vec<String>>,
    pub smoked: RefCell<Vec<(Vec3, f32)>>,
    /// Building: the build menu, and the structures to work on (laid out, or damaged).
    pub options: RefCell<Vec<crate::building::BuildOption>>,
    pub sites: RefCell<Vec<FakeSite>>,
    /// Everywhere is water too deep to stand in (BO2's plant check refuses it).
    pub deep_water: std::cell::Cell<bool>,
    /// The ground's surface (`surface_at`); dirt when unset.
    pub ground: std::cell::Cell<Option<Surface>>,
}

impl FakeHost {
    pub fn add(&self, info: ObjInfo) {
        self.objects.borrow_mut().push(info);
    }

    pub fn get(&self, id: u32) -> ObjInfo {
        *self.objects.borrow().iter().find(|o| o.id == id).unwrap()
    }

    fn rel(&self, mut o: ObjInfo, viewer: u32) -> ObjInfo {
        let owner = self.objects.borrow().iter().find(|v| v.id == viewer).map(|v| v.owner_player);
        o.flags &= !(objflags::ALLY | objflags::ENEMY);
        if let Some(owner) = owner {
            o.flags |= if o.owner_player == owner { objflags::ALLY } else { objflags::ENEMY };
        }
        o
    }
}

impl LogicHost for FakeHost {
    fn log(&self, _msg: &str) {}
    fn ground_height(&self, _x: f32, _y: f32) -> f32 {
        0.0
    }
    fn is_walkable(&self, _x: f32, _y: f32) -> bool {
        !self.deep_water.get()
    }
    fn surface_at(&self, _x: f32, _y: f32) -> Surface {
        if self.deep_water.get() {
            Surface::Water
        } else {
            self.ground.get().unwrap_or(Surface::Dirt)
        }
    }
    fn extent(&self) -> (Vec3, Vec3) {
        (Vec3::new(-5000.0, -5000.0, -100.0), Vec3::new(5000.0, 5000.0, 1000.0))
    }
    fn object(&self, id: u32, viewer: u32) -> Option<ObjInfo> {
        self.objects.borrow().iter().find(|o| o.id == id).map(|o| self.rel(*o, viewer))
    }
    fn query(&self, center: Vec3, radius: f32, viewer: u32, out: &mut Vec<ObjInfo>) {
        for o in self.objects.borrow().iter() {
            if (o.pos - center).length() <= radius {
                out.push(self.rel(*o, viewer));
            }
        }
    }
    fn set_pose(&self, id: u32, pos: Vec3, yaw: f32, _moving: bool) {
        for o in self.objects.borrow_mut().iter_mut().filter(|o| o.id == id) {
            o.pos = pos;
            o.yaw = yaw;
        }
    }
    fn set_controlled(&self, id: u32, controlled: bool) {
        let mut c = self.controlled.borrow_mut();
        c.retain(|&x| x != id);
        if controlled {
            c.push(id);
        }
    }
    fn fire_fx(&self, shooter: u32, fx_weapon: &str, target: u32, _pos: Vec3) {
        self.fx.borrow_mut().push((shooter, fx_weapon.to_string(), target));
    }
    fn damage(&self, shooter: u32, victim: u32, amount: f32, _kind: i32) -> bool {
        self.damage_log.borrow_mut().push((shooter, victim, amount));
        let mut objs = self.objects.borrow_mut();
        let v = objs.iter_mut().find(|o| o.id == victim).unwrap();
        v.health -= amount;
        if v.health <= 0.0 {
            v.flags |= objflags::DEAD;
            return true;
        }
        false
    }
    fn heal(&self, id: u32, amount: f32) {
        for o in self.objects.borrow_mut().iter_mut().filter(|o| o.id == id) {
            o.health = (o.health + amount).min(o.max_health);
        }
    }
    fn spawn(&self, template: &str, _owner: u32, _pos: Vec3, _yaw: f32) -> u32 {
        let mut s = self.spawned.borrow_mut();
        let id = 10_000 + s.len() as u32;
        s.push((id, template.to_string()));
        id
    }
    fn destroy(&self, id: u32) {
        self.destroyed.borrow_mut().push(id);
    }
    fn disable(&self, id: u32, kind: i32, seconds: f32) {
        self.disabled.borrow_mut().push((id, kind, seconds));
    }
    fn fx(&self, name: &str, _pos: Vec3) {
        self.played_fx.borrow_mut().push(name.to_string());
    }
    fn smoke(&self, center: Vec3, radius: f32) {
        self.smoked.borrow_mut().push((center, radius));
    }
    fn build_options(&self, _general: u32) -> Vec<crate::building::BuildOption> {
        self.options.borrow().clone()
    }
    fn can_place(&self, _general: u32, _template: u32, pos: Vec3, _angle: f32) -> bool {
        self.sites.borrow().iter().all(|s| (s.at - pos).length_xy() > 80.0)
    }
    fn place(&self, _general: u32, template: u32, pos: Vec3, angle: f32) -> u32 {
        let mut sites = self.sites.borrow_mut();
        let id = 20_000 + sites.len() as u32;
        sites.push(FakeSite { id, template, at: pos, angle, ..Default::default() });
        id
    }
    fn work_site(&self, _general: u32, pos: Vec3, reach: f32) -> Option<crate::building::Site> {
        // Footprints 40 across.
        self.sites.borrow().iter().filter(|s| s.percent < 100.0 && (s.at - pos).length_xy() <= 20.0 + reach)
            .map(|s| crate::building::Site { id: s.id, template: s.template, kind: s.kind, percent: s.percent, ..Default::default() })
            .next()
    }
    fn work(&self, _general: u32, site: u32, rate: f32) -> f32 {
        let mut sites = self.sites.borrow_mut();
        let s = sites.iter_mut().find(|s| s.id == site).unwrap();
        if s.busy {
            return -1.0;
        }
        s.percent = (s.percent + 10.0 * rate).min(100.0);
        s.percent
    }
    fn capture(&self, id: u32, new_owner: u32) {
        let owner = self.objects.borrow().iter().find(|o| o.id == new_owner).map(|o| o.owner_player);
        if let Some(owner) = owner {
            for o in self.objects.borrow_mut().iter_mut().filter(|o| o.id == id) {
                o.owner_player = owner;
            }
        }
    }
}

pub fn general(id: u32, player: i32) -> ObjInfo {
    ObjInfo {
        id,
        owner_player: player,
        flags: objflags::INFANTRY,
        geom_type: geom::CYLINDER,
        major_radius: 10.0,
        minor_radius: 10.0,
        height: 12.0,
        health: 300.0,
        max_health: 300.0,
        ..Default::default()
    }
}

pub fn soldier(id: u32, player: i32, x: f32) -> ObjInfo {
    ObjInfo { pos: Vec3::new(x, 0.0, 0.0), health: 120.0, max_health: 120.0, ..general(id, player) }
}

pub(crate) fn run(sim: &mut Sim, host: &FakeHost, player: i32, general: u32, input: FpsInput, frames: u32) {
    for _ in 0..frames {
        sim.input(player, general, input);
        let f = sim.frame + 1;
        sim.step(host, f);
    }
}

/// Logic frames covering `seconds`.
fn frames(seconds: f32) -> u32 {
    (seconds / LOGIC_DT).ceil() as u32 + 1
}

/// Let the camera fly in, the tablet go down and the weapon come up.
pub(crate) fn wait_until_ready(sim: &mut Sim, host: &FakeHost) {
    let ready = sim.player(0).unwrap().arsenal.raise_left;
    run(sim, host, 0, 1, FpsInput::default(), frames(ready));
}

fn events(sim: &Sim) -> Vec<EventKind> {
    sim.events.iter().map(|e| e.kind).collect()
}

/// The contract's core promise: identical inputs produce identical sim outcomes. Two Sims over
/// two identical fake hosts run the same 1200 frames of seeded pseudo-random scripted input —
/// shots, melee, reloads, alt modes, grenade throws, equipment planting, building, the Black
/// Hat — and must agree on every event, on every engine call and on the final state (refactor
/// plan 4.2: the regression net for the sim, weapons and equipment refactors; it also watches
/// for HashMap iteration or float associativity drift).
#[test]
fn identical_inputs_are_deterministic() {
    const FRAMES: u32 = 1200;
    /// A cycle of scripted intents: settle, fire, reload, move, throw, hack, build, alt mode.
    /// Phases keep the weapon usable (a uniformly random button mask would perpetually
    /// interrupt the raise), and the xorshift only jitters within each phase.
    const CYCLE: u32 = 600;
    // A fixed-seed xorshift so a flaky RNG can never hide a real drift.
    let script = {
        let mut seed: u64 = 0x5EED_CAFE_F00D_D065;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut yaw = 0.0f32;
        let mut pitch = 0.0f32;
        let mut script = Vec::with_capacity(FRAMES as usize);
        for f in 0..FRAMES {
            let t = f % CYCLE;
            let (mut held, mut pressed) = (0u32, 0u32);
            let jitter = |r: u64, scale: f32| (r % 41) as f32 / 100.0 * scale - scale / 2.0;
            yaw += jitter(next(), 0.4);
            pitch = (pitch + jitter(next(), 0.1)).clamp(-crate::input::MAX_PITCH, crate::input::MAX_PITCH);
            let fwd = (next() % 5) as f32 / 2.0 - 1.0;
            let side = (next() % 5) as f32 / 2.0 - 1.0;
            // `pressed` must be a single-frame edge (the sim acts on every frame it is set);
            // held buttons derive their own press edge in the arsenal.
            match t {
                // Settle: the weapon comes up, no buttons.
                0..=39 => {}
                // Fire bursts at the enemy soldier standing ahead.
                40..=139 => {
                    held |= buttons::ADS | buttons::FIRE;
                    pitch = pitch.clamp(-0.15, 0.0);
                }
                // Reload.
                140..=159 => held |= buttons::RELOAD,
                // Sprint forward, then walk.
                160..=189 => held |= buttons::SPRINT,
                // Pull the lethal's pin (held), then let go: thrown. Aimed up so the blast
                // lands clear of the General.
                190..=229 => {
                    held |= buttons::LETHAL;
                    pitch = 0.6;
                }
                // Same for the tactical: its held edge must land after the lethal's 0.45 s
                // throwing wind-down has finished consuming presses.
                250..=289 => {
                    held |= buttons::TACTICAL;
                    pitch = 0.6;
                }
                // Melee swings.
                300..=329 => {
                    if t.is_multiple_of(6) {
                        pressed |= buttons::MELEE;
                    }
                }
                // Stance bits.
                330..=349 => {
                    if next() % 3 == 0 {
                        held |= buttons::CROUCH;
                    }
                    if next() % 5 == 0 {
                        pressed |= buttons::JUMP;
                    }
                    if next() % 7 == 0 {
                        held |= buttons::PRONE;
                    }
                }
                // The alt mode (the launcher): toggle in for one frame, then fire a press
                // edge after its raise finishes (a semi weapon needs a fresh press), aimed up
                // so the blast lands clear.
                350 => pressed |= buttons::ALT_MODE,
                351..=374 => held |= buttons::FIRE,
                376..=384 => {
                    held |= buttons::FIRE;
                    pitch = 0.5;
                }
                385 => pressed |= buttons::ALT_MODE,
                // The Black Hat: open the field computer (one frame); its raise hands control
                // back to the RTS view (the retake at 470 brings first person up again).
                395 => pressed |= buttons::FIELD_COMPUTER,
                // Bring the hammer up, place a structure, put it away (single-frame presses).
                520 => pressed |= buttons::HAMMER,
                545 => pressed |= buttons::FIRE,
                565 => pressed |= buttons::HAMMER,
                _ => {}
            }
            script.push(FpsInput {
                held,
                pressed,
                move_packed: pack_move(fwd, side),
                yaw,
                pitch,
            });
        }
        script
    };

    let make_world = || {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(soldier(2, 1, 60.0));
        host.add(soldier(3, 1, 90.0));
        host.options.borrow_mut().push(crate::building::BuildOption {
            template: 7,
            cost: 100,
            status: crate::building::status::OK,
            radius: 20.0,
            ..Default::default()
        });
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        // The lethal slot plants (a claymore) so the equipment path is exercised alongside the
        // thrown tactical.
        if let Some(p) = sim.players[0].as_mut() {
            p.arsenal = crate::weapons::Arsenal::with_loadout(
                crate::weapons::Loadout::test_kit().with_spec("lethal=claymore_mp"),
            );
        }
        (host, sim)
    };
    let (host_a, mut sim_a) = make_world();
    let (host_b, mut sim_b) = make_world();

    for input in &script {
        let f = sim_a.frame + 1;
        // Take control again after the field computer's RTS handoff (tests re-entry).
        if f % 600 == 470 {
            sim_a.take_control(&host_a, 0, 1);
            sim_b.take_control(&host_b, 0, 1);
        }
        sim_a.input(0, 1, *input);
        sim_a.step(&host_a, f);
        sim_b.input(0, 1, *input);
        sim_b.step(&host_b, f);
        assert_eq!(events(&sim_a), events(&sim_b), "event drift at frame {}", f);
        assert_eq!(host_a.objects, host_b.objects, "object drift at frame {}", f);
    }

    // The final state agrees: where the General is, what he holds, what the engine saw (a run
    // may legitimately end with the General dead on his own explosives — then he is dead in
    // both).
    assert_eq!(sim_a.player(0).is_none(), sim_b.player(0).is_none(), "the General's fate agrees");
    assert_eq!(host_a.get(1).pos, host_b.get(1).pos, "the General ended up in the same place");
    if let (Some(a), Some(b)) = (sim_a.player(0), sim_b.player(0)) {
        assert_eq!(a.arsenal, b.arsenal, "the whole arsenal agrees (rounds, clips, cooldowns)");
    }
    assert_eq!(sim_a.projectiles.len(), sim_b.projectiles.len());
    assert_eq!(sim_a.equipment, sim_b.equipment);
    assert_eq!(*host_a.spawned.borrow(), *host_b.spawned.borrow());
    assert_eq!(*host_a.sites.borrow(), *host_b.sites.borrow());
    // And the run actually exercised the systems (a silent no-op run would prove nothing).
    let kinds = events(&sim_a);
    assert!(kinds.iter().any(|k| matches!(k, EventKind::Shot { .. })), "fired");
    assert!(kinds.iter().any(|k| matches!(k, EventKind::Planted { .. })), "planted the claymore");
    assert!(kinds.contains(&EventKind::Thrown { lethal: false }), "threw a tactical");
    assert!(kinds.contains(&EventKind::Launched), "fired the launcher alt mode");
    assert!(kinds.contains(&EventKind::ReloadStarted), "reloaded");
    assert!(kinds.contains(&EventKind::FieldComputerOpened), "hacked");
    assert!(kinds.iter().any(|k| matches!(k, EventKind::Exploded { .. })), "something went off");
    assert!(kinds.iter().any(|k| matches!(k, EventKind::Melee)), "swung the knife");
    assert!(!host_a.sites.borrow().is_empty(), "built");
}

#[test]
fn take_control_suspends_ai_and_moves_general() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    assert_eq!(*host.controlled.borrow(), vec![1]);
    run(&mut sim, &host, 0, 1, FpsInput { move_packed: pack_move(1.0, 0.0), ..Default::default() }, 30);
    assert!(host.get(1).pos.x > 20.0, "walked forward: {:?}", host.get(1).pos);
    assert_eq!(sim.controlled_ids().collect::<Vec<_>>(), vec![1]);
}

/// The ADS walk through the whole sim: an SMG (BO2 `adsMoveSpeedScale` 2.0) with Stalker
/// (`stalker_smg` 2.55) walks aimed at 2.55/2.0 of the plain SMG's aimed speed — the
/// attachment replaces the weapon's value (`FUN_008ed080`), it does not multiply it.
#[test]
fn an_aimed_stalker_smg_walks_at_the_attachments_ads_speed() {
    use crate::attachments::{apply, AttachmentDef};
    use crate::weapons::{Class, WeaponDef, WeaponState, ADS_MOVE_BASE};
    let smg = WeaponDef { id: "smg_mp".into(), class: Class::Smg, ads_move_scale: ADS_MOVE_BASE * 2.0, ads_in_time: 0.2, ..Default::default() };
    let stalker = AttachmentDef { id: "stalker_smg".into(), short: "stalker".into(), ads_move_scale: 2.55, point: 5, ..Default::default() };
    let with_stalker = apply(&smg, &[&stalker]);
    let aimed_walk = |def: WeaponDef| {
        let def: &'static WeaponDef = Box::leak(Box::new(def));
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        let p = sim.players[0].as_mut().unwrap();
        p.arsenal.weapons = vec![WeaponState::new(def)];
        p.arsenal.active = 0;
        p.arsenal.raise_left = 0.0;
        let aim = FpsInput { held: buttons::ADS, move_packed: pack_move(1.0, 0.0), ..Default::default() };
        // Fully aimed and up to speed, then measure a second of walking.
        run(&mut sim, &host, 0, 1, aim, frames(1.0));
        assert_eq!(sim.player(0).unwrap().arsenal.ads, 1.0, "fully aimed");
        let from = host.get(1).pos.x;
        run(&mut sim, &host, 0, 1, aim, frames(1.0));
        host.get(1).pos.x - from
    };
    let plain = aimed_walk(smg.clone());
    let stalked = aimed_walk(with_stalker);
    assert!(plain > 0.0, "walked aimed: {plain}");
    assert!((stalked / plain - 2.55 / 2.0).abs() < 0.01, "stalker {stalked} vs plain {plain}");
}

#[test]
fn cannot_control_someone_elses_general() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 1, 1);
    assert!(sim.player(1).is_none());
    assert!(host.controlled.borrow().is_empty());
}

/// BO2 gives the held knife as a second weapon to a class with fewer than two
/// (`_class.gsc` `giveloadout`, `initialweaponcount < 2`): a one-gun class spawns holding the
/// gun, and the weapon switch brings up the knife, which strikes with itself.
#[test]
fn a_one_gun_class_carries_the_held_knife_as_its_second_weapon() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    sim.players[0].as_mut().unwrap().arsenal = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout { primary: "mp7_mp".into(), ..crate::weapons::Loadout::test_kit().with_spec("secondary=") });
    wait_until_ready(&mut sim, &host);
    let a = &sim.player(0).unwrap().arsenal;
    let ids: Vec<&str> = a.weapons.iter().map(|w| w.def.id.as_str()).collect();
    assert_eq!(ids, ["mp7_mp", "knife_held_mp"]);
    assert_eq!(a.active, 0, "it spawns holding the gun");
    assert!(!a.is_unarmed());
    run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::SWAP_WEAPON, ..Default::default() }, 1);
    run(&mut sim, &host, 0, 1, FpsInput::default(), frames(3.0));
    let a = &sim.player(0).unwrap().arsenal;
    assert_eq!(a.current().def.id, "knife_held_mp", "the switch brings up the knife");
    assert_eq!(a.melee_weapon().id, "knife_held_mp", "a knife in the hand strikes with itself");
    // Two guns: no knife in the hand (it is still the melee); no guns: the knife alone.
    let two = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout::test_kit());
    assert!(two.weapons.iter().all(|w| w.def.class.is_gun()) && two.weapons.len() == 2);
    let none = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout::unarmed());
    assert_eq!(none.weapons.len(), 1);
}

/// Bug 1: a General spawns with no guns and no grenades, only the melee knife, until a class
/// is chosen. `Loadout::test_kit()` (the classes' base kit) is untouched.
#[test]
fn a_fresh_general_spawns_unarmed() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    let p = sim.player(0).unwrap();
    assert!(p.class.is_none());
    let a = &p.arsenal;
    assert!(a.is_unarmed(), "no gun at spawn");
    assert_eq!(a.weapons.len(), 1, "the knife is the only weapon");
    assert_eq!(a.current().def.class, crate::weapons::Class::Melee);
    assert_eq!(a.melee.id, "knife_held_mp");
    assert_eq!((a.lethal.count, a.tactical.count), (0, 0), "no grenades");
    assert!(a.loadout.primary.is_empty() && a.loadout.secondary.is_empty());
    assert!(a.loadout.perks.is_empty());
    // The full base kit is still what the classes build on.
    let base = crate::weapons::Loadout::test_kit();
    assert!(!base.primary.is_empty() && !base.secondary.is_empty() && !base.lethal.is_empty());
}

/// Unarmed is a no-op for every gun action (no panic, no shot, no index-0 fallback); the knife
/// still swings, and the hammer and field computer still work.
#[test]
fn unarmed_weapon_input_is_a_no_op_and_melee_still_works() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    wait_until_ready(&mut sim, &host);
    let everything = buttons::FIRE
        | buttons::ADS
        | buttons::RELOAD
        | buttons::SWAP_WEAPON
        | buttons::ALT_MODE
        | buttons::LETHAL
        | buttons::TACTICAL;
    run(&mut sim, &host, 0, 1, FpsInput { held: everything, ..Default::default() }, 40);
    let kinds = events(&sim);
    for kind in [EventKind::Swapped, EventKind::AltToggled, EventKind::ReloadStarted, EventKind::Launched, EventKind::Detonated] {
        assert!(!kinds.contains(&kind), "unarmed did not {kind:?}: {kinds:?}");
    }
    assert!(!kinds.iter().any(|k| matches!(k, EventKind::Shot { .. } | EventKind::Thrown { .. } | EventKind::OffhandReady { .. })), "{kinds:?}");
    assert_eq!(sim.player(0).unwrap().arsenal.ads, 0.0, "nothing to aim");
    assert!(sim.projectiles.is_empty());
    // The knife: BO2 always has melee.
    sim.events.clear();
    run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::MELEE, ..Default::default() }, 1);
    assert!(events(&sim).contains(&EventKind::Melee));
}

/// After a melee with a gun up the General's gun only fires again once the swing (plain time)
/// and its quick re-raise (BO2's state 0x15, Fast Melee Recovery's) are over; Dexterity
/// shortens only the re-raise.
#[test]
fn the_gun_fires_again_after_the_melee_and_its_re_raise() {
    let first_shot = |perks: &str| {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.players[0].as_mut().unwrap().arsenal = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout::test_kit().with_spec(perks));
        wait_until_ready(&mut sim, &host);
        run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::MELEE, ..Default::default() }, 1);
        sim.events.clear();
        let a = &sim.player(0).unwrap().arsenal;
        let (swing, quick) = (a.melee_left, a.current().def.quick_raise_time);
        for f in 1..120u32 {
            run(&mut sim, &host, 0, 1, FpsInput { held: buttons::FIRE, ..Default::default() }, 1);
            if events(&sim).iter().any(|k| matches!(k, EventKind::Shot { .. })) {
                return (f, swing, quick);
            }
        }
        panic!("never fired");
    };
    let (plain, swing, quick) = first_shot("perks=");
    let (fast, ..) = first_shot("perks=Dexterity");
    let want = |raise: f32| ((swing / LOGIC_DT - 1e-4).ceil() + (raise / LOGIC_DT - 1e-4).ceil()) as i64;
    let m = crate::tuning::get().perk_weap_melee_multiplier;
    assert!((plain as i64 - want(quick)).abs() <= 1, "fired at {plain}, want {}", want(quick));
    assert!((fast as i64 - want(quick * m)).abs() <= 1, "with Dexterity fired at {fast}, want {}", want(quick * m));
    assert!(fast < plain);
}

/// An offhand pressed mid-raise or mid-shot waits that state's time out (BO2's 0x16,
/// `FUN_008dcb70` keeps `weaponTime`), then pulls back; pressed at rest it pulls back at once.
#[test]
fn an_offhand_pressed_mid_raise_or_mid_shot_waits_that_state_out() {
    use crate::weapons::OffhandUse;
    // Frames from the press to the pull-back, and the time the interrupted state still had.
    let pull_back_after = |setup: &dyn Fn(&mut Sim, &FakeHost) -> f32| {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.players[0].as_mut().unwrap().arsenal = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout::test_kit());
        let left = setup(&mut sim, &host);
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::LETHAL, pressed: buttons::LETHAL, ..Default::default() }, 1);
        for f in 0..120u32 {
            let a = &sim.player(0).unwrap().arsenal;
            match a.offhand {
                Some(OffhandUse::Holding { .. }) => return (f, left),
                Some(OffhandUse::Starting { .. }) => assert!(a.is_busy()),
                other => panic!("frame {f}: {other:?}"),
            }
            run(&mut sim, &host, 0, 1, FpsInput { held: buttons::LETHAL, ..Default::default() }, 1);
        }
        panic!("never pulled back");
    };
    let want = |left: f32| (left / LOGIC_DT - 1e-4).ceil() as i64 - 1;
    // At rest: at once.
    let (rest, _) = pull_back_after(&|sim, host| {
        wait_until_ready(sim, host);
        0.0
    });
    assert_eq!(rest, 0);
    // Mid-raise: when the raise would have ended.
    let (raise, left) = pull_back_after(&|sim, host| {
        run(sim, host, 0, 1, FpsInput::default(), 5);
        // The raise's time is left; 0x16 runs it at the offhand's (Fast Toss's) rate.
        let a = &sim.player(0).unwrap().arsenal;
        a.raise_left * a.multiplier(crate::weapons::Timing::Toss)
    });
    assert!(left > 0.1, "still raising ({left})");
    assert!((raise as i64 - want(left)).abs() <= 1, "pulled back after {raise} frames, want {}", want(left));
    assert_eq!(sim_replay_raise(), raise, "deterministic");
    // Mid-shot: after the fire time.
    let (shot, fire_time) = pull_back_after(&|sim, host| {
        wait_until_ready(sim, host);
        run(sim, host, 0, 1, FpsInput { held: buttons::FIRE, pressed: buttons::FIRE, ..Default::default() }, 1);
        let a = &sim.player(0).unwrap().arsenal;
        a.current().def.fire_time * a.multiplier(crate::weapons::Timing::Toss)
    });
    assert!((shot as i64 - want(fire_time)).abs() <= 1, "pulled back after {shot} frames, want {}", want(fire_time));
    assert!(shot > 0);

    fn sim_replay_raise() -> u32 {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.players[0].as_mut().unwrap().arsenal = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout::test_kit());
        run(&mut sim, &host, 0, 1, FpsInput::default(), 5);
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::LETHAL, pressed: buttons::LETHAL, ..Default::default() }, 1);
        (0..120u32)
            .find(|_| {
                let holding = matches!(sim.player(0).unwrap().arsenal.offhand, Some(crate::weapons::OffhandUse::Holding { .. }));
                run(&mut sim, &host, 0, 1, FpsInput { held: buttons::LETHAL, ..Default::default() }, 1);
                holding
            })
            .unwrap()
    }
}

/// `offhandHoldIsCancelable`: held up after the pull-back, the weapon-switch button's release
/// puts the item away unused (the gun's quick raise; the count and nothing thrown); an item
/// without it, or during the pull-back, ignores the button and throws on the release.
#[test]
fn the_switch_button_cancels_a_held_cancelable_offhand() {
    use crate::weapons::{Offhand, OffhandUse, WeaponDef};
    // (count left, thrown, the offhand after the switch button came up)
    let try_cancel = |cancelable: bool, pull_back: f32, hold_frames: u32| {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.players[0].as_mut().unwrap().arsenal = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout::test_kit());
        wait_until_ready(&mut sim, &host);
        let base = sim.player(0).unwrap().arsenal.lethal.def;
        let def: &'static WeaponDef = Box::leak(Box::new(WeaponDef { hold_cancelable: cancelable, timing: crate::weapons::CycleTimes { hold_fire_time: pull_back, ..base.timing }, ..base.clone() }));
        sim.players[0].as_mut().unwrap().arsenal.lethal = Offhand { def, count: 2 };
        sim.events.clear();
        let hold = |extra: u32| FpsInput { held: buttons::LETHAL | extra, ..Default::default() };
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::LETHAL, pressed: buttons::LETHAL, ..Default::default() }, 1);
        run(&mut sim, &host, 0, 1, hold(0), hold_frames);
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::LETHAL | buttons::SWAP_WEAPON, pressed: buttons::SWAP_WEAPON, ..Default::default() }, 1);
        run(&mut sim, &host, 0, 1, hold(0), 1);
        let after = sim.player(0).unwrap().arsenal.offhand;
        run(&mut sim, &host, 0, 1, FpsInput::default(), 90);
        let thrown = events(&sim).iter().any(|k| matches!(k, EventKind::Thrown { .. }));
        (sim.player(0).unwrap().arsenal.lethal.count, thrown, after)
    };
    let (count, thrown, after) = try_cancel(true, 0.2, 30);
    assert!(matches!(after, Some(OffhandUse::Raising { .. })), "put away: {after:?}");
    assert_eq!((count, thrown), (2, false), "unused");
    assert_eq!(try_cancel(true, 0.2, 30), (count, thrown, after), "deterministic");
    let (count, thrown, _) = try_cancel(false, 0.2, 30);
    assert_eq!((count, thrown), (1, true), "not cancelable: thrown on the release");
    let (count, thrown, _) = try_cancel(true, 2.0, 0);
    assert_eq!((count, thrown), (1, true), "during the pull-back the button does nothing");
}

/// BO2's plant check (`FUN_008e1780`) on a `plantable` item: refused in the air at the press
/// (nothing starts) and over deep water while held (put away); either way nothing is used and
/// "Cannot Plant Here" is reported. On dry ground it is set down as before.
#[test]
fn a_plantable_item_is_refused_in_the_air_and_over_deep_water() {
    use crate::weapons::{Offhand, OffhandUse, WeaponDef};
    let setup = || {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.players[0].as_mut().unwrap().arsenal = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout::test_kit());
        wait_until_ready(&mut sim, &host);
        let base = crate::weapons::weapon("claymore_mp");
        // BO2's claymore throw: 100 forward, -10000 up (straight down).
        let def: &'static WeaponDef = Box::leak(Box::new(WeaponDef {
            plantable: true,
            projectile_speed: crate::units::inches(100.0),
            projectile_speed_up: crate::units::inches(-10000.0),
            ..base.clone()
        }));
        sim.players[0].as_mut().unwrap().arsenal.lethal = Offhand { def, count: 1 };
        sim.events.clear();
        (host, sim)
    };
    let press = FpsInput { held: buttons::LETHAL, pressed: buttons::LETHAL, ..Default::default() };
    let refused = |sim: &Sim| events(sim).iter().any(|k| matches!(k, EventKind::CantPlant { lethal: true }));
    let planted = |sim: &Sim| events(sim).iter().any(|k| matches!(k, EventKind::Planted { .. }));

    // In the air: the press does nothing.
    let (host, mut sim) = setup();
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::JUMP, pressed: buttons::JUMP, ..Default::default() }, 2);
    assert!(!sim.player(0).unwrap().body.on_ground, "airborne");
    run(&mut sim, &host, 0, 1, press, 1);
    assert!(refused(&sim));
    assert!(sim.player(0).unwrap().arsenal.offhand.is_none(), "nothing started");
    assert_eq!(sim.player(0).unwrap().arsenal.lethal.count, 1);

    // Held, then over deep water: put away unused.
    let (host, mut sim) = setup();
    run(&mut sim, &host, 0, 1, press, 1);
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::LETHAL, ..Default::default() }, 30);
    assert!(matches!(sim.player(0).unwrap().arsenal.offhand, Some(OffhandUse::Holding { .. })) && !refused(&sim));
    host.deep_water.set(true);
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::LETHAL, ..Default::default() }, 1);
    assert!(refused(&sim));
    assert!(matches!(sim.player(0).unwrap().arsenal.offhand, Some(OffhandUse::Raising { .. })));
    run(&mut sim, &host, 0, 1, FpsInput::default(), 60);
    assert!(!planted(&sim));
    assert_eq!(sim.player(0).unwrap().arsenal.lethal.count, 1);

    // Dry ground: set down.
    let (host, mut sim) = setup();
    run(&mut sim, &host, 0, 1, press, 1);
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::LETHAL, ..Default::default() }, 30);
    run(&mut sim, &host, 0, 1, FpsInput::default(), 60);
    assert!(planted(&sim) && !refused(&sim));
    assert_eq!(sim.player(0).unwrap().arsenal.lethal.count, 0);
}

/// A switch pressed right after a shot waits for the shot's time (`FUN_008e0e60` holds it in
/// state 6 while `weaponTime` runs), which the fire-rate specialty shortens as it does the shot.
#[test]
fn a_switch_pressed_after_a_shot_happens_when_the_shot_ends() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    sim.players[0].as_mut().unwrap().arsenal = crate::weapons::Arsenal::with_loadout(crate::weapons::Loadout::test_kit());
    wait_until_ready(&mut sim, &host);
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::FIRE, pressed: buttons::FIRE, ..Default::default() }, 1);
    let a = &sim.player(0).unwrap().arsenal;
    let shot = a.current().def.fire_time * a.multiplier(crate::weapons::Timing::Fire);
    assert!(shot > 2.0 * LOGIC_DT, "the stand-in's shot spans frames ({shot})");
    sim.events.clear();
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::SWAP_WEAPON, pressed: buttons::SWAP_WEAPON, ..Default::default() }, 1);
    let mut at = None;
    for f in 1..120u32 {
        if events(&sim).iter().any(|k| matches!(k, EventKind::Swapped)) {
            at = Some(f);
            break;
        }
        run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
    }
    let at = at.expect("switched") as i64;
    // The shot's frames, the press being the second.
    let want = (shot / LOGIC_DT - 1e-4).ceil() as i64 - 1;
    assert!((at - want).abs() <= 1, "switched {at} frames after the press, want {want}");
    assert!(at > 1, "not on the press");
}

#[test]
fn the_hammer_and_field_computer_work_unarmed() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.options.borrow_mut().push(option(7, crate::building::status::OK));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    wait_until_ready(&mut sim, &host);
    assert!(sim.player(0).unwrap().arsenal.is_unarmed());
    let look = |held: u32, pressed: u32| FpsInput { held, pressed, pitch: -0.5, ..Default::default() };
    run(&mut sim, &host, 0, 1, look(buttons::HAMMER, buttons::HAMMER), 1);
    run(&mut sim, &host, 0, 1, look(0, 0), frames(crate::building::HAMMER_RAISE));
    run(&mut sim, &host, 0, 1, look(buttons::FIRE, buttons::FIRE), 1);
    assert!(events(&sim).contains(&EventKind::BuildPlaced { template: 7 }), "{:?}", events(&sim));
    sim.events.clear();
    run(&mut sim, &host, 0, 1, look(buttons::FIELD_COMPUTER, buttons::FIELD_COMPUTER), 1);
    assert!(events(&sim).contains(&EventKind::FieldComputerOpened), "{:?}", events(&sim));
}

/// Bug 1, overhead half: a classless General reports no usable slot/ammo, so the bridge's
/// "an empty weapon can't fire" veto holds Zero Hour's own weapon; after a class lands the kit
/// fires exactly as before, and the RTS update never re-creates the record over the kit.
#[test]
fn a_classless_general_under_rts_cannot_fire_then_fires_with_its_class_kit() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    // The bridge calls rts_update every logic frame; the first call makes the record unarmed.
    let r = sim.rts_update(&host, 0, 1, 0, [3, 3, 3], None);
    assert_eq!(r.ammo, [0, 0, 0]);
    assert_eq!(r.reserve, [0, 0, 0]);
    assert_eq!(r.lock, -1, "nothing to switch to");
    assert_eq!(sim.rts_ammo(1).ammo, [0, 0, 0], "the control bar shows no usable weapon");
    assert!(sim.projectiles.is_empty(), "the third slot threw nothing");
    // A class lands from the command bar: the kit is there immediately.
    sim.choose_class(0, 1, 0, &[]);
    let full = sim.rts_ammo(1);
    assert!(full.ammo[0] > 0 && full.ammo[1] > 0, "the class kit's guns: {full:?}");
    let r = sim.rts_update(&host, 0, 1, 0, [1, 0, 0], None);
    assert_eq!((r.ammo[0], r.reserve[0]), (full.ammo[0] - 1, full.reserve[0]));
    assert_eq!(sim.player(0).unwrap().arsenal.weapons[0].main.id, "tar21_mp", "the kit was not reset");
    assert_eq!(sim.player(0).unwrap().class.as_deref(), Some("assault"));
}

/// Bug 3, overhead half: a custom class picked while commanding keeps its exact kit through
/// `take_control` (which must not re-create the record or re-apply the base kit), and the
/// weapon is ready as soon as first person starts.
#[test]
fn an_overhead_custom_pick_keeps_its_kit_through_take_control() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    // The bridge's per-frame rts_update makes the (unarmed) record before the pick.
    sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None);
    let custom = crate::classes::CustomClass {
        name: "Overhead".into(),
        primary: "fnp45_mp".into(),
        secondary: "an94_mp".into(),
        lethal: "claymore_mp".into(),
        tactical: "concussion_grenade_mp".into(),
        melee: "knife_held_mp".into(),
        perks: vec!["Toughness".into()],
        ..Default::default()
    };
    sim.choose_class(0, 1, usize::MAX, &custom.to_wire_words());
    // In overhead the kit is ready now (no first-person raise).
    assert_eq!(sim.player(0).unwrap().arsenal.raise_left, 0.0);
    sim.take_control(&host, 0, 1);
    let p = sim.player(0).unwrap();
    assert_eq!(p.class.as_deref(), Some("custom:Overhead"));
    let a = &p.arsenal;
    assert_eq!(a.weapons[0].main.id, "fnp45_mp");
    assert_eq!(a.weapons[1].main.id, "an94_mp");
    assert_eq!(a.lethal.def.id, "claymore_mp");
    assert!(a.has(crate::weapons::Perk::TOUGHNESS));
    assert!(a.raise_left >= TAKE_CONTROL_READY, "the weapon still comes up in first person");
    // A later rts_update does not re-create/reset the record.
    sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None);
    assert_eq!(sim.player(0).unwrap().arsenal.weapons[0].main.id, "fnp45_mp");
    // A re-pick in first person (the spawn offer on a later take-control) replaces the kit.
    let second = crate::classes::CustomClass {
        name: "Rifle".into(),
        primary: "an94_mp".into(),
        secondary: "fnp45_mp".into(),
        lethal: "frag_grenade_mp".into(),
        tactical: "concussion_grenade_mp".into(),
        melee: "knife_held_mp".into(),
        ..Default::default()
    };
    sim.choose_class(0, 1, usize::MAX, &second.to_wire_words());
    let a = &sim.player(0).unwrap().arsenal;
    assert_eq!(a.weapons[0].main.id, "an94_mp", "the re-pick replaced the first kit");
    assert!(a.raise_left >= TAKE_CONTROL_READY, "the first-person pick re-raises");
}

#[test]
fn custom_class_pick_logs_its_own_name() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    let custom = crate::classes::CustomClass {
        name: "Ghost Recon".into(),
        ..Default::default()
    };
    let words = custom.to_wire_words();
    sim.choose_class(0, 1, usize::MAX, &words);
    let ev = events(&sim);
    let logged = ev.iter().find_map(|e| match e {
        EventKind::ClassChosen { class, .. } => Some(*class),
        _ => None,
    });
    match logged {
        Some(name) => assert_eq!(name.as_str(), "Ghost Recon"),
        other => panic!("expected custom class name, got {other:?}"),
    }
}

#[test]
fn a_custom_class_carries_its_camo_choice() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    let custom = crate::classes::CustomClass {
        primary: "an94_mp".into(),
        secondary: "fiveseven_mp".into(),
        camo: "camo_gold".into(),
        knife: "camo_diamond".into(),
        secondary_camo: "camo_atacs".into(),
        reticle: "reticle_4".into(),
        secondary_reticle: "reticle_12".into(),
        ..Default::default()
    };
    sim.choose_class(0, 1, usize::MAX, &custom.to_wire_words());
    assert_eq!(sim.player(0).unwrap().camo, "camo_gold");
    // Each slot's optic carries its own reticle (`primaryreticle` / `secondaryreticle`), and the
    // one in hand is the slot's: the primary's until the sidearm comes up.
    let p = sim.player(0).unwrap();
    assert_eq!(p.held_reticles(), [("an94_mp", "reticle_4"), ("fiveseven_mp", "reticle_12")]);
    assert_eq!(p.held_reticle(), "reticle_4");
    let mut p = p.clone();
    p.arsenal.active = p.arsenal.weapons.iter().position(|w| w.def.id == "fiveseven_mp").expect("the sidearm is carried");
    assert_eq!(p.held_reticle(), "reticle_12");
    assert_eq!(sim.player(0).unwrap().knife_camo, "camo_diamond", "the knife carries its own camo");
    assert_eq!(sim.player(0).unwrap().secondary_camo, "camo_atacs", "the sidearm carries its own camo");
    // What the client hands the viewmodel draw: each held weapon with its own camo, the
    // sidearm with `secondarycamo` (`_class.gsc`'s `calcweaponoptions(class, 1)`).
    assert_eq!(
        sim.player(0).unwrap().held_camos(),
        [("an94_mp", "camo_gold"), ("fiveseven_mp", "camo_atacs"), ("knife_held_mp", "camo_diamond")]
    );
}

#[test]
fn class_chosen_from_overhead_applies_its_kit() {
    // A General built from the Command Center and still under RTS control: the class picked
    // from the command bar must replace the base kit without first-person control. He has no
    // `PlayerSim` yet (he was never taken into first person), so the pick creates the record.
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    assert!(sim.player(0).is_none(), "no record before the pick");
    sim.choose_class(0, 1, 0, &[]);
    let p = sim.player(0).expect("the pick made the record");
    assert_eq!(p.class.as_deref(), Some(crate::classes::at(0).unwrap().id.as_str()));
    let ev = events(&sim);
    assert!(
        ev.iter().any(|e| matches!(e, EventKind::ClassChosen { .. })),
        "the overhead pick emitted ClassChosen"
    );
}

#[test]
fn class_pick_replaces_an_existing_class() {
    // Re-opening the pick screen (the lobby/command-bar button) and choosing again swaps the
    // kit instead of being ignored.
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    let first = sim.player(0).and_then(|p| p.class.clone());
    sim.choose_class(0, 1, 1, &[]);
    let second = sim.player(0).and_then(|p| p.class.clone());
    assert_ne!(first, second, "the second pick replaced the first");
}

/// One player firing `burst` frames then idling `rest`, from a raised gun: the view kick after
/// every frame, and whether the frame fired.
fn kick_trace(input: FpsInput, burst: u32, rest: u32) -> Vec<(ViewKick, bool, f32, f32)> {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    // The stand-in AN-94 carries BO2's kick and recentre numbers.
    let rifle = crate::classes::CustomClass { name: "Rifle".into(), primary: "an94_mp".into(), secondary: "fnp45_mp".into(), ..Default::default() };
    sim.choose_class(0, 1, usize::MAX, &rifle.to_wire_words());
    wait_until_ready(&mut sim, &host);
    let idle = FpsInput { held: input.held & buttons::ADS, ..input };
    let mut trace = Vec::new();
    for f in 0..burst + rest {
        let before = sim.player(0).unwrap().view_kick;
        let now = if f < burst { input } else { idle };
        sim.events.clear();
        sim.input(0, 1, now);
        let n = sim.frame + 1;
        sim.step(&host, n);
        let p = sim.player(0).unwrap();
        // The frame aimed along the view plus the kick the last frame left (`FUN_00549fb0`).
        let (ky, kp) = before.aim_offset();
        assert!((p.pitch - (now.pitch + kp)).abs() < 1e-6 && (p.yaw - crate::math::wrap_angle(now.yaw + ky)).abs() < 1e-6, "frame {f}");
        let shot = sim.events.iter().any(|e| matches!(e.kind, EventKind::Shot { .. }));
        let centre = view_kick::centre_speed(p.arsenal.current().def, p.arsenal.ads, 0);
        if shot {
            assert!(p.view_kick.angle[0] < before.angle[0], "a shot kicks up: frame {f} {before:?} -> {:?}", p.view_kick);
        } else {
            let mut want = before;
            want.step(LOGIC_DT, centre);
            assert_eq!(p.view_kick, want, "frame {f}: the integrator alone");
        }
        trace.push((p.view_kick, shot, p.arsenal.ads, centre));
    }
    trace
}

#[test]
fn a_burst_kicks_the_aim_up_and_recentres_on_the_integrator() {
    let trace = kick_trace(FpsInput { held: buttons::FIRE, ..Default::default() }, 30, 45);
    let shots = trace.iter().filter(|t| t.1).count();
    assert!(shots >= 3, "the burst fired: {shots}");
    let peak = trace.iter().map(|t| -t.0.angle[0]).fold(0.0f32, f32::max);
    assert!(peak > 1.0 && peak <= view_kick::ANGLE_MAX, "the kick climbs over the burst: {peak}");
    assert!(trace.last().unwrap().0.is_rest(), "then recentres all the way, exactly");
    for (f, (k, shot, ads, centre)) in trace.iter().enumerate() {
        println!("KICK {f} {} {} {} {} {ads} {centre}", -k.angle[0], k.angle[1], k.angle[2], *shot as u8);
    }
}

#[test]
fn the_kick_replays_identically_and_aims_the_bullets() {
    let fire = FpsInput { held: buttons::FIRE | buttons::ADS, pitch: 0.1, yaw: 0.3, ..Default::default() };
    let a = kick_trace(fire, 20, 30);
    let b = kick_trace(fire, 20, 30);
    assert_eq!(a, b, "same inputs, same kick");
    assert!(a.iter().any(|t| t.0.angle[1] != 0.0), "the yaw kicks too");
}

/// A raised scoped sniper (BO2-shaped idle numbers) in the player's hands.
fn scoped(sim: &mut Sim, host: &FakeHost) {
    sim.take_control(host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    wait_until_ready(sim, host);
    let mut d = WeaponDef { id: "test_scope_mp".into(), class: Class::Sniper, ads_fov: 15.0, ads_in_time: 0.1, ads_out_time: 0.1, overlay_reticle: 1, clip: 5, fire_time: 0.5, max_range: 4000.0, ..Default::default() };
    d.zoom.idle_amount = (60.0, 20.0);
    d.zoom.idle_speed = (1.0, 0.5);
    d.zoom.idle_stance = (0.5, 0.25);
    d.zoom.hold_breath = true;
    let p = sim.players[0].as_mut().unwrap();
    let w = p.arsenal.active;
    p.arsenal.weapons[w].def = Box::leak(Box::new(d));
    p.arsenal.weapons[w].ammo = 5;
}

/// The aim each frame while aimed for `frames`, with `held` extra buttons, and the idle sway.
fn sway_trace(held: u32, frames: u32) -> Vec<(f32, f32, IdleSway)> {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    scoped(&mut sim, &host);
    let aim = FpsInput { held: buttons::ADS | held, pitch: 0.05, yaw: 0.2, ..Default::default() };
    let mut out = Vec::new();
    for _ in 0..frames {
        let before = sim.player(0).unwrap().aim_offset();
        run(&mut sim, &host, 0, 1, aim, 1);
        let p = sim.player(0).unwrap();
        // The frame aims along the view plus the sway the last frame left.
        assert!((p.pitch - (aim.pitch + before.1)).abs() < 1e-6 && (p.yaw - (aim.yaw + before.0)).abs() < 1e-6);
        out.push((p.yaw, p.pitch, p.idle));
    }
    out
}

#[test]
fn a_scope_sways_the_aim_and_holding_breath_steadies_it() {
    let free = sway_trace(0, 150);
    let swing = |t: &[(f32, f32, IdleSway)]| {
        let p: Vec<f32> = t.iter().map(|x| x.1).collect();
        p.iter().cloned().fold(f32::MIN, f32::max) - p.iter().cloned().fold(f32::MAX, f32::min)
    };
    // 60 × 0.01 = 0.6° each way at full ADS standing: the aim swings ~1.2° over a period.
    let free_swing = swing(&free[30..]).to_degrees();
    assert!(free_swing > 0.9 && free_swing < 1.25, "{free_swing}");
    assert_eq!(free, sway_trace(0, 150), "replays identically");
    // Holding the breath key (sprint) once aimed: the drift dies away while held.
    let held = sway_trace(buttons::SPRINT, 150);
    assert!(held[100].2.breath.holding);
    let steady = swing(&held[80..130]).to_degrees();
    assert!(steady < 0.05, "held breath: {steady}°");
    // 4.5 s later the breath runs out and the gasp sways harder than at rest.
    let long = sway_trace(buttons::SPRINT, 260);
    assert!(!long[200].2.breath.holding && long[200].2.breath.scale > 1.5, "{:?}", long[200].2.breath);
}

/// An aimed iron-sight burst while turning: (aim, fire direction) per frame.
fn iron_trace() -> Vec<(f32, f32, f32, f32)> {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    scoped(&mut sim, &host);
    // Iron sights: no overlay; BO2-shaped aimed sway and idle numbers.
    let p = sim.players[0].as_mut().unwrap();
    let w = p.arsenal.active;
    let mut d = p.arsenal.weapons[w].def.clone();
    (d.class, d.overlay_reticle, d.fire_time) = (Class::Rifle, 0, 0.1);
    d.ads_sway = crate::weapons::Sway { max_angle: 5.0, lerp_speed: 8.0, pitch_scale: 0.3, yaw_scale: 0.3, horiz_scale: 0.1, vert_scale: 0.1 };
    d.zoom.idle_amount = (10.0, 15.0);
    p.arsenal.weapons[w].def = Box::leak(Box::new(d));
    p.arsenal.weapons[w].ammo = 30;
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS, ..Default::default() }, 20);
    let mut out = Vec::new();
    for f in 0..30 {
        let yaw = f as f32 * 0.01;
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS | buttons::FIRE, yaw, ..Default::default() }, 1);
        let p = sim.player(0).unwrap();
        out.push((p.yaw, p.pitch, p.fire_yaw, p.fire_pitch));
    }
    out
}

#[test]
fn an_aimed_iron_sight_shot_follows_the_gun_pose() {
    let a = iron_trace();
    assert_eq!(a, iron_trace(), "replays identically");
    let off = a.iter().map(|t| (t.2 - t.0).abs() + (t.3 - t.1).abs()).fold(0.0f32, f32::max);
    // The gun kick spring and the turn's sway move the sights off the view centre a little.
    assert!(off > 1e-4 && off < 5f32.to_radians(), "{off}");
    // The scoped sniper's shot is the view's (its sway is the view's own).
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    scoped(&mut sim, &host);
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS, yaw: 0.3, ..Default::default() }, 20);
    let p = sim.player(0).unwrap();
    assert_eq!((p.fire_yaw, p.fire_pitch), (p.yaw, p.pitch));
}

#[test]
fn a_shot_lets_a_held_breath_go() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    scoped(&mut sim, &host);
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS | buttons::SPRINT, ..Default::default() }, 30);
    assert!(sim.player(0).unwrap().idle.breath.holding);
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS | buttons::SPRINT | buttons::FIRE, ..Default::default() }, 1);
    assert!(events(&sim).iter().any(|e| matches!(e, EventKind::Shot { .. })));
    assert!(!sim.player(0).unwrap().idle.breath.holding);
}

#[test]
fn shooting_kills_an_enemy_and_reports_hitmarkers() {
    let host = FakeHost::default();
    let mut g = general(1, 0);
    g.pos = Vec3::new(0.0, 0.0, 0.0);
    host.add(g);
    host.add(soldier(2, 1, 60.0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    // A General spawns unarmed; this test is about a gun, so pick the first preset.
    sim.choose_class(0, 1, 0, &[]);
    wait_until_ready(&mut sim, &host);
    // Let the weapon raise, then aim (ADS for accuracy) and fire. Eye is at 10 units; soldier
    // body centre is at 6, so aim slightly down.
    let pitch = -(4.0f32 / 60.0).atan();
    let aim = FpsInput { held: buttons::ADS, pitch, ..Default::default() };
    run(&mut sim, &host, 0, 1, aim, 20);
    let fire = FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() };
    run(&mut sim, &host, 0, 1, fire, 20);
    assert!(host.get(2).has(objflags::DEAD), "enemy killed; damage {:?}", host.damage_log.borrow());
    let ev = events(&sim);
    assert!(ev.iter().any(|e| matches!(e, EventKind::HitMarker { kill: true, .. })));
    assert!(host.fx.borrow().iter().all(|(s, w, _)| *s == 1 && w == "ZHBO_MTAR_FX"));
}

/// Scavenger tops the reserve up to BO2's stock cap: `iMaxAmmo` spare rounds beside the
/// magazine (`FUN_005e08d0`; a rifle's 8 magazines of 30 = 240), not a magazine fewer. The
/// magazine is untouched; the HUD's reserve is that number.
#[test]
fn a_scavenger_kill_tops_the_reserve_up_to_the_stock_cap() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.add(soldier(2, 1, 60.0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    wait_until_ready(&mut sim, &host);
    let p = sim.players[0].as_mut().unwrap();
    p.arsenal.loadout.perks.push(crate::weapons::Perk::SCAVENGER);
    let w = p.arsenal.current_mut();
    assert_eq!((w.def.clip, w.def.max_reserve), (30, 240), "{}: BO2's 30-round magazine, 8 of them spare", w.def.id);
    w.reserve = 230;
    let pitch = -(4.0f32 / 60.0).atan();
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS, pitch, ..Default::default() }, 20);
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() }, 20);
    assert!(host.get(2).has(objflags::DEAD), "enemy killed");
    let a = &sim.player(0).unwrap().arsenal;
    assert_eq!(a.current().reserve, 240, "topped up to the stock cap, not past it");
    assert!(a.current().ammo > 0 && a.current().ammo < 30, "the magazine is the one being fired");
}

/// BO2 traces a non-spread round `bulletrange` (8192 in, `Bullet_Fire`), whatever the def's
/// `maxRange` (a turret field, 0 on every MP gun): an enemy at 8,100 in is hit (the mod's old
/// 8,000 guess missed it), one at 8,300 in is not.
#[test]
fn a_round_flies_bo2s_bulletrange() {
    for (inch, hit) in [(8100.0f32, true), (8300.0, false)] {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let x = crate::units::inches(inch);
        host.add(soldier(2, 1, x));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        wait_until_ready(&mut sim, &host);
        let pitch = -(4.0f32 / x).atan();
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS, pitch, ..Default::default() }, 20);
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() }, 3);
        let damaged = host.damage_log.borrow().iter().any(|(_, v, _)| *v == 2);
        assert_eq!(damaged, hit, "enemy at {inch} in");
    }
}

#[test]
fn others_see_brass_every_shot_and_a_tracer_every_nth_bullet() {
    // BO2's third-person shot (`FUN_00531630`, `FUN_007c65c0`): the world brass once per shot
    // (the last-round twin on the round that empties the magazine) and a tracer on the 3rd,
    // 6th, … bullet of an interval-3 gun. Deterministic: the countdown lives in the sim.
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    wait_until_ready(&mut sim, &host);
    let w = sim.players[0].as_mut().unwrap().arsenal.current_mut();
    let mut def = w.def.clone();
    def.world_shell_fx_weapon = "BRASS".into();
    def.world_last_shot_shell_fx_weapon = "BRASS_LAST".into();
    def.tracer_fx_weapon = "TRACER".into();
    def.tracer_interval = 3;
    def.pellets = 1;
    def.fire = crate::weapons::FireMode::Auto;
    let def: &'static crate::weapons::WeaponDef = Box::leak(Box::new(def));
    (w.def, w.main, w.ammo, w.reserve) = (def, def, 7, 0);
    host.fx.borrow_mut().clear();
    // The tracer bullets are queued for the host layer (the viewer picks the asset): note how
    // many shots' brass had played when each was queued.
    let mut after = Vec::new();
    for _ in 0..frames(5.0) {
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::FIRE, ..Default::default() }, 1);
        let shots = host.fx.borrow().iter().filter(|(_, w, _)| w.starts_with("BRASS")).count();
        for t in std::mem::take(&mut sim.tracers) {
            assert_eq!((t.shooter, t.def.tracer_fx_weapon.as_str()), (1, "TRACER"));
            after.push(shots);
        }
    }
    let fx: Vec<String> = host.fx.borrow().iter().map(|(_, w, _)| w.clone()).collect();
    let brass: Vec<&str> = fx.iter().map(String::as_str).filter(|w| w.starts_with("BRASS")).collect();
    assert_eq!(brass, ["BRASS", "BRASS", "BRASS", "BRASS", "BRASS", "BRASS", "BRASS_LAST"], "{fx:?}");
    assert!(!fx.iter().any(|w| w == "TRACER"), "the sim fires no tracer itself: {fx:?}");
    // The tracer follows its shot's brass: shots 3 and 6 of 7.
    assert_eq!(after, [3, 6], "{fx:?}");
}

#[test]
fn the_tracer_countdown_is_bo2s() {
    let draws = |interval: u32, bullets: usize, start: u8| {
        let mut c = start;
        (1..=bullets).filter(|_| fire::tracer_due(&mut c, interval)).collect::<Vec<_>>()
    };
    assert_eq!(draws(1, 4, 0), [1, 2, 3, 4], "every bullet");
    assert_eq!(draws(2, 6, 0), [2, 4, 6]);
    assert_eq!(draws(3, 7, 0), [3, 6]);
    assert!(draws(0, 50, 0).is_empty(), "no interval, no tracer");
    // A countdown left above a new gun's interval is clamped to it (switching from an AR to an LMG).
    assert_eq!(draws(1, 2, 2), [1, 2]);
    // One below the interval carries over: the next draw comes sooner.
    assert_eq!(draws(3, 3, 1), [1]);
}

#[test]
fn a_landed_melee_reports_its_hit_and_a_miss_does_not() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    wait_until_ready(&mut sim, &host);
    sim.events.clear();
    // The strike lands after the melee weapon's delay (`iMeleeDelay`, assault-shield's melee
    // states): run the swing out.
    run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::MELEE, ..Default::default() }, 1);
    run(&mut sim, &host, 0, 1, FpsInput::default(), 20);
    let ev = events(&sim);
    assert!(ev.contains(&EventKind::Melee) && !ev.contains(&EventKind::MeleeHit), "a swing at the air: {ev:?}");
    run(&mut sim, &host, 0, 1, FpsInput::default(), 60);
    // An enemy at arm's length straight ahead (yaw 0 faces +x).
    host.add(soldier(2, 1, 8.0));
    sim.events.clear();
    run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::MELEE, ..Default::default() }, 1);
    run(&mut sim, &host, 0, 1, FpsInput::default(), 20);
    let ev = events(&sim);
    assert!(ev.contains(&EventKind::Melee) && ev.contains(&EventKind::MeleeHit), "{ev:?}");
}

#[test]
fn a_pellet_reports_the_surface_it_hit() {
    // An enemy structure straight ahead: the pellets land on concrete, not on the ground.
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.add(ObjInfo {
        id: 2,
        owner_player: 1,
        flags: objflags::STRUCTURE,
        geom_type: geom::BOX,
        major_radius: 20.0,
        minor_radius: 20.0,
        height: 40.0,
        health: 5000.0,
        max_health: 5000.0,
        pos: Vec3::new(60.0, 0.0, 0.0),
        ..Default::default()
    });
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    wait_until_ready(&mut sim, &host);
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS, ..Default::default() }, 20);
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS | buttons::FIRE, ..Default::default() }, 20);
    let impacts: Vec<_> = events(&sim)
        .into_iter()
        .filter_map(|k| match k {
            EventKind::Impact { surface, at, .. } => {
                assert!((at.x - 40.0).abs() < 1.0, "the impact is on the wall's near face, got {at:?}");
                Some(surface)
            }
            _ => None,
        })
        .collect();
    assert!(!impacts.is_empty(), "the wall took pellets");
    assert!(impacts.iter().all(|s| *s == Surface::Concrete), "{impacts:?}");
}

#[test]
fn a_shot_that_hits_nothing_raises_no_impact() {
    // Up into the sky, nothing in front: the bullet flies to its range and stops in the air.
    // (Level fire is not enough: the hip spread cone reaches the ground at long range, which
    // is a real ground hit and rightly raises an impact.)
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    wait_until_ready(&mut sim, &host);
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::FIRE, pitch: 0.3, ..Default::default() }, 10);
    assert!(
        !events(&sim).iter().any(|k| matches!(k, EventKind::Impact { .. })),
        "no hit, no impact (and so no impact sound)"
    );
}

#[test]
fn a_missed_shot_into_the_ground_asks_the_host_for_its_surface() {
    // Firing down: the pellets stop on the terrain, whose surface is the host's answer.
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    wait_until_ready(&mut sim, &host);
    let down = FpsInput { held: buttons::FIRE, pitch: -0.6, ..Default::default() };
    run(&mut sim, &host, 0, 1, down, 10);
    let impacts: Vec<_> = events(&sim)
        .into_iter()
        .filter_map(|k| match k {
            EventKind::Impact { surface, .. } => Some(surface),
            _ => None,
        })
        .collect();
    assert!(!impacts.is_empty(), "the ground took pellets");
    assert!(impacts.iter().all(|s| *s == Surface::Dirt), "{impacts:?}");
}

/// A pellet's sound is BO2's `prj_bullet_impact_<size>_<surface>` for the gun's type: the sim
/// reports the ground's surface, the client's cue follows it (`sounds::impact_cue`).
#[test]
fn a_shot_into_two_grounds_plays_each_surfaces_hit_sound() {
    crate::impacts::install(crate::impacts::TEST_TABLE);
    let fire_into = |ground: Surface| {
        let host = FakeHost::default();
        host.ground.set(Some(ground));
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        sim.choose_class(0, 1, 0, &[]);
        wait_until_ready(&mut sim, &host);
        run(&mut sim, &host, 0, 1, FpsInput { held: buttons::FIRE, pitch: -0.6, ..Default::default() }, 10);
        let surfaces: Vec<Surface> = events(&sim)
            .into_iter()
            .filter_map(|k| match k {
                EventKind::Impact { surface, .. } => Some(surface),
                _ => None,
            })
            .collect();
        assert!(!surfaces.is_empty(), "the ground took pellets");
        surfaces
    };
    let (concrete, dirt, water) = (fire_into(Surface::Concrete), fire_into(Surface::Dirt), fire_into(Surface::Water));
    assert!(concrete.iter().all(|s| *s == Surface::Concrete) && dirt.iter().all(|s| *s == Surface::Dirt));
    // The cue for a bullet type with converted sounds (78 in the test table).
    let cue = |s: &[Surface]| crate::sounds::impact_cue(78, s[0]);
    assert_eq!((cue(&concrete), cue(&dirt)), (Some("HIT_CONCRETE"), Some("HIT_DIRT")));
    assert_eq!(cue(&water), None, "no alias for the surface: silent, as BO2's hash miss");
    // A type with no converted sounds keeps the fixed cues (an unconverted archive).
    assert_eq!(crate::sounds::impact_cue(79, Surface::Concrete), Some(crate::sounds::IMPACT));
    assert_eq!(crate::sounds::impact_cue(79, Surface::Flesh), Some(crate::sounds::IMPACT_FLESH));
}

/// A wall `thick` inches deep (enemy concrete) 40 units out and an enemy soldier 60 out behind
/// it; the General fires one aimed round of a flat-damage gun of `pen`, with FMJ's
/// `bulletpenetration` when `fmj`. Returns (wall damage, soldier damage, the round's events).
fn shoot_through_wall(pen: crate::penetration::Penetrate, specs: &[Specialty], thick: f32) -> (f32, f32, Vec<EventKind>) {
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.add(ObjInfo {
        id: 2,
        owner_player: 1,
        flags: objflags::STRUCTURE,
        geom_type: geom::BOX,
        major_radius: crate::units::inches(thick) * 0.5,
        minor_radius: 30.0,
        height: 40.0,
        health: 5000.0,
        max_health: 5000.0,
        pos: Vec3::new(40.0, 0.0, 0.0),
        ..Default::default()
    });
    host.add(ObjInfo { health: 5000.0, max_health: 5000.0, ..soldier(3, 1, 60.0) });
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    wait_until_ready(&mut sim, &host);
    let w = sim.players[0].as_mut().unwrap().arsenal.current_mut();
    let mut def = w.def.clone();
    def.traits.penetrate = pen;
    def.damage = [40.0; 6];
    def.damage_range = [0.0; 6];
    def.pellets = 1;
    def.specialties = crate::weapons::Specialties::of(specs);
    let def: &'static crate::weapons::WeaponDef = Box::leak(Box::new(def));
    (w.def, w.main) = (def, def);
    let pitch = -(4.0f32 / 60.0).atan();
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS, pitch, ..Default::default() }, 20);
    sim.events.clear();
    host.damage_log.borrow_mut().clear();
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() }, 1);
    let log = host.damage_log.borrow().clone();
    let to = |id: u32| log.iter().filter(|d| d.1 == id).map(|d| d.2).sum::<f32>();
    assert_eq!(log.iter().filter(|d| d.1 == 2).count(), 1, "one round on the wall: {log:?}");
    (to(2), to(3), events(&sim))
}

/// BO2's penetration (`FUN_005d0350`): 4.5 in of concrete costs a medium round half its damage
/// (9 in deep), and the soldier behind takes the rest; the wall's far face plays the exit row.
#[test]
fn a_round_through_a_thin_wall_hits_the_soldier_behind_for_less() {
    use crate::penetration::Penetrate;
    let (wall, behind, ev) = shoot_through_wall(Penetrate::Medium, &[], 4.5);
    assert!(wall > 0.0 && (behind / wall - 0.5).abs() < 0.01, "wall {wall}, behind {behind}");
    let exits: Vec<_> = ev.iter().filter_map(|k| match *k {
        EventKind::Impact { at, surface, exit: true, .. } => Some((at, surface)),
        _ => None,
    }).collect();
    // The soldier's 32 in through the middle spends the other half (64 in deep): no exit there.
    assert_eq!(exits.len(), 1, "the wall's far face: {ev:?}");
    assert_eq!(exits[0].1, Surface::Concrete);
    assert!((exits[0].0.x - (40.0 + crate::units::inches(2.25))).abs() < 0.05, "{exits:?}");
    let entries = ev.iter().filter(|k| matches!(k, EventKind::Impact { exit: false, .. })).count();
    assert_eq!(entries, 2, "the wall and the soldier: {ev:?}");
    // No penetration type: the wall takes it all.
    let (_, behind, ev) = shoot_through_wall(Penetrate::None, &[], 4.5);
    assert_eq!(behind, 0.0);
    assert!(!ev.iter().any(|k| matches!(k, EventKind::Impact { exit: true, .. })));
}

/// FMJ grants `bulletpenetration`: `perk_bulletPenetrationMultiplier` (2) doubles every depth,
/// so 7 in of concrete stops a small round (6 in) but not a small FMJ round (12 in).
#[test]
fn fmj_carries_a_round_through_a_wall_that_stops_it_without() {
    use crate::penetration::Penetrate;
    let (_, behind, _) = shoot_through_wall(Penetrate::Small, &[], 7.0);
    assert_eq!(behind, 0.0, "7 in is past a small round's 6 in");
    let (wall, behind, _) = shoot_through_wall(Penetrate::Small, &[Specialty::BulletPenetration], 7.0);
    assert!((behind / wall - (1.0 - 7.0 / 12.0)).abs() < 0.01, "wall {wall}, behind {behind}");
}

/// FMJ's other specialty, `armorpiercing`: a structure (BO2's scorestreak) takes
/// `perk_armorpiercing` (40) percent more of the round; a soldier would take none extra.
#[test]
fn armorpiercing_hits_a_structure_harder() {
    use crate::penetration::Penetrate;
    let (plain, ..) = shoot_through_wall(Penetrate::None, &[], 4.5);
    let (pierced, ..) = shoot_through_wall(Penetrate::None, &[Specialty::ArmorPiercing], 4.5);
    assert!((pierced / plain - 1.4).abs() < 1e-3, "plain {plain}, armour-piercing {pierced}");
}

#[test]
fn a_kill_reports_a_fatal_flesh_impact() {
    // The template test's shot, asserting the record's blood bits: body (not head), fatal.
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.add(soldier(2, 1, 60.0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    wait_until_ready(&mut sim, &host);
    let pitch = -(4.0f32 / 60.0).atan();
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS, pitch, ..Default::default() }, 20);
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() }, 20);
    assert!(host.get(2).has(objflags::DEAD), "the soldier was killed");
    let flesh: Vec<_> = events(&sim)
        .into_iter()
        .filter_map(|k| match k {
            EventKind::Impact { surface, head, fatal, .. } => (surface == Surface::Flesh).then_some((head, fatal)),
            _ => None,
        })
        .collect();
    assert!(!flesh.is_empty(), "blood on the soldier");
    assert!(flesh.iter().all(|(head, _)| !head), "a body shot: {flesh:?}");
    assert!(flesh.iter().any(|(_, fatal)| *fatal), "one of them killed: {flesh:?}");
}

#[test]
fn bullets_pass_through_friendlies() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.add(soldier(3, 0, 30.0));
    host.add(soldier(2, 1, 60.0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    wait_until_ready(&mut sim, &host);
    let pitch = -(4.0f32 / 60.0).atan();
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS, pitch, ..Default::default() }, 20);
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::ADS | buttons::FIRE, pitch, ..Default::default() }, 10);
    assert_eq!(host.get(3).health, 120.0, "friendly untouched");
    assert!(host.get(2).health < 120.0, "enemy behind was hit");
}

fn option(template: u32, status: i32) -> crate::building::BuildOption {
    let mut o = crate::building::BuildOption { template, cost: 500, status, ..Default::default() };
    o.name[..4].copy_from_slice(b"Shed");
    o
}

#[test]
fn the_hammer_places_a_structure_and_holding_use_builds_it() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.options.borrow_mut().extend([option(7, crate::building::status::OK), option(8, crate::building::status::OK)]);
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    wait_until_ready(&mut sim, &host);
    let ammo = sim.player(0).unwrap().arsenal.current().ammo;
    let look = |held: u32, pressed: u32| FpsInput { held, pressed, pitch: -0.5, ..Default::default() };
    run(&mut sim, &host, 0, 1, look(buttons::HAMMER, buttons::HAMMER), 1);
    assert!(sim.player(0).unwrap().hammer.is_some());
    // ADS picks the next structure, and does not aim the gun.
    run(&mut sim, &host, 0, 1, look(buttons::ADS, buttons::ADS), 1);
    run(&mut sim, &host, 0, 1, look(0, 0), frames(crate::building::HAMMER_RAISE));
    assert_eq!(sim.player(0).unwrap().arsenal.ads, 0.0);
    sim.events.clear();
    run(&mut sim, &host, 0, 1, look(buttons::FIRE, buttons::FIRE), 1);
    assert!(events(&sim).contains(&EventKind::BuildPlaced { template: 8 }), "{:?}", events(&sim));
    assert_eq!(sim.player(0).unwrap().arsenal.current().ammo, ammo, "the gun did not fire");
    let at = host.sites.borrow()[0].at;
    assert!(at.x >= crate::building::PLACE_MIN - 0.01 && at.x < crate::building::PLACE_RANGE && at.z == 0.0, "placed ahead on the ground: {at:?}");
    // One structure per hammer: it went away.
    assert!(sim.player(0).unwrap().hammer.is_none());
    assert!(events(&sim).contains(&EventKind::Hammer { up: false }));
    // Up again, on the same spot: no room.
    run(&mut sim, &host, 0, 1, look(buttons::HAMMER, buttons::HAMMER), 1);
    run(&mut sim, &host, 0, 1, look(0, 0), frames(crate::building::HAMMER_RAISE));
    sim.events.clear();
    run(&mut sim, &host, 0, 1, look(buttons::FIRE, buttons::FIRE), 1);
    assert!(events(&sim).contains(&EventKind::BuildRefused { status: crate::building::status::BLOCKED }));
    assert!(sim.player(0).unwrap().hammer.is_some(), "refused: still in hand");
    run(&mut sim, &host, 0, 1, look(buttons::HAMMER, buttons::HAMMER), 1);

    // Up to the site: the use key builds it (not the field computer), only while held.
    host.sites.borrow_mut()[0].at = Vec3::new(25.0, 0.0, 0.0);
    run(&mut sim, &host, 0, 1, look(buttons::FIELD_COMPUTER, buttons::FIELD_COMPUTER), 3);
    assert_eq!(sim.player(0).unwrap().mode, Mode::Fps);
    assert!(sim.player(0).unwrap().building);
    run(&mut sim, &host, 0, 1, look(0, 0), 2);
    let paused = host.sites.borrow()[0].percent;
    assert!((paused - 30.0).abs() < 0.01, "three frames of building: {paused}");
    sim.events.clear();
    run(&mut sim, &host, 0, 1, look(buttons::FIELD_COMPUTER, buttons::FIELD_COMPUTER), 7);
    assert!(events(&sim).contains(&EventKind::WorkDone { kind: crate::building::work::BUILD }), "{:?}", events(&sim));
    assert!(!sim.player(0).unwrap().building);
    // Built: the use key is the field computer's again.
    run(&mut sim, &host, 0, 1, look(0, 0), 1);
    run(&mut sim, &host, 0, 1, look(buttons::FIELD_COMPUTER, buttons::FIELD_COMPUTER), 1);
    assert!(matches!(sim.player(0).unwrap().mode, Mode::FieldComputer { .. }));
}

#[test]
fn the_hammer_refuses_what_he_cannot_afford_and_puts_away_for_a_weapon() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.options.borrow_mut().push(option(7, crate::building::status::NO_MONEY));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    wait_until_ready(&mut sim, &host);
    run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::HAMMER, ..Default::default() }, 1);
    run(&mut sim, &host, 0, 1, FpsInput::default(), frames(crate::building::HAMMER_RAISE));
    sim.events.clear();
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::FIRE, pressed: buttons::FIRE, ..Default::default() }, 1);
    assert!(events(&sim).contains(&EventKind::BuildRefused { status: crate::building::status::NO_MONEY }));
    assert!(host.sites.borrow().is_empty());
    run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::SWAP_WEAPON, ..Default::default() }, 1);
    assert!(sim.player(0).unwrap().hammer.is_none());
}

#[test]
fn the_wheel_turns_the_structure_being_placed() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.options.borrow_mut().push(option(7, crate::building::status::OK));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    wait_until_ready(&mut sim, &host);
    run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::HAMMER, ..Default::default() }, 1);
    run(&mut sim, &host, 0, 1, FpsInput::default(), frames(crate::building::HAMMER_RAISE));
    let facing = sim.player(0).unwrap().hammer.unwrap().angle;
    run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::ROTATE_LEFT, ..Default::default() }, 1);
    run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::ROTATE_LEFT, ..Default::default() }, 1);
    run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::ROTATE_RIGHT, ..Default::default() }, 1);
    let turned = sim.player(0).unwrap().hammer.unwrap().angle;
    assert!((crate::math::wrap_angle(turned - facing) - crate::building::ROTATE_STEP).abs() < 1e-4, "{facing} -> {turned}");
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::FIRE, pressed: buttons::FIRE, ..Default::default() }, 1);
    assert!((host.sites.borrow()[0].angle - turned).abs() < 1e-4, "placed as turned");
}

#[test]
fn holding_use_at_a_damaged_structure_repairs_it() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let repair = crate::building::work::REPAIR;
    host.sites.borrow_mut().push(FakeSite { id: 5, template: 3, at: Vec3::new(30.0, 0.0, 0.0), percent: 60.0, kind: repair, ..Default::default() });
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    wait_until_ready(&mut sim, &host);
    let hold = FpsInput { held: buttons::FIELD_COMPUTER, pressed: buttons::FIELD_COMPUTER, ..Default::default() };
    run(&mut sim, &host, 0, 1, hold, 1);
    assert!(events(&sim).contains(&EventKind::Working { kind: repair, percent: 70.0 }), "{:?}", events(&sim));
    assert_eq!(sim.player(0).unwrap().mode, Mode::Fps, "the use key repaired, not the field computer");
    run(&mut sim, &host, 0, 1, hold, 3);
    assert!(events(&sim).contains(&EventKind::WorkDone { kind: repair }), "{:?}", events(&sim));

    // Someone else repairing it: refused once, when the key goes down.
    host.sites.borrow_mut().push(FakeSite { id: 6, template: 3, at: Vec3::new(30.0, 0.0, 0.0), percent: 50.0, kind: repair, busy: true, ..Default::default() });
    sim.events.clear();
    run(&mut sim, &host, 0, 1, hold, 1);
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::FIELD_COMPUTER, ..Default::default() }, 3);
    let refused = events(&sim).iter().filter(|e| **e == EventKind::BuildRefused { status: crate::building::status::BUSY }).count();
    assert_eq!(refused, 1, "{:?}", events(&sim));
    assert_eq!(host.sites.borrow()[1].percent, 50.0);
}

#[test]
fn field_computer_returns_control_to_rts_and_back() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::FIELD_COMPUTER, ..Default::default() }, 1);
    assert!(matches!(sim.player(0).unwrap().mode, Mode::FieldComputer { .. }));
    assert_eq!(*host.controlled.borrow(), vec![1], "still controlled while the laptop comes up");
    run(&mut sim, &host, 0, 1, FpsInput::default(), frames(FIELD_COMPUTER_RAISE));
    assert_eq!(sim.player(0).unwrap().mode, Mode::Rts);
    assert!(host.controlled.borrow().is_empty(), "AI resumed");
    assert!(events(&sim).contains(&EventKind::ReturnedToRts { general: 1 }));
    sim.take_control(&host, 0, 1);
    assert_eq!(sim.player(0).unwrap().mode, Mode::Fps);
}

#[test]
fn health_regenerates_after_delay_and_death_releases() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
    host.objects.borrow_mut()[0].health = 150.0;
    run(&mut sim, &host, 0, 1, FpsInput::default(), 30 * 4);
    assert!(host.get(1).health < 151.0, "no regen during the delay");
    run(&mut sim, &host, 0, 1, FpsInput::default(), 30 * 5);
    assert_eq!(host.get(1).health, 300.0, "regenerated to full");
    host.objects.borrow_mut()[0].flags |= objflags::DEAD;
    run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
    assert!(sim.player(0).is_none());
    assert!(events(&sim).contains(&EventKind::GeneralDied { general: 1 }));
}

#[test]
fn damage_flinches_the_view_and_toughness_cuts_it() {
    // G7: flinch = BO2 damage x bg_viewKickScale (0.175), x0.25 with Toughness.
    let flinch_for = |toughness: bool| {
        let host = FakeHost::default();
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
        let p = sim.players[0].as_mut().unwrap();
        p.arsenal.loadout.perks.retain(|k| *k != crate::weapons::Perk::TOUGHNESS);
        if toughness {
            p.arsenal.loadout.perks.push(crate::weapons::Perk::TOUGHNESS);
        }
        // 60 Zero Hour points are 40 on BO2's scale.
        host.objects.borrow_mut()[0].health -= 40.0 * crate::units::BO2_TO_ZH_DAMAGE;
        run(&mut sim, &host, 0, 1, FpsInput::default(), 1);
        events(&sim).into_iter().find_map(|k| if let EventKind::Flinch { pitch_deg } = k { Some(pitch_deg) } else { None }).expect("a flinch")
    };
    assert!((flinch_for(false) - 7.0).abs() < 1e-3);
    assert!((flinch_for(true) - 1.75).abs() < 1e-3);
}

#[test]
fn rts_shots_use_the_generals_ammo_and_carry_into_first_person() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    // The General under RTS gets a record from the bridge's per-frame update; spawn unarmed,
    // so give him a class first (the test is about the class kit's ammo).
    sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None);
    sim.choose_class(0, 1, 0, &[]);
    let full = sim.rts_ammo(1);
    // Zero Hour fires the rifle: one round a shot, the clip refilled from the spare rounds.
    let r = sim.rts_update(&host, 0, 1, 0, [1, 0, 0], None);
    assert_eq!((r.ammo[0], r.reserve[0]), (full.ammo[0] - 1, full.reserve[0]));
    assert_eq!(r.lock, -1);
    // Empty the rifle completely: the General is moved to the pistol.
    let rounds = (full.ammo[0] + full.reserve[0]) as u32;
    let r = sim.rts_update(&host, 0, 1, 0, [rounds, 0, 0], None);
    assert_eq!(r.ammo[0] + r.reserve[0], 0);
    assert_eq!(r.lock, 1, "to the pistol");
    // The player picks the pistol; a thrown grenade is one fewer, and he goes back to the pistol.
    let r = sim.rts_update(&host, 0, 1, 1, [0, 1, 1], Some(Vec3::new(200.0, 0.0, 0.0)));
    assert_eq!(r.ammo[2], full.ammo[2] - 1);
    assert_eq!(r.lock, 1, "back to the gun after the throw");
    assert_eq!(sim.projectiles.len(), 1, "a BO2 grenade, thrown as in first person");
    // Taking control keeps the ammo and the weapon in hand.
    sim.take_control(&host, 0, 1);
    let p = sim.player(0).unwrap();
    assert_eq!(p.arsenal.active, 1);
    assert_eq!(p.arsenal.weapons[1].ammo as i32, r.ammo[1]);
    assert_eq!(p.arsenal.weapons[0].ammo + p.arsenal.weapons[0].reserve, 0);
    // While in first person nothing is counted.
    let before = sim.rts_ammo(1);
    sim.rts_update(&host, 0, 1, 0, [5, 5, 1], None);
    assert_eq!(sim.rts_ammo(1), before);
}

#[test]
fn rts_throws_land_near_the_target_and_can_be_tacticals() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None);
    sim.choose_class(0, 1, 0, &[]);
    let full = sim.rts_ammo(1);
    sim.rts_offhand(0, 1, Third::Tactical.wire());
    let r = sim.rts_update(&host, 0, 1, 2, [0, 0, 1], Some(Vec3::new(150.0, 40.0, 0.0)));
    assert_eq!((r.tactical, r.ammo[2], r.tactical_selected), (full.tactical - 1, full.ammo[2], 1), "the tactical, not the lethal");
    let (_, proj) = &sim.projectiles[0];
    assert_eq!(proj.def.class, crate::weapons::Class::Tactical);
    // The arc comes down by the target.
    let mut p = proj.pos;
    let mut v = proj.vel;
    while v.z > 0.0 || p.z > 0.0 {
        v.z -= crate::projectiles::gravity() * LOGIC_DT;
        p += v * LOGIC_DT;
    }
    assert!((Vec3::new(p.x, p.y, 0.0) - Vec3::new(150.0, 40.0, 0.0)).length() < 8.0, "lands at {p:?}");
}

#[test]
fn returning_to_rts_locks_the_weapon_in_hand() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    // The pistol is slot 1 of a class kit; the unarmed kit has only the knife.
    sim.choose_class(0, 1, 0, &[]);
    sim.players[0].as_mut().unwrap().arsenal.active = 1;
    run(&mut sim, &host, 0, 1, FpsInput { pressed: buttons::FIELD_COMPUTER, ..Default::default() }, 1);
    run(&mut sim, &host, 0, 1, FpsInput::default(), frames(FIELD_COMPUTER_RAISE));
    assert_eq!(sim.player(0).unwrap().mode, Mode::Rts);
    assert_eq!(sim.rts_update(&host, 0, 1, 0, [0, 0, 0], None).lock, 1, "the pistol, which was in hand");
    assert_eq!(sim.rts_update(&host, 0, 1, 1, [0, 0, 0], None).lock, -1, "once");
}

/// BO2's blast is the impact table's for the def's `impactType` and the surface it went off on
/// (`FUN_004c0ac0` -> `FUN_00533210`): the sim reports the surface in the event, the effect and
/// the client's cues follow it.
#[test]
fn a_blast_on_two_surfaces_plays_their_own_effect_and_sound() {
    crate::impacts::install(crate::impacts::TEST_TABLE);
    let frag: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef {
        id: "test_frag_mp".into(),
        effect: crate::weapons::Effect::Explode,
        impact_type: 77,
        explode_sound: "OWN".into(),
        fuse_time: 0.1,
        ..Default::default()
    }));
    let blast_on = |ground: Surface| {
        let host = FakeHost::default();
        host.ground.set(Some(ground));
        host.add(general(1, 0));
        let mut sim = Sim::new();
        sim.projectiles.push((0, Projectile::launch(frag, 1, Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0), 0.1, &host, 0.0)));
        run(&mut sim, &host, 0, 1, FpsInput::default(), frames(0.5));
        let ev = events(&sim)
            .into_iter()
            .find_map(|k| match k {
                EventKind::Exploded { sound, impact_type, surface, .. } => Some((sound, impact_type, surface)),
                _ => None,
            })
            .expect("it went off");
        let fx = host.played_fx.borrow().clone();
        (ev, fx)
    };
    let ((sound, t, on_concrete), fx_concrete) = blast_on(Surface::Concrete);
    assert_eq!((sound, t, on_concrete), ("OWN", 77, Surface::Concrete));
    let ((_, _, on_dirt), fx_dirt) = blast_on(Surface::Dirt);
    assert_eq!(on_dirt, Surface::Dirt);
    // The table's cell, then the def's own: dirt has no cell of its own and takes the default.
    assert_eq!(fx_concrete, vec!["FX_C".to_string()]);
    assert_eq!(fx_dirt, vec!["FX_D".to_string()]);
    assert_eq!(crate::sounds::explosion_cues(sound, t, on_concrete), [Some("SND_C"), Some("OWN")]);
    assert_eq!(crate::sounds::explosion_cues(sound, t, on_dirt), [Some("SND_D"), Some("OWN")]);
    // A surface the family has no alias for plays no table sound (BO2's hash miss is silent),
    // and not the generic blast either: the type's sounds are converted.
    assert_eq!(crate::sounds::explosion_cues(sound, t, Surface::Water), [None, Some("OWN")]);
    assert_eq!(crate::sounds::explosion_cues("", t, Surface::Water), [None, None]);
}

/// BO2's event 0x47 (`G_RunMissile` at each bounce): the def's `bounceSound[surface]` and the
/// `grenade_bounce` row's cell for the surface, exact (none on dirt here, and no fallback).
#[test]
fn a_bounce_plays_the_defs_sound_and_the_bounce_cell_for_the_surface() {
    crate::impacts::install(crate::impacts::TEST_TABLE);
    let frag: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef {
        id: "test_bouncer_mp".into(),
        effect: crate::weapons::Effect::Explode,
        impact_type: 77,
        fuse_time: 5.0,
        projectile_gravity: true,
        bounce_sounds: vec![("concrete".into(), "BOUNCE_C".into()), ("water".into(), crate::sounds::NO_SOUND.into())],
        ..Default::default()
    }));
    let bounce_on = |ground: Surface| {
        let host = FakeHost::default();
        host.ground.set(Some(ground));
        host.add(general(1, 0));
        let mut sim = Sim::new();
        let mut p = Projectile::launch(frag, 1, Vec3::new(0.0, 0.0, 2.0), Vec3::new(1.0, 0.0, 0.0), 5.0, &host, 0.0);
        p.vel = Vec3::new(4.0, 0.0, -10.0);
        sim.projectiles.push((0, p));
        run(&mut sim, &host, 0, 1, FpsInput::default(), frames(1.0));
        let sounds: Vec<&str> = events(&sim)
            .into_iter()
            .filter_map(|k| match k {
                EventKind::Bounced { sound, .. } => Some(sound),
                _ => None,
            })
            .collect();
        let fx = host.played_fx.borrow().clone();
        (sounds, fx)
    };
    let (concrete, fx_concrete) = bounce_on(Surface::Concrete);
    assert!(!concrete.is_empty() && concrete.iter().all(|s| *s == "BOUNCE_C"), "{concrete:?}");
    assert_eq!(fx_concrete.len(), concrete.len());
    assert!(fx_concrete.iter().all(|f| f == "FX_BOUNCE_C"), "{fx_concrete:?}");
    let (dirt, fx_dirt) = bounce_on(Surface::Dirt);
    assert!(!dirt.is_empty() && dirt.iter().all(|s| s.is_empty()), "no cell for dirt: silent {dirt:?}");
    assert!(fx_dirt.is_empty(), "the bounce row's default cell is not a fallback: {fx_dirt:?}");
    assert_eq!(crate::sounds::some(concrete[0]), Some("BOUNCE_C"));
    assert_eq!(crate::sounds::some(crate::sounds::NO_SOUND), None);
}

/// BO2's dud (`FUN_00604280`: struck before `iProjectileActivateDist` of path): no blast, the
/// direct hit's impact damage only, and event 0x51's effects and sounds for the surface.
#[test]
fn a_launcher_grenade_inside_its_arming_distance_is_a_dud() {
    crate::impacts::install(crate::impacts::TEST_TABLE);
    let gl = |activate_dist: f32| -> &'static crate::weapons::WeaponDef {
        let mut damage = [0.0; 6];
        damage[0] = 75.0;
        Box::leak(Box::new(crate::weapons::WeaponDef {
            id: "test_gl_mp".into(),
            effect: crate::weapons::Effect::Explode,
            impact_type: 77,
            impact_explode: true,
            projectile_speed: 20.0,
            explosion_radius: 10.0,
            explosion_inner_damage: 200.0,
            explosion_outer_damage: 50.0,
            damage,
            activate_dist,
            dud_fx: "OWN_DUD_FX".into(),
            dud_sound: "OWN_DUD".into(),
            ..Default::default()
        }))
    };
    let shoot = |def: &'static crate::weapons::WeaponDef, target: bool| {
        let host = FakeHost::default();
        host.ground.set(Some(Surface::Concrete));
        host.add(general(1, 0));
        if target {
            host.add(soldier(5, 1, 3.0));
        }
        let mut sim = Sim::new();
        let dir = if target { Vec3::new(1.0, 0.0, 0.0) } else { Vec3::new(1.0, 0.0, -1.0).normalized_or_zero() };
        sim.projectiles.push((0, Projectile::launch(def, 1, Vec3::new(0.0, 0.0, 1.0), dir, f32::INFINITY, &host, 0.0)));
        run(&mut sim, &host, 0, 1, FpsInput::default(), frames(1.0));
        let damage = host.damage_log.borrow().clone();
        let fx = host.played_fx.borrow().clone();
        (events(&sim), damage, fx)
    };
    // Into the ground a step away, well inside 100 units of path: a dud on concrete.
    let (ev, damage, fx) = shoot(gl(100.0), false);
    assert!(!ev.iter().any(|k| matches!(k, EventKind::Exploded { .. })), "a dud does not go off");
    let dud = ev.iter().find_map(|k| match *k {
        EventKind::Dud { sound, surface, .. } => Some((sound, surface)),
        _ => None,
    });
    assert_eq!(dud, Some(("OWN_DUD", Surface::Concrete)));
    assert!(damage.is_empty(), "nothing hit directly, no blast");
    assert_eq!(fx, vec!["FX_DUD_C".to_string(), "OWN_DUD_FX".to_string()], "the dud row's cell, then the def's own");
    assert_eq!(crate::sounds::dud_cues("OWN_DUD", Surface::Concrete), [Some("DUD_HIT_C"), Some("OWN_DUD")]);
    assert_eq!(crate::sounds::dud_cues("OWN_DUD", Surface::Dirt), [None, Some("OWN_DUD")], "no large-bullet alias on dirt: silent");
    // A direct hit before arming: the impact damage (`iDamage`) only, no splash.
    let (ev, damage, _) = shoot(gl(100.0), true);
    assert!(ev.iter().any(|k| matches!(k, EventKind::Dud { surface: Surface::Flesh, .. })), "{ev:?}");
    assert_eq!(damage.len(), 1, "{damage:?}");
    assert_eq!((damage[0].1, damage[0].2), (5, 75.0 * crate::units::BO2_TO_ZH_DAMAGE));
    // Armed (no activation distance): the same shot goes off.
    let (ev, _, _) = shoot(gl(0.0), false);
    assert!(ev.iter().any(|k| matches!(k, EventKind::Exploded { .. })) && !ev.iter().any(|k| matches!(k, EventKind::Dud { .. })));
}

/// BO2's crossbow (`projExplosion` bolt, t6mp `FUN_00608c10`): the bolt strikes for its impact
/// damage, becomes its spawned grenade stuck in what it hit, and that goes off on its own fuse with
/// its own blast. Here the bolt spawns the built-in frag (the converted table carries
/// `explosive_bolt_mp`); before the fix a bolt only stuck, with no fuse, and never hurt anyone.
#[test]
fn a_crossbow_bolt_strikes_then_its_spawned_grenade_goes_off_in_him() {
    let frag = crate::weapons::weapon("frag_grenade_mp");
    let mut damage = [0.0; 6];
    damage[0] = 40.0;
    let bolt: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef {
        id: "test_crossbow_mp".into(),
        class: Class::Special,
        effect: crate::weapons::Effect::Explode,
        sticky: true,
        spawns: frag.id.clone(),
        projectile_speed: crate::units::inches(4500.0),
        damage,
        ..Default::default()
    }));
    let host = FakeHost::default();
    host.add(general(1, 0));
    // 150 inches off: inside the reach the old 0.08 s arm time left untouchable at this speed.
    host.add(ObjInfo { health: 1000.0, max_health: 1000.0, ..soldier(5, 1, crate::units::inches(150.0)) });
    let mut sim = Sim::new();
    sim.projectiles.push((0, Projectile::launch(bolt, 1, Vec3::new(0.0, 0.0, 6.0), Vec3::new(1.0, 0.0, 0.0), 0.0, &host, 0.0)));
    run(&mut sim, &host, 0, 1, FpsInput::default(), 3);
    assert_eq!(host.damage_log.borrow().clone(), vec![(1, 5, 40.0 * crate::units::BO2_TO_ZH_DAMAGE)], "the strike's impact damage");
    assert!(events(&sim).iter().any(|k| matches!(k, EventKind::HitMarker { kill: false, .. })));
    assert_eq!(sim.projectiles.len(), 1, "the bolt became its grenade");
    assert_eq!(sim.projectiles[0].1.def.id, frag.id);
    assert_eq!(sim.projectiles[0].1.stuck_to, Some(5), "stuck in him");
    run(&mut sim, &host, 0, 1, FpsInput::default(), frames(frag.fuse_time) - 2);
    // (The shooter, inside the frag's radius, takes his share too.)
    let log: Vec<_> = host.damage_log.borrow().iter().copied().filter(|h| h.1 == 5).collect();
    assert_eq!(log.len(), 2, "{log:?}");
    let blast = frag.explosion_damage_at(0.0) * crate::units::BO2_TO_ZH_DAMAGE;
    assert!((log[1].2 - blast).abs() < 1e-3, "the grenade's blast at its centre: {log:?}");
    assert!(events(&sim).iter().any(|k| matches!(k, EventKind::Exploded { .. })));
}

#[test]
fn a_smoke_grenade_screens_for_its_time_then_clears() {
    let host = FakeHost::default();
    host.add(general(1, 0));
    let mut sim = Sim::new();
    let smoke: &'static crate::weapons::WeaponDef =
        Box::leak(Box::new(crate::weapons::WeaponDef { id: "willy_pete_mp".into(), effect: crate::weapons::Effect::Smoke, fuse_time: 0.5, ..Default::default() }));
    let p = Projectile::launch(smoke, 1, Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0), 0.5, &host, 0.0);
    sim.projectiles.push((0, p));
    run(&mut sim, &host, 0, 1, FpsInput::default(), frames(1.0));
    assert_eq!(sim.smokes.len(), 1);
    assert!(!host.smoked.borrow().is_empty());
    run(&mut sim, &host, 0, 1, FpsInput::default(), frames(crate::projectiles::SMOKE_TIME));
    assert!(sim.smokes.is_empty(), "cleared after {} s", crate::projectiles::SMOKE_TIME);
}

#[test]
fn identical_inputs_give_identical_results() {
    let make = || {
        let host = FakeHost::default();
        host.add(general(1, 0));
        host.add(soldier(2, 1, 80.0));
        host.add(soldier(4, 1, 90.0));
        let mut sim = Sim::new();
        sim.take_control(&host, 0, 1);
        for i in 0..200u32 {
            let input = FpsInput {
                held: if i % 7 < 4 { buttons::FIRE } else { buttons::SPRINT },
                move_packed: pack_move(((i % 11) as f32 / 5.0) - 1.0, ((i % 5) as f32 / 2.0) - 1.0),
                yaw: (i as f32 * 0.01).sin(),
                pitch: -0.05,
                ..Default::default()
            };
            sim.input(0, 1, input);
            sim.step(&host, i + 1);
        }
        let hits = host.damage_log.borrow().len();
        (host.get(1).pos, host.get(2).health, host.get(4).health, hits)
    };
    assert_eq!(make(), make());
}

/// The knife's lunge (BO2's melee charge, `FUN_008dc770`/`FUN_008c3770`): with an enemy in front
/// past `player_meleeRange` but inside the knife's `meleeChargeRange`, the melee charges — the
/// General closes to the reach on the charge's decelerating move and the strike lands with the
/// charge delay on the enemy it charges at. A knife without a charge range swings at the air.
fn lunge(charge_range: f32) -> (f32, f32) {
    let host = FakeHost::default();
    host.add(general(1, 0));
    host.add(ObjInfo { health: 5000.0, max_health: 5000.0, ..soldier(2, 1, 18.0) });
    let mut sim = Sim::new();
    sim.take_control(&host, 0, 1);
    sim.choose_class(0, 1, 0, &[]);
    wait_until_ready(&mut sim, &host);
    let p = sim.players[0].as_mut().unwrap();
    let mut knife = p.arsenal.class_melee.clone();
    (knife.melee_charge_range, knife.melee_charge_time, knife.melee_charge_delay, knife.melee_delay) = (charge_range, 1.16, 0.1, 0.05);
    p.arsenal.class_melee = Box::leak(Box::new(knife));
    host.damage_log.borrow_mut().clear();
    run(&mut sim, &host, 0, 1, FpsInput { held: buttons::MELEE, pressed: buttons::MELEE, ..Default::default() }, 1);
    run(&mut sim, &host, 0, 1, FpsInput::default(), 20);
    let dealt = host.damage_log.borrow().iter().filter(|d| d.1 == 2).map(|d| d.2).sum::<f32>();
    (sim.player(0).unwrap().body.pos.x, dealt)
}

#[test]
fn the_knife_lunges_past_its_reach_and_strikes() {
    let reach = crate::weapons::MELEE_RANGE + crate::hitscan::INFANTRY_HIT_RADIUS;
    // The install's held knife: 116 in of charge, 1.16 s charge time, 0.1 s charge delay.
    let (x, dealt) = lunge(crate::units::inches(116.0));
    assert!(dealt > 0.0, "the lunge reached him");
    assert!((x - (18.0 - reach)).abs() < 1.0, "stopped at the reach: x {x}, want {}", 18.0 - reach);
    let (x, dealt) = lunge(0.0);
    assert_eq!(dealt, 0.0, "no charge range: out of reach");
    assert!(x.abs() < 0.5, "and no lunge: x {x}");
    assert_eq!(crate::weapons::MELEE_RANGE, crate::units::inches(crate::tuning::Tuning::default().player_melee_range), "the reach is the tuning pack's player_meleeRange");
}
