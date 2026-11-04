/*
** Conquer Ops - converter: BO2's player animation script (`mp/playeranim.script`).
**
** The script is how BO2's server and client pick a player's body animation: per movement type
** (`idle`, `walk`, `run`, `sprint`, ...) a list of entries, each a set of conditions (stance,
** weapon position, weapon class, direction, ...) and the animation it plays; "the first match found
** is the one used" (the script's own header). The converter reads it from `common_mp` and asks it
** which clip a rifleman in each of the mod's body states plays, so the third-person body and the
** hit boxes are posed by BO2's own choice instead of a guessed name.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/
use std::collections::HashMap;

/// The script's path in `common_mp`.
pub const PATH: &str = "mp/playeranim.script";
/// The `playerAnimType` names, one per line: a weapon def's `playerAnimType` is its line number
/// from 0 (`none`, `default`, `other`, ...).
pub const TYPES_PATH: &str = "mp/playeranimtypes.txt";

/// The `playerAnimType` names of [`TYPES_PATH`], in order (blank lines and comments skipped).
pub fn anim_types(text: &str) -> Vec<String> {
    text.lines().map(|l| l.split("//").next().unwrap_or("").trim()).filter(|l| !l.is_empty()).map(str::to_ascii_lowercase).collect()
}

/// One entry: its conditions (`key -> accepted values`; empty for `default`) and the animation.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    conditions: Vec<(String, Vec<String>)>,
    anim: String,
}

/// An entry's conditions: `key -> accepted values`.
type Conditions = Vec<(String, Vec<String>)>;
/// An event entry: its conditions and its torso clip (`None`: it plays nothing).
type EventEntry = (Conditions, Option<String>);

/// A `set <key> <name> = ...` define: the values it stands for, or `all NOT ...` (every value but).
#[derive(Clone, Debug, PartialEq)]
enum Define {
    Any(Vec<String>),
    AllBut(Vec<String>),
}

/// The parsed script: the movement types of the `ANIMATIONS` section (every `STATE`) in order,
/// and the events of the `EVENTS` section (an entry with no torso line plays nothing).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script {
    moves: Vec<(String, Vec<Entry>)>,
    events: Vec<(String, Vec<EventEntry>)>,
    /// The `DEATH` section: each entry's conditions and its alternatives (`both` lines; the
    /// engine picks one).
    deaths: Vec<(Conditions, Vec<String>)>,
    defines: HashMap<(String, String), Define>,
}

/// What a player is doing, as the script's conditions read it (lower-case keys and values).
pub type Context<'a> = &'a [(&'a str, &'a str)];

impl Script {
    pub fn parse(text: &str) -> Script {
        let mut s = Script::default();
        let mut section = "";
        let mut depth = 0usize;
        let mut header: Option<String> = None;
        let mut pending: Option<Vec<(String, Vec<String>)>> = None;
        let mut taken = false;
        for raw in text.lines() {
            let line = raw.split("//").next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if depth == 0 && matches!(line, "DEFINES" | "ANIMATIONS" | "EVENTS" | "DEATH") {
                section = match line {
                    "DEFINES" => "defines",
                    "ANIMATIONS" => "anims",
                    "DEATH" => "death",
                    _ => "events",
                };
                continue;
            }
            // The `DEATH` block is one braced section among the events: a name at the top level
            // after it is an event again.
            if section == "death" && depth == 0 && line != "{" {
                section = "events";
            }
            if section == "death" {
                s.death_line(line, &mut depth, &mut header);
                continue;
            }
            if section == "defines" {
                s.define(line);
                continue;
            }
            if section == "events" {
                s.event_line(line, &mut depth, &mut header);
                continue;
            }
            if section != "anims" {
                continue;
            }
            match line {
                "{" => {
                    depth += 1;
                    if depth == 3 {
                        pending = header.take().map(|h| conditions(&h));
                        taken = false;
                    }
                }
                "}" => {
                    depth = depth.saturating_sub(1);
                    if depth < 3 {
                        pending = None;
                    }
                }
                _ => match depth {
                    1 => s.moves.push((line.to_ascii_lowercase(), Vec::new())),
                    2 => header = Some(line.to_string()),
                    3 => {
                        // The first `both` (or `legs`) line is the body's clip; other lines are
                        // the torso layer, events and random alternatives.
                        let mut w = line.split_whitespace();
                        if let (Some("both" | "legs"), Some(anim), Some(c), false) = (w.next(), w.next(), pending.as_ref(), taken) {
                            if let Some((_, entries)) = s.moves.last_mut() {
                                entries.push(Entry { conditions: c.clone(), anim: anim.to_string() });
                                taken = true;
                            }
                        }
                    }
                    _ => {}
                },
            }
        }
        s
    }

    /// One line of the `EVENTS` section: `<event> { <conditions> { torso <clip> ... } ... }` (one
    /// level shallower than a movement type, which sits inside its `STATE`).
    fn event_line(&mut self, line: &str, depth: &mut usize, header: &mut Option<String>) {
        match line {
            "{" => {
                *depth += 1;
                if *depth == 2 {
                    let c = header.take().map(|h| conditions(&h)).unwrap_or_default();
                    if let Some((_, entries)) = self.events.last_mut() {
                        entries.push((c, None));
                    }
                }
            }
            "}" => *depth = depth.saturating_sub(1),
            _ => match *depth {
                0 => self.events.push((line.to_ascii_lowercase(), Vec::new())),
                1 => *header = Some(line.to_string()),
                2 => {
                    // The entry's first `torso` line is its clip (`legs` for the whole-body
                    // events: `jump`, `land`).
                    let mut w = line.split_whitespace();
                    if let (Some("torso" | "legs"), Some(anim)) = (w.next(), w.next()) {
                        if let Some((_, Some(last))) = self.events.last_mut().map(|(n, e)| (n, e.last_mut())) {
                            if last.1.is_none() {
                                last.1 = Some(anim.to_string());
                            }
                        }
                    }
                }
                _ => {}
            },
        }
    }

    /// One line of the `DEATH` section: `{ <conditions> { both <clip> ... } ... }`.
    fn death_line(&mut self, line: &str, depth: &mut usize, header: &mut Option<String>) {
        match line {
            "{" => {
                *depth += 1;
                if *depth == 2 {
                    let c = header.take().map(|h| conditions(&h)).unwrap_or_default();
                    self.deaths.push((c, Vec::new()));
                }
            }
            "}" => *depth = depth.saturating_sub(1),
            _ => match *depth {
                1 => *header = Some(line.to_string()),
                2 => {
                    let mut w = line.split_whitespace();
                    if let (Some("both"), Some(anim), Some(last)) = (w.next(), w.next(), self.deaths.last_mut()) {
                        last.1.push(anim.to_string());
                    }
                }
                _ => {}
            },
        }
    }

    /// The death clips the `DEATH` section offers in context `ctx` (`stance`, `movestatus`,
    /// `direction`, `dmgtype`, `dmgdirection`): the first entry whose every condition holds, its
    /// alternatives (empty when none holds).
    pub fn death(&self, ctx: Context) -> &[String] {
        self.deaths.iter().find(|(c, _)| c.iter().all(|(k, vals)| self.holds(k, vals, ctx))).map_or(&[], |(_, a)| a.as_slice())
    }

    fn define(&mut self, line: &str) {
        // `set <key> <name> = v AND v` or `set <key> <name> = all NOT v NOT v`.
        let Some(rest) = line.strip_prefix("set ") else { return };
        let Some((lhs, rhs)) = rest.split_once('=') else { return };
        let mut l = lhs.split_whitespace();
        let (Some(key), Some(name)) = (l.next(), l.next()) else { return };
        let words: Vec<String> = rhs.split_whitespace().map(str::to_ascii_lowercase).collect();
        let def = if words.first().is_some_and(|w| w == "all") {
            Define::AllBut(words.iter().skip(1).filter(|w| *w != "not").cloned().collect())
        } else {
            Define::Any(words.into_iter().filter(|w| w != "and").collect())
        };
        self.defines.insert((key.to_ascii_lowercase(), name.to_ascii_lowercase()), def);
    }

    /// The clip the script plays for movement type `mv` in context `ctx`: the first entry whose
    /// every condition holds. A condition on something `ctx` does not name never holds (a perk,
    /// a slope, a vehicle seat: none of them is the plain rifleman the mod's states describe).
    pub fn pick(&self, mv: &str, ctx: Context) -> Option<&str> {
        let (_, entries) = self.moves.iter().find(|(m, _)| m == mv)?;
        entries.iter().find(|e| e.conditions.iter().all(|(k, vals)| self.holds(k, vals, ctx))).map(|e| e.anim.as_str())
    }

    /// The torso clip event `ev` plays in context `ctx`: the first entry whose every condition
    /// holds, `None` when that entry plays nothing (or none holds).
    pub fn event(&self, ev: &str, ctx: Context) -> Option<&str> {
        let (_, entries) = self.events.iter().find(|(e, _)| e == ev)?;
        entries.iter().find(|(c, _)| c.iter().all(|(k, vals)| self.holds(k, vals, ctx))).and_then(|(_, a)| a.as_deref())
    }

    fn holds(&self, key: &str, values: &[String], ctx: Context) -> bool {
        let Some(&(_, have)) = ctx.iter().find(|(k, _)| *k == key) else { return false };
        values.iter().any(|v| match self.defines.get(&(key.to_string(), v.clone())) {
            Some(Define::Any(vs)) => vs.iter().any(|x| x == have),
            Some(Define::AllBut(not)) => !not.iter().any(|x| x == have),
            None => v == have || v == "all",
        })
    }
}

/// An entry header's conditions: `key v AND v, key v` (`default`: none).
fn conditions(header: &str) -> Vec<(String, Vec<String>)> {
    if header.eq_ignore_ascii_case("default") {
        return Vec::new();
    }
    header
        .split(',')
        .filter_map(|c| {
            let mut w = c.split_whitespace();
            let key = w.next()?.to_ascii_lowercase();
            let values = w.map(str::to_ascii_lowercase).filter(|v| v != "and").collect();
            Some((key, values))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = "
DEFINES
set movestatus moving = walk AND run
set playerAnimType grenadeonly = all NOT m203 NOT briefcase
ANIMATIONS
STATE COMBAT
{
	idle
	{
		weaponclass pistol AND pistol_spread
		{
			both pb_stand_alert_pistol
		}
		weaponclass grenade, playerAnimType grenadeonly, stance prone
		{
			both pb_grenade_prone_idle grenadeAnim
		}
		perk laststand
		{
			both pb_laststand_idle
		}
		stance prone // comment
		{
			both pb_prone_aim
			torso pt_prone_aim
		}
		default // two handed rifle type weapon
		{
			both pb_stand_alert
			both pb_stand_alert_v2
		}
	}
	run
	{
		movestatus moving, stance crouch
		{
			both pb_crouch_run_forward
		}
		default
		{
			both pb_combatrun_forward_loop
		}
	}
}
EVENTS
fireweapon
{
	playerAnimType dualwield, movestatus moving
	{
		// play no animations to prevent blend problems
	}
	weaponclass pistol AND pistol_spread, weapon_position ads
	{
		torso pt_stand_shoot_ads_pistol
		torso pt_other
	}
	default
	{
		torso pt_fire
	}
}
";

    #[test]
    fn the_first_entry_whose_conditions_hold_wins() {
        let s = Script::parse(SCRIPT);
        let rifle = |stance: &'static str| [("stance", stance), ("weaponclass", "rifle"), ("playeranimtype", "default"), ("movestatus", "run")];
        assert_eq!(s.pick("idle", &rifle("stand")), Some("pb_stand_alert"), "default, its first clip");
        assert_eq!(s.pick("idle", &rifle("prone")), Some("pb_prone_aim"), "a perk the context lacks never holds");
        let pistol = [("stance", "stand"), ("weaponclass", "pistol_spread")];
        assert_eq!(s.pick("idle", &pistol), Some("pb_stand_alert_pistol"), "`AND` lists alternatives");
        let grenade = [("stance", "prone"), ("weaponclass", "grenade"), ("playeranimtype", "default")];
        assert_eq!(s.pick("idle", &grenade), Some("pb_grenade_prone_idle"), "`all NOT ...` defines");
        let briefcase = [("stance", "prone"), ("weaponclass", "grenade"), ("playeranimtype", "briefcase")];
        assert_eq!(s.pick("idle", &briefcase), Some("pb_prone_aim"));
        assert_eq!(s.pick("run", &rifle("crouch")), Some("pb_crouch_run_forward"), "a define's values");
        assert_eq!(s.pick("run", &rifle("stand")), Some("pb_combatrun_forward_loop"));
        assert_eq!(s.pick("sprint", &rifle("stand")), None);
        assert_eq!(s.pick("fireweapon", &rifle("stand")), None, "events are not movement types");
    }

    #[test]
    fn events_pick_their_torso_clip_and_an_empty_entry_plays_nothing() {
        let s = Script::parse(SCRIPT);
        let ads_pistol = [("weaponclass", "pistol"), ("weapon_position", "ads"), ("movestatus", "stationary"), ("playeranimtype", "default")];
        assert_eq!(s.event("fireweapon", &ads_pistol), Some("pt_stand_shoot_ads_pistol"), "the entry's first torso line");
        let rifle = [("weaponclass", "rifle"), ("weapon_position", "hip"), ("movestatus", "stationary"), ("playeranimtype", "default")];
        assert_eq!(s.event("fireweapon", &rifle), Some("pt_fire"));
        let dw = [("weaponclass", "pistol"), ("weapon_position", "ads"), ("movestatus", "run"), ("playeranimtype", "dualwield")];
        assert_eq!(s.event("fireweapon", &dw), None, "the first match plays nothing");
        assert_eq!(s.event("reload", &rifle), None, "no such event");
        let death = Script::parse("DEATH\n{\n\tstance prone\n\t{\n\t\tboth pb_prone_death\n\t}\n\tdmgType headshot, dmgDirection front\n\t{\n\t\tboth pb_death_headshot_front\n\t\tboth pb_death_headshot_front_v2\n\t}\n\tdefault\n\t{\n\t\tboth pb_stand_death\n\t}\n}\n");
        let ctx = |stance, ty, dir| [("stance", stance), ("dmgtype", ty), ("dmgdirection", dir), ("movestatus", "stationary"), ("direction", "none")];
        assert_eq!(death.death(&ctx("prone", "headshot", "front")), ["pb_prone_death"], "the first entry that holds");
        assert_eq!(death.death(&ctx("stand", "headshot", "front")), ["pb_death_headshot_front", "pb_death_headshot_front_v2"], "its alternatives");
        assert_eq!(death.death(&ctx("stand", "melee", "back")), ["pb_stand_death"]);
        // An event after the block still parses.
        let after = Script::parse("EVENTS\nreload\n{\n\tdefault\n\t{\n\t\ttorso pt_a\n\t}\n}\nDEATH\n{\n\tdefault\n\t{\n\t\tboth pb_d\n\t}\n}\nprime_grenade\n{\n\tdefault\n\t{\n\t\ttorso pt_prime\n\t}\n}\n");
        assert_eq!(after.event("prime_grenade", &[]), Some("pt_prime"));
        assert_eq!(after.event("reload", &[]), Some("pt_a"));
        assert!(s.pick("idle", &rifle).is_some(), "the movement types still parse after the events");
    }

    #[test]
    fn anim_types_are_numbered_by_line() {
        let t = anim_types("none\r\ndefault\r\n\r\nother // a comment\nsniper\n");
        assert_eq!(t, ["none", "default", "other", "sniper"]);
    }
}
