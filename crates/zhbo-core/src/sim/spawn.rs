//! Classes and spawns: the player's chosen class, the General's record, and when he comes in.
//!
//! BO2 keeps two classes per player (`_globallogic_ui.gsc` `menuclass`, `_class.gsc`):
//! `pers["class"]`, the class the player chose, which belongs to the player and survives his
//! deaths, and `curclass`, the class this life was given at its spawn (`giveloadout`). Here the
//! first is [`Sim::chosen`] (per player seat) and the second is [`PlayerSim::class`] (per General
//! record, which dies with him). A new General's record is made wearing the chosen class, so after a
//! death the next General comes in with it and no class screen. A pick mid-life changes the chosen
//! class at once but the worn kit only in the grace period, before he has done any combat
//! (`level.ingraceperiod && !self.hasdonecombat`); otherwise BO2 prints
//! `MP_CHANGE_CLASS_NEXT_SPAWN` and the kit follows at the next spawn.
//!
//! "Spawned" is the General's first time in first person with a class: a Take Control on a classed
//! General, or the pick that ends his wait behind the class screen
//! ([`PlayerSim::awaiting_class`]). That is where the player's Tactical Insertion is used up: a
//! take-control he backs out of without a class never spawned him.

use super::*;

/// A class a player chose (BO2's `pers["class"]`): its id, display name, kit and the custom
/// class's per-slot options.
#[derive(Clone, Debug, PartialEq)]
pub struct ChosenClass {
    /// `classes.rs` id for a shipped class, `custom:<name>` for one built in the editor.
    pub id: String,
    pub name: ClassName,
    pub loadout: crate::weapons::Loadout,
    pub reticle: String,
    pub reticle_color: String,
    pub camo: String,
    pub knife_camo: String,
    pub secondary_camo: String,
    pub secondary_reticle: String,
}

crate::statehash::state_hash!(struct ChosenClass {
    id, name, loadout, reticle, reticle_color, camo, knife_camo, secondary_camo, secondary_reticle,
});

impl ChosenClass {
    /// The class a `MSG_ZHBO_CHOOSE_CLASS` names: a shipped class by registry index (empty
    /// `payload`, identical on every client), or a custom class's wire words, repaired to a legal
    /// class ([`crate::classrepair::repair_received`]) on receipt. `None` for an index past the list or
    /// words that do not decode: the pick is dropped.
    pub fn decode(index: usize, payload: &[u32]) -> Option<Self> {
        if payload.is_empty() {
            let c = crate::classes::at(index)?;
            return Some(ChosenClass {
                id: c.id.clone(),
                name: ClassName::new(&c.name),
                loadout: crate::classes::loadout_of(&c),
                reticle: String::new(),
                reticle_color: String::new(),
                camo: String::new(),
                knife_camo: String::new(),
                secondary_camo: String::new(),
                secondary_reticle: String::new(),
            });
        }
        let mut c = crate::classes::CustomClass::from_wire_words(payload)?;
        // Never trust the sender's class (BO2's server reads every slot through its checked
        // loadout getters): the editor's own repair pass, on every machine, before it is worn,
        // less the rules that read this machine's own install (DLC locks, camo lists: the
        // sender's to own). An illegal class is mended slot by slot (`classrepair`).
        let _ = crate::classrepair::repair_received(&mut c);
        let name = if c.name.is_empty() { "Custom Class".to_string() } else { c.name.clone() };
        Some(ChosenClass {
            id: format!("custom:{}", c.name),
            name: ClassName::new(&name),
            loadout: c.loadout(),
            reticle: c.reticle.clone(),
            reticle_color: c.reticle_color.clone(),
            camo: c.camo.clone(),
            knife_camo: c.knife.clone(),
            secondary_camo: c.secondary_camo.clone(),
            secondary_reticle: c.secondary_reticle.clone(),
        })
    }
}

impl PlayerSim {
    /// Give him the class's kit (BO2's `setclass` + `giveloadout`). In first person the weapon
    /// still has to come up (the pick lands between fights, not mid-swing); overhead the kit is
    /// ready for the next time he takes the field without a re-raise.
    pub fn wear(&mut self, c: &ChosenClass) {
        self.class = Some(c.id.clone());
        self.reticle.clone_from(&c.reticle);
        self.reticle_color.clone_from(&c.reticle_color);
        self.camo.clone_from(&c.camo);
        self.knife_camo.clone_from(&c.knife_camo);
        self.secondary_camo.clone_from(&c.secondary_camo);
        self.secondary_reticle.clone_from(&c.secondary_reticle);
        self.arsenal = Arsenal::with_loadout(c.loadout.clone());
        if matches!(self.mode, Mode::Fps) {
            let first = self.arsenal.current().def.first_raise_time;
            self.arsenal.raise_left = TAKE_CONTROL_READY + first;
            self.arsenal.raise_total = first;
            self.arsenal.raise_kind = crate::weapons::RaiseKind::First;
            self.arsenal.drop_left = 0.0;
            self.arsenal.alt_pending = false;
            self.arsenal.offhand = None;
            self.arsenal.ads = 0.0;
        } else {
            self.arsenal.raise_left = 0.0;
            self.arsenal.raise_total = 0.0;
        }
    }
}

