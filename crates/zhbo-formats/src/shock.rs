//! Black Ops II (T6) shellshock definitions: the `shock/<name>.shock` `RawFile` assets the GSC's
//! `shellshock( name, duration )` plays. Each is a list of `bg_shock_*` dvars, one `key "value"`
//! per line; t6mp `FUN_008bca80` copies them into the shellshock parameters (its registration,
//! `FUN_008b7bb0`, names and documents every key). Times here are seconds as written.

use crate::t6zone::Zone;

/// `bg_shock_screenType`: the full-screen layer the shock draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScreenType {
    /// `"blurred"`: the view blurs (t6mp `FUN_00510460`).
    #[default]
    Blurred,
    /// `"flashed"`: a whiteout and a frozen screengrab (t6mp `FUN_005105c0`).
    Flashed,
}

/// One shellshock: the `bg_shock_*` values of its file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shock {
    pub name: String,
    pub screen_type: ScreenType,
    /// `screenBlurBlendTime`: the blur's strength (the engine's blend amount, as seconds × 1000).
    pub blur_blend_time: f32,
    /// `screenBlurBlendFadeTime`: the last seconds over which the blur fades out.
    pub blur_fade_time: f32,
    /// `screenFlashWhiteFadeTime` / `screenFlashShotFadeTime`: how soon before the end the whiteout
    /// and the screengrab start blending out.
    pub flash_white_fade_time: f32,
    pub flash_shot_fade_time: f32,
    /// `viewKickPeriod` (s) and `viewKickRadius`: the view kick's noise period and strength.
    pub view_kick_period: f32,
    pub view_kick_radius: f32,
    /// `viewKickFadeTime`: registered, but t6mp's loader never copies it (`FUN_008bca80` writes a
    /// constant 3000 ms in its place), so the engine does not use it.
    pub view_kick_fade_time: f32,
    pub sound: bool,
    pub sound_loop: String,
    pub sound_loop_silent: String,
    /// `soundEnd` / `soundEndAbort`: the aliases played when the loop ends / is cut short.
    pub sound_end: String,
    pub sound_end_abort: String,
    pub sound_fade_in_time: f32,
    pub sound_fade_out_time: f32,
    pub sound_loop_fade_time: f32,
    pub sound_loop_end_delay: f32,
    /// `soundRoomType`, `soundDryLevel`, `soundWetLevel`: the reverb room the shock switches to.
    pub sound_room: String,
    pub sound_dry_level: f32,
    pub sound_wet_level: f32,
    /// `soundModEndDelay`: how long past the shock's end the sound modification lasts.
    pub sound_mod_end_delay: f32,
    /// `soundSnapshot`: the sound duck the shock applies (`FUN_008381e0`: "Could not find duck").
    pub sound_snapshot: String,
    /// `lookControl`: whether the look is limited (the rest of the `lookControl_*` keys).
    pub look_control: bool,
    /// Degrees per second.
    pub look_max_pitch_speed: f32,
    pub look_max_yaw_speed: f32,
    pub look_sensitivity_scale: f32,
    pub look_fade_time: f32,
    /// `movement`: the run speed's scale while shocked.
    pub movement: f32,
    pub animation: bool,
    pub visionset: String,
    pub visionset_in_time: f32,
    pub visionset_out_time: f32,
}

/// The rawfile name of shock `name` (`concussion_grenade_mp` → `shock/concussion_grenade_mp.shock`).
pub fn path(name: &str) -> String {
    format!("shock/{name}.shock")
}

/// Parse a `.shock` file's text. Unknown keys are ignored; a key the file leaves out keeps the
/// default (every MP shock file writes all of them).
pub fn parse(name: &str, text: &str) -> Shock {
    let mut s = Shock { name: name.to_string(), movement: 1.0, look_sensitivity_scale: 1.0, ..Default::default() };
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("bg_shock_") else { continue };
        let (key, value) = match rest.split_once(char::is_whitespace) {
            Some((k, v)) => (k, v.trim().trim_matches('"')),
            None => continue,
        };
        let num = || value.parse::<f32>().unwrap_or(0.0);
        let flag = || value.parse::<f32>().map(|v| v != 0.0).unwrap_or(false);
        match key {
            "screenType" => s.screen_type = if value.eq_ignore_ascii_case("flashed") { ScreenType::Flashed } else { ScreenType::Blurred },
            "screenBlurBlendTime" => s.blur_blend_time = num(),
            "screenBlurBlendFadeTime" => s.blur_fade_time = num(),
            "screenFlashWhiteFadeTime" => s.flash_white_fade_time = num(),
            "screenFlashShotFadeTime" => s.flash_shot_fade_time = num(),
            "viewKickPeriod" => s.view_kick_period = num(),
            "viewKickRadius" => s.view_kick_radius = num(),
            "viewKickFadeTime" => s.view_kick_fade_time = num(),
            "sound" => s.sound = flag(),
            "soundLoop" => s.sound_loop = value.into(),
            "soundLoopSilent" => s.sound_loop_silent = value.into(),
            "soundEnd" => s.sound_end = value.into(),
            "soundEndAbort" => s.sound_end_abort = value.into(),
            "soundFadeInTime" => s.sound_fade_in_time = num(),
            "soundFadeOutTime" => s.sound_fade_out_time = num(),
            "soundLoopFadeTime" => s.sound_loop_fade_time = num(),
            "soundLoopEndDelay" => s.sound_loop_end_delay = num(),
            "soundRoomType" => s.sound_room = value.into(),
            "soundDryLevel" => s.sound_dry_level = num(),
            "soundWetLevel" => s.sound_wet_level = num(),
            "soundModEndDelay" => s.sound_mod_end_delay = num(),
            "soundSnapshot" => s.sound_snapshot = value.into(),
            "lookControl" => s.look_control = flag(),
            "lookControl_maxpitchspeed" => s.look_max_pitch_speed = num(),
            "lookControl_maxyawspeed" => s.look_max_yaw_speed = num(),
            "lookControl_mousesensitivityscale" => s.look_sensitivity_scale = num(),
            "lookControl_fadeTime" => s.look_fade_time = num(),
            "movement" => s.movement = num(),
            "animation" => s.animation = flag(),
            "visionset_name" => s.visionset = value.into(),
            "visionset_inTime" => s.visionset_in_time = num(),
            "visionset_outTime" => s.visionset_out_time = num(),
            _ => {}
        }
    }
    s
}

