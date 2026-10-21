/*
** Conquer Ops - converter: the third-person body's MP animations (Track R7).
**
** BO2's MP bodies share one player skeleton; the body clips are BO2's `pb_*` body animations in
** `common_mp` (the same zone the third-person W3D conversion reads: `generals::ANIMS`). The mod's
** native body plays one clip per movement state (`zhbo_native::thirdperson::state`); this module
** resolves each state to a BO2 animation, by its known names first and then by scanning the zone
** for a plausible one, because the exact spelling differs between installs for the stances the W3D
** path never needed (walk, crouch, prone).
**
** This program is free software: you can redistribute it and/or modify it under the terms of the
** GNU General Public License as published by the Free Software Foundation, either version 3 of the
** License, or (at your option) any later version.
*/
use zhbo_formats::t6zone::Zone;
use zhbo_formats::xanim::{self, Anim};
use zhbo_native::thirdperson::state;

/// One state's clip.
pub(crate) struct Clip {
    pub state: u16,
    pub anim: Anim,
}

/// The names the W3D path already proves exist (`generals::ANIMS`), then the stances it never
/// converted. The first name in a state's list that the install has is the one used.
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
    (state::STAND_IDLE_PISTOL, &["pb_stand_alert_pistol", "pb_stand_idle_pistol"]),
    (state::STAND_ADS_PISTOL, &["pb_stand_ads_pistol"]),
];

/// Every body state's clip from `common`, in [`CANDIDATES`] order, and the states that resolved to
/// nothing (the body then falls back to a nearer stance at run time,
/// `zhbo_native::thirdperson::state::fallbacks`).
pub(crate) fn clips(common: &Zone) -> (Vec<Clip>, Vec<u16>) {
    let names: Vec<String> = common
        .assets
        .iter()
        .filter(|a| a.kind == zhbo_formats::fastfile::ASSET_XANIM)
        .filter_map(|a| common.asset_name(a))
        .map(|n| n.trim_start_matches(',').to_string())
        .collect();
    let mut out = Vec::new();
    let mut missing = Vec::new();
    for &(st, candidates) in CANDIDATES {
        let name = candidates
            .iter()
            .find(|c| names.iter().any(|n| n == *c))
            .map(|c| (*c).to_string())
            .or_else(|| names.iter().find(|n| plausible(st, n)).cloned());
        match name.and_then(|n| xanim::find(common, &n).ok().map(|a| (n, a))) {
            Some((_, anim)) => out.push(Clip { state: st, anim }),
            None => missing.push(st),
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
    // Not a body-locomotion clip at all (deaths, jumps, turns, the torso-only `pt_*` clips...).
    let locomotion = ![
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
        state::STAND_IDLE_PISTOL => has("stand") && (has("alert") || has("idle")) && pistol && !ads && locomotion,
        state::STAND_ADS_PISTOL => has("stand") && ads && pistol && locomotion,
        _ => false,
    }
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
        assert!(plausible(state::STAND_ADS_PISTOL, "pb_stand_ads_pistol"));
        // Other stances and transitions are refused.
        assert!(!plausible(state::STAND_WALK, "pb_walk_backward_loop"), "backward is not forward");
        assert!(!plausible(state::STAND_RUN, "pb_sprint_forward_loop"), "sprint has its own state");
        assert!(!plausible(state::STAND_IDLE, "pb_stand_ads"), "ADS has its own state");
        assert!(!plausible(state::STAND_IDLE, "pb_stand_alert_pistol"), "the pistol has its own state");
        assert!(!plausible(state::CROUCH_ADS, "pb_crouch_alert"));
        assert!(!plausible(state::PRONE_IDLE, "pb_prone_getup"), "a transition is not the idle");
        assert!(!plausible(state::STAND_RUN, "pt_rifle_fire_ads"), "a torso clip is not locomotion");
    }

    #[test]
    fn every_state_has_a_candidate_and_a_scan_rule() {
        for st in 1..=state::COUNT {
            assert!(CANDIDATES.iter().any(|c| c.0 == st), "state {st} has no candidate list");
        }
        assert_eq!(CANDIDATES.len(), state::COUNT as usize, "one candidate list per state");
    }
}