/// Whether an event is BO2's `hasdonecombat`: a weapon fired (`weapon_fired`), a grenade pulled
/// back or thrown (`grenade_pullback`), a missile launched (`missile_fire`), or damage taken
/// (`_globallogic_player.gsc`'s damage callback) — `_weapons.gsc:656,779,805`,
/// `_globallogic_player.gsc:1076`.
pub(crate) fn is_combat(kind: &EventKind) -> bool {
    matches!(kind, EventKind::Shot { .. } | EventKind::Thrown { .. } | EventKind::Planted { .. } | EventKind::Launched | EventKind::Damaged { .. })
}

impl Sim {
    /// A fresh record for `general`, wearing the player's chosen class if he has one (a new
    /// General after a death comes in with it), else unarmed and unclassed.
    pub(crate) fn new_record(&self, idx: usize, general: u32, mode: Mode, info: Option<&ObjInfo>) -> PlayerSim {
        let (pos, yaw, health, max_health) = info.map_or((Vec3::ZERO, 0.0, 0.0, 0.0), |i| (i.pos, i.yaw, i.health, i.max_health));
        let mut p = PlayerSim {
            general,
            class: None,
            reticle: String::new(),
            reticle_color: String::new(),
            camo: String::new(),
            knife_camo: String::new(),
            secondary_camo: String::new(),
            secondary_reticle: String::new(),
            mode,
            body: Body::new(pos, &self.tuning),
            yaw,
            pitch: 0.0,
            view_kick: ViewKick::default(),
            idle: IdleSway::default(),
            gun: Default::default(),
            fire_yaw: 0.0,
            fire_pitch: 0.0,
            // No class: unarmed (knife only). Overhead the RTS report marks every slot empty; in
            // first person he waits behind the class screen.
            arsenal: Arsenal::unarmed(),
            rng: Rng::new(general.wrapping_mul(2_654_435_761) ^ self.frame),
            input: FpsInput { yaw, ..Default::default() },
            seen_frame: 0,
            pressed_accum: 0,
            health,
            max_health,
            regen_wait: 0.0,
            hack: None,
            rts: RtsState::default(),
            hammer: None,
            site: None,
            building: false,
            optics: Default::default(),
            tracer_count: 0,
            charge: None,
            shock: Default::default(),
            combat: false,
            spawned: false,
        };
        if let Some(Some(c)) = self.chosen.get(idx) {
            p.wear(c);
        }
        p
    }

    /// [`Sim::new_record`] for a General taking the place of the player's last (his first, or the
    /// next after a death): a class that waited for him (picked mid-life, `ClassAtNextSpawn`) comes
    /// in now, and the record says so with `ClassChosen` (the log's "assumed the ... class"), as
    /// an at-once pick does.
    pub(crate) fn fresh_record(&mut self, idx: usize, general: u32, mode: Mode, info: Option<&ObjInfo>) -> PlayerSim {
        let p = self.new_record(idx, general, mode, info);
        if std::mem::take(&mut self.class_waits[idx]) {
            if let (Some(c), Some(_)) = (self.chosen[idx].as_ref(), p.class.as_ref()) {
                let class = c.name;
                self.emit(idx, EventKind::ClassChosen { general, class });
            }
        }
        p
    }

    /// The player's chosen class (BO2's `pers["class"]`), which his next General spawns with.
    pub fn chosen_class(&self, player: i32) -> Option<&ChosenClass> {
        self.chosen.get(usize::try_from(player).ok()?)?.as_ref()
    }

    /// BO2's `level.ingraceperiod`: the match's first seconds, when a class change applies at once
    /// to a General who has not fought yet.
    pub fn in_grace_period(&self) -> bool {
        (self.frame as f32) * LOGIC_DT < crate::tuning::get().class_change_grace_period
    }

