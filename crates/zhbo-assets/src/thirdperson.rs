/*
** Conquer Ops - converter: the third-person body's MP animations (Track R7).
**
** BO2's MP bodies share one player skeleton; the body clips are BO2's `pb_*` body animations in
** `common_mp` (the same zone the third-person W3D conversion reads: `generals::ANIMS`). The mod's
** native body plays one clip per movement state (`zhbo_native::thirdperson::state`) of the held
** weapon's animation set; this module groups the weapons into sets by what BO2's own player
** animation script (`mp/playeranim.script`, `crate::playeranim`) plays for their `weapClass` and
** `playerAnimType`, and resolves each set's states to BO2 animations by asking the script (the
** rifleman's, set 0, also by its known names, else by scanning the zone for a plausible one).
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/
use zhbo_formats::t6zone::Zone;
use zhbo_formats::xanim::{self, Anim};
use zhbo_native::thirdperson::state;

/// One clip of the body manifest: its id (`state::id` of an animation set and a movement state).
pub(crate) struct Clip {
    pub state: u16,
    pub anim: Anim,
}

/// The names the W3D path already proves exist (`generals::ANIMS`), then the stances it never
/// converted: the rifleman's (set 0) when the install has no script. The first name in a state's
/// list that the install has is the one used.
const CANDIDATES: &[(u16, &[&str])] = &[
    (state::STAND_IDLE, &["pb_stand_alert", "pb_stand_idle", "pb_stand_idle_loop"]),
    (state::STAND_WALK, &["pb_walk_forward_loop", "pb_combatwalk_forward_loop", "pb_walk_fwd_loop"]),
    (state::STAND_RUN, &["pb_combatrun_forward_loop", "pb_run_forward_loop", "pb_combatrun_fwd_loop"]),
    (state::STAND_SPRINT, &["pb_sprint_forward_loop", "pb_sprint_loop", "pb_combatrun_sprint_loop"]),
    (state::STAND_ADS, &["pb_stand_ads"]),
    (state::CROUCH_IDLE, &["pb_crouch_alert", "pb_crouch_idle", "pb_ducked_alert", "pb_ducked_idle"]),
    (state::CROUCH_WALK, &["pb_crouch_walk_forward_loop", "pb_crouchwalk_forward_loop", "pb_crouch_combatrun_forward_loop", "pb_ducked_walk_loop"]),
    (state::CROUCH_ADS, &["pb_crouch_ads", "pb_ducked_ads"]),
    (state::PRONE_IDLE, &["pb_prone_alert", "pb_prone_idle"]),
    (state::PRONE_CRAWL, &["pb_prone_forward_loop", "pb_prone_crawl_forward_loop", "pb_prone_fwd_loop"]),
];

/// A weapon as BO2's player animation script reads it: its `weapClass` and `playerAnimType` names.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Key {
    pub class: String,
    pub anim_type: String,
}

impl Key {
    /// The rifleman: the script's `default` entries ("All player states should have animations
    /// sutable for a two handed rifle type weapon").
    pub fn rifle() -> Key {
        Key { class: "rifle".into(), anim_type: "default".into() }
    }

    /// BO2 weapon `w`'s key: its `weapClass` (`WeapClass::name`) and the `playerAnimType` line of
    /// `types` (`mp/playeranimtypes.txt`; an index past it reads `default`).
    pub fn of(w: &zhbo_formats::weapon::Weapon, types: &[String]) -> Key {
        Key {
            class: zhbo_core::weapons::WeapClass::from_bo2(w.class).name().into(),
            anim_type: types.get(w.player_anim_type as usize).cloned().unwrap_or_else(|| "default".into()),
        }
    }
}

/// What one animation set plays: the body clip of every movement state (`state::COUNT` names,
/// `None` where the script and the install have none), and the torso clips of its `fireweapon`
/// (aimed) and `reload` events, which the W3D body layers over the stance. Sets are told apart by
/// their movement clips; the torso clips are those of the set's plain (`default`) weapon (the
/// W3D body carries one fire and one reload per set; the native body has no torso layer).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SetNames {
    pub states: Vec<Option<String>>,
    pub fire: Option<String>,
    pub reload: Option<String>,
}

impl SetNames {
    /// Movement state `st`'s clip.
    pub fn state(&self, st: u16) -> Option<&str> {
        self.states.get(st as usize - 1).and_then(|s| s.as_deref())
    }
}

/// The animation sets of the converted weapons: set 0 the rifleman's, then one per distinct way
/// the script animates the others, and each weapon key's set.
pub(crate) struct Sets {
    pub sets: Vec<SetNames>,
    keys: Vec<(Key, u8)>,
    dropped: Vec<Key>,
}

impl Sets {
    /// The set of a weapon with key `k` (0 for a key the build never saw).
    pub fn of(&self, k: &Key) -> u8 {
        self.keys.iter().find(|(x, _)| x == k).map_or(0, |(_, s)| *s)
    }

    /// The weapons' keys and their sets, for the summary.
    pub fn keys(&self) -> impl Iterator<Item = &(Key, u8)> {
        self.keys.iter()
    }

    /// The keys past [`state::MAX_SETS`] distinct sets, which keep the rifleman's.
    pub fn dropped(&self) -> &[Key] {
        &self.dropped
    }
}

/// The script's movement type and conditions for movement state `st` of a soldier holding a weapon
/// with key `k`: hip or ADS, moving forward or still. Prone movement is the script's `walk` (its
/// movement types have no prone run); crouched movement its `run` (`runcr`).
fn script_query(st: u16, k: &Key) -> (&'static str, [(&'static str, &str); 6]) {
    let ctx = |stance, pos, status, dir| [("stance", stance), ("weapon_position", pos), ("weaponclass", k.class.as_str()), ("movestatus", status), ("direction", dir), ("playeranimtype", k.anim_type.as_str())];
    match st {
        state::STAND_WALK => ("walk", ctx("stand", "hip", "walk", "forward")),
        state::STAND_RUN => ("run", ctx("stand", "hip", "run", "forward")),
        state::STAND_SPRINT => ("sprint", ctx("stand", "hip", "run", "forward")),
        state::STAND_ADS => ("idle", ctx("stand", "ads", "stationary", "none")),
        state::CROUCH_IDLE => ("idle", ctx("crouch", "hip", "stationary", "none")),
        state::CROUCH_WALK => ("run", ctx("crouch", "hip", "run", "forward")),
        state::CROUCH_ADS => ("idle", ctx("crouch", "ads", "stationary", "none")),
        state::PRONE_IDLE => ("idle", ctx("prone", "hip", "stationary", "none")),
        state::PRONE_CRAWL => ("walk", ctx("prone", "hip", "walk", "forward")),
        _ => ("idle", ctx("stand", "hip", "stationary", "none")),
    }
}

/// What the script plays for key `k` (`script` over the zone's animation `names`; a clip the zone
/// lacks is none). The rifleman's states the script leaves empty take [`CANDIDATES`], then a
/// [`plausible`] scan of the zone.
fn resolve(script: Option<&crate::playeranim::Script>, names: &[String], k: &Key) -> SetNames {
    let have = |c: &str| names.iter().find(|n| n.eq_ignore_ascii_case(c)).cloned();
    let rifle = *k == Key::rifle();
    let states = (1..=state::COUNT)
        .map(|st| {
            let (mv, ctx) = script_query(st, k);
            let scripted = script.and_then(|s| s.pick(mv, &ctx)).and_then(have);
            if !rifle {
                return scripted;
            }
            let candidates = CANDIDATES.iter().find(|c| c.0 == st).map_or(&[][..], |c| c.1);
            scripted.or_else(|| candidates.iter().find_map(|c| have(c))).or_else(|| names.iter().find(|n| plausible(st, n)).cloned())
        })
        .collect();
    let event = |ev: &str, pos: &'static str| {
        let ctx = [("stance", "stand"), ("weapon_position", pos), ("weaponclass", k.class.as_str()), ("movestatus", "stationary"), ("direction", "none"), ("playeranimtype", k.anim_type.as_str())];
        script.and_then(|s| s.event(ev, &ctx)).and_then(have)
    };
    SetNames { states, fire: event("fireweapon", "ads"), reload: event("reload", "hip") }
}

/// The animation sets of the weapons with `keys` from `common`'s player animation script: set 0 is
/// the rifleman's; a key the script animates exactly as the rifleman shares it, and keys animated
/// alike share one set. Without the script every key is the rifleman's. The W3D body has a model
/// condition per set, so past [`state::MAX_SETS`] a key keeps the rifleman's (listed by
/// [`Sets::dropped`] for the converter's summary).
pub(crate) fn sets(common: &Zone, keys: impl IntoIterator<Item = Key>) -> Sets {
    let script = zhbo_formats::havok_lua::rawfile(common, crate::playeranim::PATH).map(|b| crate::playeranim::Script::parse(&String::from_utf8_lossy(&b)));
    let names = anim_names(common);
    let mut all: Vec<Key> = keys.into_iter().collect();
    all.sort();
    all.dedup();
    let mut sets = vec![resolve(script.as_ref(), &names, &Key::rifle())];
    let mut out = Vec::new();
    for k in all {
        let mut own = resolve(script.as_ref(), &names, &k);
        // A state the script gives this key no clip for is the rifleman's.
        for (st, c) in own.states.iter_mut().enumerate() {
            if c.is_none() {
                c.clone_from(&sets[0].states[st]);
            }
        }
        let set = match sets.iter().position(|s| s.states == own.states) {
            Some(i) => {
                // The set's torso clips are its plain weapon's (the keys come sorted, so the first
                // `default` member wins); the rifleman's stay the rifleman's.
                if i > 0 && k.anim_type == "default" && !out.iter().any(|(m, s): &(Key, u8)| *s as usize == i && m.anim_type == "default") {
                    (sets[i].fire, sets[i].reload) = (own.fire, own.reload);
                }
                i
            }
            None => {
                sets.push(own);
                sets.len() - 1
            }
        };
        out.push((k, set as u8));
    }
    let mut dropped = Vec::new();
    if sets.len() > state::MAX_SETS as usize {
        sets.truncate(state::MAX_SETS as usize);
        for (k, s) in out.iter_mut().filter(|(_, s)| *s >= state::MAX_SETS) {
            dropped.push(k.clone());
            *s = 0;
        }
    }
    Sets { sets, keys: out, dropped }
}

/// Whether the third-person body holds weapon `d` in hand and walks with it: a gun, a launcher,
/// the knife, the shield, an alt mode, a scorestreak weapon (BO2's primary, alt-mode and melee
/// inventories). Offhands are thrown or placed from the held weapon's stance (the W3D body's
/// throw), and a dual-wield left gun rides its right gun.
pub(crate) fn held_in_hand(d: &zhbo_core::weapons::WeaponDef) -> bool {
    use zhbo_core::weapons::Inventory;
    matches!(d.inventory, Inventory::Primary | Inventory::AltMode | Inventory::Melee)
}

/// The animation sets of every weapon def in `defs` held in hand ([`held_in_hand`], by its BO2
/// weapon in `bo2`), each def's `body_anims` set to its own. Defs with no BO2 weapon of their name
/// (a combination, an alt mode) are left alone: they copy their own def's set when they are built.
pub(crate) fn assign_sets(common: &Zone, bo2: &[zhbo_formats::weapon::Weapon], defs: &mut [zhbo_core::weapons::WeaponDef]) -> Sets {
    let types = anim_types(common);
    let key = |d: &zhbo_core::weapons::WeaponDef| bo2.iter().find(|w| w.name == d.id).filter(|_| held_in_hand(d)).map(|w| Key::of(w, &types));
    let sets = sets(common, defs.iter().filter_map(key));
    for d in defs.iter_mut() {
        if let Some(k) = key(d) {
            d.body_anims = sets.of(&k);
        }
    }
    sets
}

/// Every animation name in `zone`.
fn anim_names(zone: &Zone) -> Vec<String> {
    zone.assets.iter().filter(|a| a.kind == zhbo_formats::fastfile::ASSET_XANIM).filter_map(|a| zone.asset_name(a)).map(|n| n.trim_start_matches(',').to_string()).collect()
}

/// The `playerAnimType` names of `common` (`mp/playeranimtypes.txt`), empty without the file.
pub(crate) fn anim_types(common: &Zone) -> Vec<String> {
    zhbo_formats::havok_lua::rawfile(common, crate::playeranim::TYPES_PATH).map(|b| crate::playeranim::anim_types(&String::from_utf8_lossy(&b))).unwrap_or_default()
}

/// The body manifest's clips from `common`: every movement state of set 0, and of every other set
/// the states whose clip is not the rifleman's (the run-time fallback reaches set 0's for the
/// rest, `state::fallbacks`); and the movement states set 0 resolved to nothing (the body then
/// falls back to a nearer stance at run time).
pub(crate) fn clips(common: &Zone, sets: &Sets) -> (Vec<Clip>, Vec<u16>) {
    let mut out = Vec::new();
    let mut missing = Vec::new();
    let mut cache: Vec<(String, Anim)> = Vec::new();
    for (set, names) in sets.sets.iter().enumerate() {
        for st in 1..=state::COUNT {
            let name = names.state(st);
            if set > 0 && name == sets.sets[0].state(st) {
                continue;
            }
            let anim = name.and_then(|n| match cache.iter().find(|(c, _)| c == n) {
                Some((_, a)) => Some(a.clone()),
                None => xanim::find(common, n).ok().inspect(|a| cache.push((n.to_string(), a.clone()))),
            });
            match anim {
                Some(anim) => out.push(Clip { state: state::id(set as u8, st), anim }),
                None if set == 0 => missing.push(st),
                None => {}
            }
        }
    }
    (out, missing)
}

/// Whether `name` is plausibly the animation for `state`, for the states whose spelling this
/// build's candidate list did not name. Words that mark a different stance or a transition are
/// refused, so the scan cannot pick (say) a pistol or an ADS clip for the rifle run.
fn plausible(state: u16, name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let has = |w: &str| n.contains(w);
    // Not a body-locomotion clip at all (a viewmodel's or anything else not `pb_*`, deaths, jumps,
    // turns, the torso-only `pt_*` clips...).
    let locomotion = n.starts_with("pb_") && ![
        "death", "jump", "land", "turn", "climb", "mantle", "melee", "grenade", "tablet", "reload", "backward", "back_", "left", "right", "strafe", "swim",
        "kill", "hit", "vault", "dive", "slide", "crawl_start", "crawl_end", "getup", "godown",
    ]
    .iter()
    .any(|w| has(w));
    let pistol = has("pistol");
    let ads = has("ads");
    match state {
        state::STAND_IDLE => has("stand") && (has("alert") || has("idle")) && !pistol && !ads && locomotion,
        state::STAND_WALK => has("walk") && (has("forward") || has("fwd")) && has("loop") && !pistol && !ads && locomotion,
        state::STAND_RUN => has("run") && (has("forward") || has("fwd")) && has("loop") && !has("sprint") && !pistol && !ads && locomotion,
        state::STAND_SPRINT => has("sprint") && has("loop") && !pistol && !ads && locomotion,
        state::STAND_ADS => has("stand") && ads && !pistol && locomotion,
        state::CROUCH_IDLE => (has("crouch") || has("ducked")) && (has("alert") || has("idle")) && !pistol && !ads && locomotion,
        state::CROUCH_WALK => (has("crouch") || has("ducked")) && (has("walk") || has("run")) && has("loop") && !pistol && !ads && locomotion,
        state::CROUCH_ADS => (has("crouch") || has("ducked")) && ads && !pistol && locomotion,
        state::PRONE_IDLE => has("prone") && (has("alert") || has("idle")) && !pistol && !ads && locomotion,
        state::PRONE_CRAWL => has("prone") && (has("forward") || has("fwd") || has("crawl")) && has("loop") && !pistol && !ads && locomotion,
        _ => false,
    }
}

/// The soldier's hit boxes in every body state of `clips`: every bone of `body` with an `XBoneInfo`
/// big enough for BO2's trace (`radiusSquared` at least [`zhbo_core::hitloc::MIN_RADIUS_SQUARED`])
/// and a location the trace can hit, at the bone's model-space transform in the first frame of
/// the state's clip. BO2 traces the pose the server animated (stance, gait, ADS); the sim picks the
/// state from its own movement state (`zhbo_core::nativebody::state_of`), the same clips the
/// third-person body draws, and Zero Hour's infantry take the stand-idle pose.
pub(crate) fn hit_boxes(body: &zhbo_formats::xmodel::Model, clips: &[Clip]) -> Vec<zhbo_core::hitloc::HitBox> {
    use zhbo_native::pose::{PartRef, Skeleton};
    let model = zhbo_native::model::Model { name: body.name.clone(), bones: crate::native::bones(body), ..Default::default() };
    let (skel, _) = Skeleton::compose(&[PartRef { model: &model, tag: None, offset: zhbo_native::math::IDENTITY, rigid: false }], &[], &[]);
    let (mut local, mut pose) = (Vec::new(), Vec::new());
    let mut out = Vec::new();
    for clip in clips {
        skel.sample(&crate::native::anim(&clip.anim), 0.0, &mut local);
        skel.model_space(&local, &mut pose);
        out.extend(
            body.hit_boxes
                .iter()
                .enumerate()
                .filter(|(_, h)| h.radius_squared >= zhbo_core::hitloc::MIN_RADIUS_SQUARED)
                .filter(|(_, h)| zhbo_core::hitloc::rank(h.location, true).is_some())
                .map(|(i, h)| zhbo_core::hitloc::HitBox {
                    state: clip.state,
                    location: h.location,
                    bone: body.bones[i].name.clone(),
                    rotation: pose[i].0,
                    position: pose[i].1,
                    mins: h.mins,
                    maxs: h.maxs,
                }),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scan_recognises_the_stances_by_their_words() {
        // Known names, and a spelling the candidate list does not carry.
        assert!(plausible(state::STAND_IDLE, "pb_stand_alert"));
        assert!(plausible(state::STAND_WALK, "pb_walk_fwd_loop"));
        assert!(plausible(state::STAND_RUN, "pb_combatrun_forward_loop"));
        assert!(plausible(state::STAND_SPRINT, "pb_sprint_forward_loop"));
        assert!(plausible(state::CROUCH_IDLE, "pb_crouch_alert"));
        assert!(plausible(state::CROUCH_WALK, "pb_crouch_walk_forward_loop"));
        assert!(plausible(state::PRONE_CRAWL, "pb_prone_forward_loop"));
        // Other stances and transitions are refused.
        assert!(!plausible(state::STAND_WALK, "pb_walk_backward_loop"), "backward is not forward");
        assert!(!plausible(state::STAND_RUN, "pb_sprint_forward_loop"), "sprint has its own state");
        assert!(!plausible(state::STAND_IDLE, "pb_stand_ads"), "ADS has its own state");
        assert!(!plausible(state::STAND_IDLE, "pb_stand_alert_pistol"), "the pistol has its own state");
        assert!(!plausible(state::CROUCH_ADS, "pb_crouch_alert"));
        assert!(!plausible(state::PRONE_IDLE, "pb_prone_getup"), "a transition is not the idle");
        assert!(!plausible(state::STAND_RUN, "pt_rifle_fire_ads"), "a torso clip is not locomotion");
        assert!(!plausible(state::STAND_SPRINT, "viewmodel_mp7_sprint_loop"), "a viewmodel clip is not the body's");
    }

    /// Every weapon the third-person body holds plays, in every movement state, the clip BO2's own
    /// player animation script gives its `weapClass` and `playerAnimType` — through its def's
    /// `body_anims`, the body manifest's clip ids and their run-time fallback — and every
    /// combination keeps its base weapon's set. `--nocapture` prints the sweep.
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn every_weapon_takes_the_body_animations_bo2_gives_it() {
        use zhbo_native::thirdperson::Body;
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let mp = crate::paks::MpPool::load(&dir).unwrap();
        let (bo2, atts) = mp.weapons().unwrap();
        let types = anim_types(&mp.common);
        assert!(types.len() > 20 && types[0] == "none" && types[1] == "default", "{types:?}");
        let roster = crate::weapons::Roster::read(&mp.patch);
        let mut table = crate::weapons::table(&bo2, &atts, None, &Default::default(), &roster);
        let sets = assign_sets(&mp.common, &bo2, &mut table.weapons);
        assert!(sets.dropped().is_empty(), "sets past the body's: {:?}", sets.dropped());
        let (clips, missing) = clips(&mp.common, &sets);
        assert!(missing.is_empty(), "rifleman states without a clip: {missing:?}");
        let body = Body { clips: clips.iter().map(|c| (c.state, c.anim.name.trim_start_matches(',').to_ascii_lowercase())).collect(), ..Default::default() };
        // The script, asked again for each weapon's own key.
        let script = crate::playeranim::Script::parse(&String::from_utf8_lossy(&zhbo_formats::havok_lua::rawfile(&mp.common, crate::playeranim::PATH).unwrap()));
        let names = anim_names(&mp.common);
        let have = |c: &str| names.iter().any(|n| n.eq_ignore_ascii_case(c));
        let rifle = |st: u16| sets.sets[0].state(st).unwrap().to_ascii_lowercase();
        let mut held = 0;
        for d in table.weapons.iter().filter(|d| held_in_hand(d)) {
            let Some(w) = bo2.iter().find(|w| w.name == d.id) else { continue };
            held += 1;
            let k = Key::of(w, &types);
            let mut row = Vec::new();
            for st in 1..=state::COUNT {
                let (mv, ctx) = script_query(st, &k);
                let want = script.pick(mv, &ctx).filter(|c| have(c)).map_or_else(|| rifle(st), str::to_ascii_lowercase);
                let got = body.clip(state::id(d.body_anims, st)).unwrap_or_else(|| panic!("{}: no clip for state {st}", d.id));
                assert_eq!(got, want, "{} ({}/{}) state {st}", d.id, k.class, k.anim_type);
                row.push(got.to_string());
            }
            eprintln!("{:<22} {:<15} {:<15} set {} {}", d.id, k.class, k.anim_type, d.body_anims, row.join(" "));
        }
        assert!(held > 60, "{held} held weapons");
        // BO2's own picks for the weapons the playtest walks with: the secondaries and specials.
        let walk = |id: &str, st: u16| {
            let d = table.weapons.iter().find(|d| d.id == id).unwrap_or_else(|| panic!("no {id}"));
            body.clip(state::id(d.body_anims, st)).unwrap().to_string()
        };
        for (id, st, want) in [
            ("fnp45_mp", state::STAND_WALK, "pb_combatwalk_forward_loop_pistol"),
            ("fnp45_mp", state::STAND_RUN, "pb_pistol_run_fast"),
            ("fnp45_mp", state::STAND_IDLE, "pb_stand_alert_pistol"),
            ("judge_mp", state::STAND_WALK, "pb_combatwalk_forward_loop_pistol"),
            ("fnp45_dw_mp", state::STAND_WALK, "pb_dw_walk_forward"),
            ("smaw_mp", state::STAND_WALK, "pb_walk_forward_rpg_ads"),
            ("usrpg_mp", state::STAND_RUN, "pb_combatrun_forward_rpg"),
            ("fhj18_mp", state::STAND_IDLE, "pb_stand_alert_rpg"),
            ("knife_held_mp", state::STAND_WALK, "pb_knife_walk_forward"),
            ("knife_ballistic_mp", state::STAND_WALK, "pb_b_knife_walk_forward"),
            ("riotshield_mp", state::STAND_WALK, "pb_riotshield_walk_forward"),
            ("crossbow_mp", state::STAND_WALK, "pb_stand_shoot_walk_forward"),
            ("mp7_mp", state::STAND_SPRINT, "pb_smg_sprint"),
            ("lsat_mp", state::STAND_SPRINT, "pb_lmg_sprint"),
            ("an94_mp", state::STAND_SPRINT, "pb_sprint"),
            ("ksg_mp", state::CROUCH_WALK, "pb_crouch_run_forward"),
        ] {
            assert_eq!(walk(id, st), want, "{id} state {st}");
        }
        // A combination and an alt mode copy their own def's set.
        crate::weapons::add_variants(&mut table, &crate::weapons::converted(&mp.patch, &bo2), &bo2);
        let mut combos = 0;
        for d in table.weapons.iter().filter(|d| d.id.contains('+') && !d.id.contains('~')) {
            let base = zhbo_core::attachments::split_id(&d.id).0;
            let b = table.weapons.iter().find(|x| x.id == base).unwrap();
            assert_eq!(d.body_anims, b.body_anims, "{} vs its base", d.id);
            combos += 1;
        }
        assert!(combos > 100, "{combos} combinations");
    }

    #[test]
    fn every_state_has_a_candidate_and_a_scan_rule() {
        for st in 1..=state::COUNT {
            assert!(CANDIDATES.iter().any(|c| c.0 == st), "state {st} has no candidate list");
        }
        assert_eq!(CANDIDATES.len(), state::COUNT as usize, "one candidate list per state");
    }

    /// The soldier's hit boxes from this machine's install: BO2's MP body (the three faction
    /// bodies carry the same boxes) stood in its idle pose — a head box at head height, feet at the
    /// ground, and a torso far thinner than the old 32-inch cylinder.
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_soldier_hit_boxes_are_the_mp_bodys_standing() {
        use zhbo_core::hitloc::{self, Body};
        use zhbo_core::math::Vec3;
        use zhbo_core::units::inches;
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let common = crate::paks::load_zone(&dir, "common_mp").unwrap();
        let (clips, _) = clips(&common, &sets(&common, []));
        let stand = clips.iter().find(|c| c.state == state::STAND_IDLE).unwrap();
        let mut sets = Vec::new();
        for f in crate::generals::FACTIONS {
            let zone = crate::paks::load_zone(&dir, f.zone).unwrap();
            let body = zhbo_formats::xmodel::find(&zone, f.body).unwrap();
            sets.push(hit_boxes(&body, std::slice::from_ref(stand)));
        }
        assert!(sets.iter().all(|s| s == &sets[0]), "the faction bodies share their hit boxes");
        let boxes = &sets[0];
        for b in boxes {
            eprintln!("{:<16} {:<12} pos {:>6.1?} rot {:>6.3?}", b.bone, hitloc::NAMES[b.location as usize], b.position, b.rotation);
        }
        assert!(boxes.len() >= 17, "{} boxes", boxes.len());
        let body = Body::new(boxes);
        let x = Vec3::new(1.0, 0.0, 0.0);
        // Level shots from in front (+X is the body's forward) at head, chest and shin height.
        let shot_at = |y: f32, z: f32, rifle: bool| body.pick(Vec3::new(inches(-100.0), inches(y), inches(z)), x, 1000.0, rifle);
        let shot = |z: f32, rifle: bool| shot_at(0.0, z, rifle);
        assert!(shot(60.0, true).is_some_and(|(_, l)| hitloc::is_head(l)), "{:?}", shot(60.0, true));
        let chest = shot(47.0, false).expect("a chest");
        assert!((hitloc::TORSO_UPPER..=hitloc::TORSO_LOWER).contains(&chest.1), "{chest:?}");
        // The idle stance stands the feet apart: a shin is off the centre line.
        assert!(shot(12.0, false).is_none());
        assert!(shot_at(-9.0, 12.0, false).is_some_and(|(_, l)| (13..=18).contains(&l)), "{:?}", shot_at(-9.0, 12.0, false));
        assert!(shot(75.0, true).is_none(), "nothing above the head");
        // The torso, front to back, is far thinner than the old cylinder's 32 inches.
        // (At the waist, below the arms the idle holds across the chest.)
        let (back, back_loc) = body.pick(Vec3::new(inches(100.0), 0.0, inches(36.0)), -x, 1000.0, false).unwrap();
        let (front, front_loc) = body.pick(Vec3::new(inches(-100.0), 0.0, inches(36.0)), x, 1000.0, false).unwrap();
        assert_eq!((front_loc, back_loc), (hitloc::TORSO_LOWER, hitloc::TORSO_LOWER));
        let depth = (inches(200.0) - back - front) / inches(1.0);
        eprintln!("torso depth at 36 in: {depth:.1} in");
        assert!(depth > 4.0 && depth < 20.0, "{depth}");
    }

    /// Each stance's pose from this machine's install: the crouched head is lower than the
    /// standing one, the prone head near the ground, and a level shot at standing head height
    /// passes over a crouched and a prone soldier.
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_soldier_hit_boxes_follow_the_stance() {
        use zhbo_core::hitloc::{self, Poses};
        use zhbo_core::math::Vec3;
        use zhbo_core::units::inches;
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let common = crate::paks::load_zone(&dir, "common_mp").unwrap();
        let (clips, _) = clips(&common, &sets(&common, []));
        let f = &crate::generals::FACTIONS[0];
        let zone = crate::paks::load_zone(&dir, f.zone).unwrap();
        let boxes = hit_boxes(&zhbo_formats::xmodel::find(&zone, f.body).unwrap(), &clips);
        let poses = Poses::new(&boxes);
        // The head box's centre (model space, inches), from its posed bone.
        let head = |s: u16| {
            let b: Vec<_> = boxes.iter().filter(|b| b.state == s && b.bone == "head").cloned().collect();
            assert_eq!(b.len(), 1, "state {s}: one head bone");
            let p = &zhbo_core::hitloc::Body::new(&b).boxes[0];
            let c = (p.mins + p.maxs) * 0.5;
            (p.origin + p.axes[0] * c.x + p.axes[1] * c.y + p.axes[2] * c.z) * (1.0 / inches(1.0))
        };
        let (stand, crouch, prone) = (head(state::STAND_IDLE), head(state::CROUCH_IDLE), head(state::PRONE_IDLE));
        for c in &clips {
            eprintln!("state {:>2} `{}`: head {:.1?}", c.state, c.anim.name, head(c.state));
        }
        // BO2's player animation script's rifleman picks (`mp/playeranim.script`).
        let name = |s: u16| clips.iter().find(|c| c.state == s).map(|c| c.anim.name.trim_start_matches(',').to_string());
        for (s, want) in [
            (state::STAND_IDLE, "pb_stand_alert"),
            (state::STAND_WALK, "pb_stand_shoot_walk_forward"),
            (state::STAND_RUN, "pb_combatrun_forward_loop"),
            (state::STAND_SPRINT, "pb_sprint"),
            (state::CROUCH_IDLE, "pb_crouch_alert"),
            (state::CROUCH_WALK, "pb_crouch_run_forward"),
            (state::PRONE_IDLE, "pb_prone_aim"),
            (state::PRONE_CRAWL, "pb_prone_crawl"),
        ] {
            assert_eq!(name(s).as_deref(), Some(want), "state {s}");
        }
        assert!(crouch.z < stand.z - 10.0, "crouched head {crouch:?} vs standing {stand:?}");
        assert!(prone.z < 20.0, "prone head {prone:?}");
        // Level shots from in front (+X forward) at a head's height and side offset.
        let x = Vec3::new(1.0, 0.0, 0.0);
        let level = |s: u16, at: Vec3| poses.get(s).unwrap().pick(Vec3::new(inches(-200.0), inches(at.y), inches(at.z)), x, 1000.0, true);
        assert!(level(state::STAND_IDLE, stand).is_some_and(|(_, l)| hitloc::is_head(l)));
        assert!(level(state::CROUCH_IDLE, crouch).is_some_and(|(_, l)| hitloc::is_head(l)), "{:?}", level(state::CROUCH_IDLE, crouch));
        assert!(level(state::PRONE_IDLE, prone).is_some_and(|(_, l)| hitloc::is_head(l)), "{:?}", level(state::PRONE_IDLE, prone));
        assert!(level(state::CROUCH_IDLE, stand).is_none(), "over the crouched head");
        assert!(level(state::PRONE_IDLE, crouch).is_none(), "over the prone soldier");
    }
}
