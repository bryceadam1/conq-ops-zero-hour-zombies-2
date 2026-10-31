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

/// One entry: its conditions (`key -> accepted values`; empty for `default`) and the animation.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    conditions: Vec<(String, Vec<String>)>,
    anim: String,
}

/// A `set <key> <name> = ...` define: the values it stands for, or `all NOT ...` (every value but).
#[derive(Clone, Debug, PartialEq)]
enum Define {
    Any(Vec<String>),
    AllBut(Vec<String>),
}

/// The parsed script: the movement types of the `ANIMATIONS` section (every `STATE`) in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script {
    moves: Vec<(String, Vec<Entry>)>,
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
            if depth == 0 && matches!(line, "DEFINES" | "ANIMATIONS" | "EVENTS") {
                section = if line == "DEFINES" { "defines" } else if line == "ANIMATIONS" { "anims" } else { "" };
                continue;
            }
            if section == "defines" {
                s.define(line);
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
}
