/*
** Conquer Ops - converter: the mod's INI rewriting: one dialect for object INIs and the weapon table.
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/
use crate::character::HIERARCHY;
use crate::generals::flash_bone;


/// Set `key = value` in the `kind name` block of an INI text: the first line with that key in the
/// block (at any depth, such as a Draw module's `Model`), or a new line before the block's `End`.
pub fn ini_set(ini: &str, kind: &str, name: &str, key: &str, value: &str) -> Result<String, String> {
    let header = format!("{kind} {name}");
    let mut out = String::new();
    let (mut inside, mut depth, mut done) = (false, 0i32, false);
    for line in ini.lines() {
        let t = line.trim();
        let word = t.split_whitespace().next().unwrap_or("");
        if !inside && t == header {
            inside = true;
            depth = 0;
        } else if inside && !done {
            if word == key && t[key.len()..].trim_start().starts_with('=') {
                let indent = &line[..line.len() - line.trim_start().len()];
                out.push_str(&format!("{indent}{key} = {value}\n"));
                done = true;
                continue;
            }
            if t == "End" {
                if depth == 0 {
                    out.push_str(&format!("  {key} = {value}\n"));
                    done = true;
                    inside = false;
                } else {
                    depth -= 1;
                }
            } else if !t.is_empty() && !t.starts_with(';') && !t.contains('=') {
                depth += 1; // a nested block (Draw module, condition state, ...)
            }
        } else if inside && t == "End" && depth == 0 {
            inside = false;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !done {
        return Err(format!("no {kind} {name} to set {key} in"));
    }
    Ok(out)
}

/// Replace `object`'s Draw module in `ini` (the base General INI) with `draw`.
pub fn replace_draw(ini: &str, object: &str, draw: &str) -> Result<String, String> {
    let header = format!("Object {object}\n");
    let start = ini.find(&header).ok_or_else(|| format!("{object} not in the General INI"))? + header.len();
    let draw_at = start + ini[start..].find("  Draw = W3DModelDraw").ok_or("no Draw module")?;
    let end_rel = ini[draw_at..].find("\n  End\n").ok_or("unterminated Draw module")?;
    let end = draw_at + end_rel + "\n  End\n".len();
    Ok(format!("{}{}{}", &ini[..draw_at], draw, &ini[end..]))
}

/// The Draw module for a General with a BO2 model. `hidden` are the sub-objects hidden until the
/// bridge shows them (the weapons not in hand, the tablet); every condition state hides them, and
/// the bridge's own show/hide wins over that (`ZhboBridge.cpp`). USER_1 raises the scorestreak
/// tablet (the field computer), USER_2 puts it away.
///
/// `sets` are the W3D clips of each third-person animation set (`generals::SetAnims`: idle, run,
/// aim, fire, reload; set 0 the rifleman's). The bridge raises set `k`'s model condition
/// (`names::ANIM_SET_CONDITIONS[k - 1]`) while a weapon of that set is in hand, so the soldier
/// stands, runs, aims, fires and reloads as BO2's player animation script has him for it.
pub(crate) fn draw_module(model: &str, hidden: &[String], guns: (usize, usize), sets: &[[String; 5]]) -> String {
    let a = |n: &str| format!("{HIERARCHY}.{n}");
    // Zero Hour reads INI lines of up to about a kilobyte; several HideSubObject lines add up.
    let hide: String = hidden.chunks(80).map(|c| format!("\x20     HideSubObject       = {}\n", c.join(" "))).collect();
    // A state: the animation, how it plays, and the sub-objects hidden in it (every state must say,
    // or the hidden weapons show).
    let state = |conditions: &str, aliases: &[String], anim: &str, mode: &str| {
        let mut st = format!("\x20   ConditionState        = {conditions}\n\x20     Animation           = {}\n\x20     AnimationMode       = {mode}\n", a(anim));
        if mode == "LOOP" {
            st.push_str("\x20     Flags               = RANDOMSTART\n");
        }
        if conditions.contains("MOVING") {
            st.push_str("\x20     ParticleSysBone     = None InfantryDustTrails\n");
        }
        st.push_str(&hide);
        st.push_str("\x20   End\n");
        for alias in aliases {
            st.push_str(&format!("\x20   AliasConditionState   = {alias}\n"));
        }
        st
    };
    let flags: Vec<&str> = (1..sets.len()).map(|k| zhbo_core::names::ANIM_SET_CONDITIONS[k - 1]).collect();
    // `conditions` with each set's flag added too: a state every set shares (dying, throwing, the
    // tablet) must still win over a set's idle, which matches the flag alone.
    let with_flags = |conditions: &[&str]| -> Vec<String> {
        conditions.iter().flat_map(|c| std::iter::once(c.to_string()).chain(flags.iter().map(move |f| format!("{c} {f}")))).collect()
    };
    // Zero Hour plays the state whose conditions match the most of the unit's, then the one with the
    // fewest others, then the first: the grenade and the animation sets come before the rifleman so
    // they win ties, and each set's states carry its flag on top of the rifleman's conditions.
    let mut d = format!(
        "  Draw = W3DModelDraw ModuleTag_01\n\
         \x20   OkToChangeModelColor = Yes\n\
         \x20   DefaultConditionState\n\
         \x20     Model               = {model}\n\
         \x20     Animation           = {idle}\n\
         \x20     AnimationMode       = LOOP\n\
         \x20     Flags               = RANDOMSTART\n\
         \x20     WeaponFireFXBone    = PRIMARY {primary}\n\
         \x20     WeaponFireFXBone    = SECONDARY {secondary}\n\
         {hide}\
         \x20   End\n",
        idle = a(&sets[0][0]),
        primary = flash_bone(guns.0),
        secondary = flash_bone(guns.1),
    );
    let dying = with_flags(&["DYING", "DYING MOVING"]);
    d.push_str(&state(&dying[0], &dying[1..], "ZHBO_PLR_DTA", "ONCE").replace(
        &format!("Animation           = {}\n", a("ZHBO_PLR_DTA")),
        &format!("Animation           = {}\n\x20     Animation           = {}\n", a("ZHBO_PLR_DTA"), a("ZHBO_PLR_DTB")),
    ));
    // A grenade: winding up and throwing are one animation.
    let throw = with_flags(&["PREATTACK_C", "FIRING_C", "BETWEEN_FIRING_SHOTS_C", "MOVING PREATTACK_C", "MOVING FIRING_C"]);
    d.push_str(&state(&throw[0], &throw[1..], "ZHBO_PLR_THR", "ONCE"));
    // Each animation set, then the rifleman (no flag). Either gun slot fires the weapon in hand.
    for (k, [idle, run, aim, fire, reload]) in sets.iter().enumerate().rev() {
        let flag = if k == 0 { String::new() } else { format!(" {}", flags[k - 1]) };
        let c = |conditions: &[&str]| -> Vec<String> { conditions.iter().map(|x| format!("{x}{flag}").trim().to_string()).collect() };
        let reloading = c(&["RELOADING_A", "RELOADING_B"]);
        d.push_str(&state(&reloading[0], &reloading[1..], reload, "ONCE"));
        let firing = c(&["FIRING_A", "FIRING_B"]);
        d.push_str(&state(&firing[0], &firing[1..], fire, "ONCE"));
        let aiming = c(&["BETWEEN_FIRING_SHOTS_A", "PREATTACK_A", "BETWEEN_FIRING_SHOTS_B", "PREATTACK_B"]);
        d.push_str(&state(&aiming[0], &aiming[1..], aim, "LOOP"));
        let moving = c(&[
            "MOVING",
            "MOVING FIRING_A",
            "MOVING BETWEEN_FIRING_SHOTS_A",
            "MOVING RELOADING_A",
            "MOVING FIRING_B",
            "MOVING BETWEEN_FIRING_SHOTS_B",
            "MOVING RELOADING_B",
        ]);
        d.push_str(&state(&moving[0], &moving[1..], run, "LOOP"));
        if k > 0 {
            d.push_str(&state(flags[k - 1], &[], idle, "LOOP"));
        }
    }
    // The field computer's tablet (the bridge raises no set's flag while it is up).
    d.push_str(&state("USER_1", &[], "ZHBO_PLR_TBU", "ONCE"));
    d.push_str(&state("USER_2", &[], "ZHBO_PLR_TBD", "ONCE"));
    d.push_str("\x20 End\n");
    d
}