/// Shock `name` from the first of `zones` holding its file (highest priority first), or `None`.
pub fn find(zones: &[&Zone], name: &str) -> Option<Shock> {
    let p = path(name);
    zones.iter().find_map(|z| crate::havok_lua::rawfile(z, &p)).map(|bytes| parse(name, &String::from_utf8_lossy(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_concussion_file() {
        let text = "bg_shock_screenType \"flashed\"\nbg_shock_screenBlurBlendTime \"0.4\"\n\
                    bg_shock_screenFlashWhiteFadeTime \"7.5\"\n\nbg_shock_lookControl \"1\"\n\
                    bg_shock_lookControl_maxyawspeed \"22\"\nbg_shock_lookControl_mousesensitivityscale \"0.1\"\n\
                    bg_shock_movement \"0.32\"\nbg_shock_soundLoop \"chr_tinitus_loop\"\nbg_shock_viewKickPeriod \".75\"\n\
                    bg_shock_soundModEndDelay \"0.1\"\nbg_shock_soundRoomType \"shock_flashbang\"\nbg_shock_soundEnd \"\"\n";
        let s = parse("concussion_grenade_mp", text);
        assert_eq!(s.screen_type, ScreenType::Flashed);
        assert_eq!((s.blur_blend_time, s.flash_white_fade_time, s.view_kick_period), (0.4, 7.5, 0.75));
        assert!(s.look_control);
        assert_eq!((s.look_max_yaw_speed, s.look_sensitivity_scale, s.movement), (22.0, 0.1, 0.32));
        assert_eq!(s.sound_loop, "chr_tinitus_loop");
        assert_eq!((s.sound_mod_end_delay, s.sound_room.as_str(), s.sound_end.as_str()), (0.1, "shock_flashbang", ""));
        assert_eq!(path("flashbang"), "shock/flashbang.shock");
    }

    /// The shocks the MP GSC plays are in the weapon pool, with the values the script relies on.
    #[test]
    #[ignore = "reads this machine's Black Ops II install"]
    fn the_installs_mp_shocks_parse() {
        let dir = std::path::PathBuf::from(std::env::var("ZHBO_BO2_DIR").expect("set ZHBO_BO2_DIR"));
        let zones: Vec<Zone> = crate::weapon::MP_POOL.iter().map(|n| crate::t6zone::load_file(&crate::t6zone::install_path(&dir, n)).unwrap()).collect();
        let refs: Vec<&Zone> = zones.iter().collect();
        for n in ["concussion_grenade_mp", "flashbang", "frag_grenade_mp", "proximity_grenade", "proximity_grenade_exit"] {
            assert!(find(&refs, n).is_some(), "{n}");
        }
        let c = find(&refs, "concussion_grenade_mp").unwrap();
        assert_eq!((c.screen_type, c.flash_white_fade_time, c.look_max_yaw_speed, c.look_sensitivity_scale, c.movement), (ScreenType::Flashed, 7.5, 22.0, 0.1, 0.32));
        let f = find(&refs, "flashbang").unwrap();
        assert_eq!((f.screen_type, f.flash_white_fade_time, f.look_control, f.movement), (ScreenType::Flashed, 3.5, false, 0.8));
    }

    #[test]
    fn junk_lines_and_missing_keys_keep_the_defaults() {
        let s = parse("x", "// a comment\nbg_shock_movement\nnot_a_key \"3\"\nbg_shock_screenType \"blurred\"");
        assert_eq!((s.movement, s.look_sensitivity_scale, s.screen_type), (1.0, 1.0, ScreenType::Blurred));
    }
}