    /// `MSG_ZHBO_TAKE_CONTROL`: the player clicked "Take Control" on their General.
    pub fn take_control(&mut self, host: &dyn LogicHost, player: i32, general: u32) {
        let Ok(idx) = usize::try_from(player) else { return };
        if idx >= MAX_PLAYERS {
            return;
        }
        let Some(info) = host.object(general, general) else { return };
        if info.has(objflags::DEAD) || info.owner_player != player {
            return;
        }
        // One General per player in first person; release a previous one.
        if let Some(prev) = self.players[idx].as_ref() {
            if prev.general != general && prev.is_first_person() {
                host.set_controlled(prev.general, false);
            }
        }
        let tuning = self.tuning;
        let mut state = match self.players[idx].take() {
            Some(mut p) if p.general == general => {
                p.body = Body::new(info.pos, &tuning);
                p.mode = Mode::Fps;
                p.yaw = info.yaw;
                p.pitch = 0.0;
                p.view_kick = ViewKick::default();
                p.idle = IdleSway::default();
                p.gun = Default::default();
                p.input = FpsInput { yaw: info.yaw, ..Default::default() };
                p
            }
            _ => self.fresh_record(idx, general, Mode::Fps, Some(&info)),
        };
        // The weapon can't fire or aim until the tablet is down and it is up again.
        // BO2's first raise, once the tablet is down.
        let first = state.arsenal.current().def.first_raise_time;
        state.arsenal.raise_left = TAKE_CONTROL_READY + first;
        state.arsenal.raise_total = first;
        state.arsenal.raise_kind = crate::weapons::RaiseKind::First;
        state.arsenal.drop_left = 0.0;
        state.arsenal.alt_pending = false;
        state.arsenal.offhand = None;
        state.arsenal.ads = 0.0;
        self.players[idx] = Some(state);
        host.set_controlled(general, true);
        self.emit(idx, EventKind::EnteredFps { general });
        // A General with a class comes in now; one without waits behind the class screen and comes
        // in when his pick lands.
        self.spawn_in(host, idx);
    }

    /// The General's spawn, if this is it: his first time in first person with a class. He comes
    /// in at the player's Tactical Insertion, if one is down (it is used up), and his combat
    /// (`hasdonecombat`, reset on `spawned_player`) starts over.
    pub(crate) fn spawn_in(&mut self, host: &dyn LogicHost, idx: usize) {
        let Some(Some(p)) = self.players.get_mut(idx) else { return };
        if p.spawned || p.class.is_none() || !matches!(p.mode, Mode::Fps) {
            return;
        }
        p.spawned = true;
        p.combat = false;
        if let Some((pos, yaw)) = self.equipment.take_insertion(host, idx) {
            host.set_pose(p.general, pos, yaw, false);
            p.body = Body::new(pos, &self.tuning);
            p.yaw = yaw;
            p.input.yaw = yaw;
        }
    }

    /// `MSG_ZHBO_CHOOSE_CLASS`: the player picked a class from the class screen or the command bar
    /// (`index` and `payload` as [`ChosenClass::decode`] takes them). BO2's `menuclass`:
    /// * the class is the player's from now on (`pers["class"]`), whatever happens to the kit;
    /// * a General with no class yet takes it at once, and if he was waiting behind the class
    ///   screen in first person, that is his spawn (on the next step);
    /// * a classed General takes it at once only in the grace period before his first combat;
    ///   otherwise he keeps his kit and is told `MP_CHANGE_CLASS_NEXT_SPAWN` (his next General
    ///   comes in with it);
    /// * choosing the class already chosen does nothing.
    pub fn choose_class(&mut self, player: i32, general: u32, index: usize, payload: &[u32]) {
        let Ok(idx) = usize::try_from(player) else { return };
        if idx >= MAX_PLAYERS {
            return;
        }
        let Some(chosen) = ChosenClass::decode(index, payload) else { return };
        // The pick names the General under command; one naming another General is dropped.
        if self.players[idx].as_ref().is_some_and(|p| p.general != general) {
            return;
        }
        if self.chosen[idx].as_ref() == Some(&chosen) && self.players[idx].as_ref().is_some_and(|p| p.class.is_some()) {
            return;
        }
        self.chosen[idx] = Some(chosen.clone());
        let name = chosen.name;
        let grace = self.in_grace_period();
        // The class can be picked before the General is ever taken into first person (the command
        // bar's class set, overhead): make his record, which wears it, ready for the field.
        // His position and health are filled in from the object when he takes the field.
        let Some(p) = self.players[idx].as_mut() else {
            self.class_waits[idx] = false;
            self.players[idx] = Some(self.new_record(idx, general, Mode::Rts, None));
            self.emit(idx, EventKind::ClassChosen { general, class: name });
            return;
        };
        if p.class.is_none() || (grace && !p.combat) {
            // A wait behind the class screen ends with this: he spawns on the next step
            // (`Sim::spawn_in`, which needs the host for the Tactical Insertion).
            p.wear(&chosen);
            self.class_waits[idx] = false;
            self.emit(idx, EventKind::ClassChosen { general, class: name });
        } else {
            // Back to the class he wears: nothing waits (BO2 still prints the hint, `menuclass`).
            self.class_waits[idx] = p.class.as_ref() != Some(&chosen.id);
            self.emit(idx, EventKind::ClassAtNextSpawn { general, class: name });
        }
    }
}
