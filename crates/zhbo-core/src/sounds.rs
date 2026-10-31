//! First-person sound cues: which BO2 sound to play, and when.
//!
//! The sounds themselves are Zero Hour AudioEvents converted from the player's BO2 install
//! (`zhbo_assets::sfx`, same names). The client queues requests during its frame; the FFI layer
//! hands them to the host afterwards. Reload foley is not here: it follows the viewmodel clips'
//! notetracks (`host/zhbo/ZhboViewmodel.cpp`).

use crate::math::Vec3;
use crate::sim::PlayerSim;

pub const FIRE: &str = "ZHBO_BO2_Fire";
pub const FIRE_PISTOL: &str = "ZHBO_BO2_FirePistol";

pub const FIRE_LAUNCHER: &str = "ZHBO_BO2_FireLauncher";
pub const EXPLOSION: &str = "ZHBO_BO2_Explosion";
pub const PULL_PIN: &str = "ZHBO_BO2_PullPin";
pub const THROW: &str = "ZHBO_BO2_Throw";
pub const MELEE_SWING: &str = "ZHBO_BO2_MeleeSwing";
/// Equipment planted, and the C4 detonator.
pub const PLANT: &str = "ZHBO_BO2_Plant";
pub const DETONATE: &str = "ZHBO_BO2_Detonate";

/// The player's own gunshot for a weapon: its own converted BO2 sound, else by class.
pub fn fire_for(def: &'static crate::weapons::WeaponDef) -> &'static str {
    if !def.fire_sound.is_empty() {
        def.fire_sound.as_str()
    } else if def.class == crate::weapons::Class::Pistol {
        FIRE_PISTOL
    } else {
        FIRE
    }
}
/// The empty magazine's click for a weapon: its own converted BO2 sound, else the generic event.
pub fn dry_fire_for(def: &'static crate::weapons::WeaponDef) -> &'static str {
    if !def.empty_fire_sound.is_empty() {
        def.empty_fire_sound.as_str()
    } else {
        DRY_FIRE
    }
}
pub const DRY_FIRE: &str = "ZHBO_BO2_DryFire";
pub const HIT_MARKER: &str = "ZHBO_BO2_HitMarker";
pub const HEARTBEAT: &str = "ZHBO_BO2_Heartbeat";
pub const HURT: &str = "ZHBO_BO2_Hurt";
pub const STEP: &str = "ZHBO_BO2_Step";
pub const SPRINT_STEP: &str = "ZHBO_BO2_SprintStep";
pub const LAND: &str = "ZHBO_BO2_Land";
pub const IMPACT: &str = "ZHBO_BO2_Impact";
pub const IMPACT_FLESH: &str = "ZHBO_BO2_ImpactFlesh";

macro_rules! ui_cues {
    ($($cue:ident = $alias:literal,)*) => {
        /// A BO2 menu sound: every alias BO2's multiplayer UI Lua plays (`Engine.PlaySound` and the
        /// widgets' sound fields across `patch_mp`/`patch_ui_mp`). The converter turns each into an
        /// AudioEvent named [`UiCue::event`] from the player's own banks (`zhbo_assets::sfx`), so a
        /// screen plays the exact alias BO2 plays at the same point. Some aliases the shared widgets
        /// name (most `uin_navigation_*`, `uin_slider_*`) exist only in the single-player banks, so
        /// BO2's multiplayer menus play nothing for them; the converter leaves them out the same
        /// way, and the host logs a missing event once.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum UiCue { $($cue,)* }

        impl UiCue {
            pub const ALL: &'static [UiCue] = &[$(UiCue::$cue,)*];

            /// BO2's alias name.
            pub const fn alias(self) -> &'static str {
                match self { $(UiCue::$cue => $alias,)* }
            }

            /// The converted AudioEvent's name.
            pub const fn event(self) -> &'static str {
                match self { $(UiCue::$cue => concat!("ZHBO_UI_", $alias),)* }
            }
        }
    };
}

ui_cues! {
    CacCmnBackout = "cac_cmn_backout",
    CacCmnDeny = "cac_cmn_deny",
    CacCmnItemRemove = "cac_cmn_item_remove",
    CacEnterCac = "cac_enter_cac",
    CacEquipmentAdd = "cac_equipment_add",
    CacEquipmentRemove = "cac_equipment_remove",
    CacGlobeDraw = "cac_globe_draw",
    CacGridEquipItem = "cac_grid_equip_item",
    CacGridNav = "cac_grid_nav",
    CacKillstreakBlocked = "cac_killstreak_blocked",
    CacKillstreakNav = "cac_killstreak_nav",
    CacLoadoutEditSel = "cac_loadout_edit_sel",
    CacLoadoutEditSubmenu = "cac_loadout_edit_submenu",
    CacLoadoutNav = "cac_loadout_nav",
    CacLoadoutWildcardNav = "cac_loadout_wildcard_nav",
    CacMainEditLoadout = "cac_main_edit_loadout",
    CacMainExitCac = "cac_main_exit_cac",
    CacMainNav = "cac_main_nav",
    CacSafearea = "cac_safearea",
    CacScreenFade = "cac_screen_fade",
    CacScreenHpan = "cac_screen_hpan",
    CacSlideEquipItem = "cac_slide_equip_item",
    CacSlideNavDown = "cac_slide_nav_down",
    CacSlideNavLr = "cac_slide_nav_lr",
    CacSlideNavUp = "cac_slide_nav_up",
    CacWildcardEquip1 = "cac_wildcard_equip_1",
    CacWildcardEquip2 = "cac_wildcard_equip_2",
    CacWildcardEquip3 = "cac_wildcard_equip_3",
    CacWildcardEquip4 = "cac_wildcard_equip_4",
    CacWildcardEquip5 = "cac_wildcard_equip_5",
    CacWildcardEquip6 = "cac_wildcard_equip_6",
    CacWildcardEquip7 = "cac_wildcard_equip_7",
    CacWildcardRemove = "cac_wildcard_remove",
    UinAlertSlideout = "uin_alert_slideout",
    UinButtonPressDrc = "uin_button_press_drc",
    UinCmnBackout = "uin_cmn_backout",
    UinLauDoublexp = "uin_lau_doublexp",
    UinLoadoutNav = "uin_loadout_nav",
    UinMainEnter = "uin_main_enter",
    UinMainNav = "uin_main_nav",
    UinMainPause = "uin_main_pause",
    UinMapChosen = "uin_map_chosen",
    UinMenuLeftRight = "uin_menu_left_right",
    UinMenuTrans02 = "uin_menu_trans_02",
    UinNavigationBackout = "uin_navigation_backout",
    UinNavigationClick = "uin_navigation_click",
    UinNavigationSelectMain = "uin_navigation_select_main",
    UinNavigationSelectSubmenu = "uin_navigation_select_submenu",
    UinNavigationVote = "uin_navigation_vote",
    UinNavigationWipe = "uin_navigation_wipe",
    UinNavigationWpnAlt = "uin_navigation_wpn_alt",
    UinPanelSlideDrc = "uin_panel_slide_drc",
    UinRankDemotion = "uin_rank_demotion",
    UinRankPromotion = "uin_rank_promotion",
    UinScrollTickDrc = "uin_scroll_tick_drc",
    UinSlideNavDown = "uin_slide_nav_down",
    UinSlideNavLr = "uin_slide_nav_lr",
    UinSlideNavUp = "uin_slide_nav_up",
    UinSliderDecDrc = "uin_slider_dec_drc",
    UinSliderGrabDrc = "uin_slider_grab_drc",
    UinSliderIncDrc = "uin_slider_inc_drc",
    UinSliderReleaseDrc = "uin_slider_release_drc",
    UinToggleDrc = "uin_toggle_drc",
}

/// Distance between footsteps (Zero Hour units): a 54" step, 69" sprinting, at the one BO2-inches
/// factor (`units::inches`). Kept at the values the footstep cadence was tuned to.
const STRIDE: f32 = crate::units::inches(54.0);
const SPRINT_STRIDE: f32 = crate::units::inches(69.0);
/// Below this share of health the heartbeat plays, once per beat.
const HEARTBEAT_HEALTH: f32 = 0.35;
const HEARTBEAT_PERIOD: f32 = 0.9;
/// A landing needs at least this downward speed (units/s) to be heard.
const LAND_SPEED: f32 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Request {
    pub event: &'static str,
    /// Where it happens, for positional sounds; `None` plays it as the player's own sound.
    pub at: Option<Vec3>,
}

/// Per-player state for the cues that depend on movement and health.
#[derive(Clone, Debug, Default)]
pub struct Cues {
    stride_left: f32,
    last_pos: Option<Vec3>,
    was_airborne: bool,
    fall_speed: f32,
    heartbeat_in: f32,
    fire_was_held: bool,
    weapon: WeaponCues,
    pub queue: Vec<Request>,
}

/// The arsenal state the weapon cues compare against, from the last frame.
#[derive(Clone, Debug, Default)]
struct WeaponCues {
    seen: bool,
    raise_left: f32,
    raise_played: bool,
    drop_left: f32,
    ads: f32,
}

/// A sound field overridden to nothing (Zero Hour's `NONE`): BO2's attachment override with an
/// empty alias (the XM8's silencer silences the burst opening), distinct from "no override".
pub const NO_SOUND: &str = "NONE";

/// BO2's silent alias: an attachment that overrides a sound to `null_sound` plays nothing.
pub const NULL_SOUND_ALIAS: &str = "null_sound";

/// A def-level sound event to play, or `None` when the def names none, names [`NO_SOUND`], or
/// names BO2's silent [`NULL_SOUND_ALIAS`].
pub fn some(event: &str) -> Option<&str> {
    let silent = event.is_empty() || event == NO_SOUND || crate::names::def_sound_alias(event) == Some(NULL_SOUND_ALIAS);
    (!silent).then_some(event)
}

/// The player's own shot for this round: the def's last-round sound on the magazine's last
/// round (`fireLastSoundPlayer`, the Strela), else `normal`.
pub fn shot_for(def: &'static crate::weapons::WeaponDef, ammo_left: u16, normal: &'static str) -> &'static str {
    if ammo_left == 0 {
        some(def.fire_last_sound.as_str()).unwrap_or(normal)
    } else {
        normal
    }
}

/// A launch's shot: the def's own converted shot, else the global launcher sound.
pub fn launch_for(def: &'static crate::weapons::WeaponDef) -> &'static str {
    some(def.fire_sound.as_str()).unwrap_or(FIRE_LAUNCHER)
}

/// A throw: the offhand's own throw (`fireSoundPlayer`), else the global one.
pub fn throw_for(def: &'static crate::weapons::WeaponDef) -> &'static str {
    some(def.fire_sound.as_str()).unwrap_or(THROW)
}

/// The pull-back: the offhand's own (`pullbackSoundPlayer`), else the pin.
pub fn pullback_for(def: &'static crate::weapons::WeaponDef) -> &'static str {
    some(def.pullback_sound.as_str()).unwrap_or(PULL_PIN)
}

/// A melee swing with `def` in hand: its own swipe (`meleeSwipeSoundPlayer`), else the knife's.
pub fn melee_swing_for(def: &'static crate::weapons::WeaponDef) -> &'static str {
    some(def.melee_swipe_sound.as_str()).unwrap_or(MELEE_SWING)
}

/// A blast: the projectile def's own (`projExplosionSound`), else the generic one.
pub fn explosion_for(sound: &'static str) -> &'static str {
    some(sound).unwrap_or(EXPLOSION)
}

impl Cues {
    pub fn play(&mut self, event: &'static str) {
        self.queue.push(Request { event, at: None });
    }

    pub fn play_at(&mut self, event: &'static str, at: Vec3) {
        self.queue.push(Request { event, at: Some(at) });
    }

    /// Forget movement history (first person starts, or the General changed).
    pub fn reset(&mut self) {
        let queue = std::mem::take(&mut self.queue);
        *self = Cues { queue, ..Default::default() };
    }

    /// Footsteps, landing, heartbeat and dry fire for this frame.
    pub fn update(&mut self, dt: f32, p: &PlayerSim, fire_held: bool) {
        let pos = p.body.pos;
        if p.body.on_ground {
            if self.was_airborne && self.fall_speed > LAND_SPEED {
                self.play(LAND);
                self.stride_left = STRIDE * 0.5;
            }
            if let Some(last) = self.last_pos {
                let moved = ((pos.x - last.x).powi(2) + (pos.y - last.y).powi(2)).sqrt();
                // Teleports and respawns are not steps.
                if moved < 50.0 {
                    self.stride_left -= moved;
                }
            }
            if self.stride_left <= 0.0 {
                let sprint = p.body.sprinting;
                self.play(if sprint { SPRINT_STEP } else { STEP });
                self.stride_left += if sprint { SPRINT_STRIDE } else { STRIDE };
                if self.stride_left < 0.0 {
                    self.stride_left = STRIDE;
                }
            }
            self.was_airborne = false;
            self.fall_speed = 0.0;
        } else {
            self.was_airborne = true;
            self.fall_speed = self.fall_speed.max(-p.body.vel.z);
        }
        self.last_pos = Some(pos);

        if p.max_health > 0.0 && p.health / p.max_health < HEARTBEAT_HEALTH && p.health > 0.0 {
            self.heartbeat_in -= dt;
            if self.heartbeat_in <= 0.0 {
                self.play(HEARTBEAT);
                self.heartbeat_in = HEARTBEAT_PERIOD;
            }
        } else {
            self.heartbeat_in = 0.0;
        }

        let w = p.arsenal.current();
        if fire_held && !self.fire_was_held && w.ammo == 0 && !w.is_reloading() {
            self.play(dry_fire_for(w.def));
        }
        self.fire_was_held = fire_held;
        self.weapon_moments(p);
    }

    /// BO2's def-level weapon sounds at their moments, read off the arsenal's state changes: the
    /// putaway as a drop begins (the weapon going away), the raise — or the first raise — once
    /// the drop is done and the raise runs, and the zoom as aiming starts. An alt toggle plays
    /// `altSwitchSoundPlayer` as it begins and its raise plays nothing: BO2's weapon change sends
    /// EV_WEAPON_ALT instead of EV_PUTAWAY_WEAPON for an alt pair (`FUN_008db500`) and
    /// EV_RAISE_PICKUP_WEAPON instead of EV_RAISE_WEAPON (`FUN_008df320`); the client plays the
    /// held def's `altSwitchSound(Player)` for the first and nothing for the second
    /// (`FUN_004c1590`). The held def is the one up after the toggle's command: with no alt drop
    /// (`iAltDropTime` 0, every launcher pair) the change finishes in that same pmove
    /// (`FUN_008e4da0`), so it is the incoming mode — the mod's toggle has no drop phase either.
    fn weapon_moments(&mut self, p: &PlayerSim) {
        use crate::weapons::arsenal::RaiseKind;
        let a = &p.arsenal;
        let prev = std::mem::take(&mut self.weapon);
        let mut now = WeaponCues { seen: true, raise_left: a.raise_left, raise_played: prev.raise_played, drop_left: a.drop_left, ads: a.ads };
        if a.drop_left > prev.drop_left + 1e-6 {
            if let Some(e) = a.weapons.get(a.previous).and_then(|w| some(w.def.putaway_sound.as_str())) {
                self.play(e);
            }
        }
        // A new raise (the timer jumped up, or the first frame seen with one running) re-arms it.
        if !prev.seen || a.raise_left > prev.raise_left + 1e-6 {
            now.raise_played = false;
            if a.raise_kind == RaiseKind::Alt {
                if let Some(e) = some(a.current().def.alt_switch_sound.as_str()) {
                    self.play(e);
                }
            }
        }
        if a.raise_left > 0.0 && a.drop_left <= 0.0 && !now.raise_played {
            now.raise_played = true;
            let def = a.current().def;
            let e = match a.raise_kind {
                RaiseKind::First => some(def.first_raise_sound.as_str()),
                RaiseKind::Normal | RaiseKind::Quick => some(def.raise_sound.as_str()),
                RaiseKind::Alt => None,
            };
            if let Some(e) = e {
                self.play(e);
            }
        }
        if prev.ads <= 0.0 && a.ads > 0.0 {
            if let Some(e) = some(a.current().def.ads_zoom_sound.as_str()) {
                self.play(e);
            }
        }
        self.weapon = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(c: &mut Cues) -> Vec<&'static str> {
        c.queue.drain(..).map(|r| r.event).collect()
    }

    #[test]
    fn dry_fire_follows_the_def() {
        let def: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef { empty_fire_sound: "ZHBO_BO2_DryFireGlRifle".into(), ..Default::default() }));
        assert_eq!(dry_fire_for(def), "ZHBO_BO2_DryFireGlRifle");
        let plain: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef::default()));
        assert_eq!(dry_fire_for(plain), DRY_FIRE);
    }

    #[test]
    fn raise_putaway_and_zoom_play_the_defs_sounds_at_their_moments() {
        use crate::weapons::arsenal::RaiseKind;
        let def: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef {
            raise_sound: "ZHBO_BO2_Snd_fly_generic_raise_plr".into(),
            first_raise_sound: "ZHBO_BO2_Snd_fly_generic_first_raise_plr".into(),
            putaway_sound: "ZHBO_BO2_Snd_fly_generic_down_plr".into(),
            ads_zoom_sound: "ZHBO_BO2_Snd_fly_scope_zoom".into(),
            ..Default::default()
        }));
        let mut p = PlayerSim::spawn_for_tests();
        for w in p.arsenal.weapons.iter_mut() {
            w.def = def;
        }
        let mut c = Cues::default();
        // Only the weapon's own cues (the first frame also takes a footstep).
        let weapon = |c: &mut Cues| -> Vec<&'static str> { events(c).into_iter().filter(|e| e.starts_with(crate::names::DEF_SOUND_PREFIX)).collect() };
        let a = &mut p.arsenal;
        (a.raise_left, a.raise_kind, a.drop_left, a.ads) = (0.5, RaiseKind::First, 0.0, 0.0);
        c.update(0.016, &p, false);
        assert_eq!(weapon(&mut c), vec!["ZHBO_BO2_Snd_fly_generic_first_raise_plr"], "the spawn's first raise");
        p.arsenal.raise_left = 0.4;
        c.update(0.016, &p, false);
        assert!(weapon(&mut c).is_empty(), "once per raise");
        // A swap: the drop starts (putaway), then the raise once the drop is done.
        let a = &mut p.arsenal;
        (a.drop_left, a.raise_left, a.raise_kind) = (0.3, 0.6, RaiseKind::Normal);
        c.update(0.016, &p, false);
        assert_eq!(weapon(&mut c), vec!["ZHBO_BO2_Snd_fly_generic_down_plr"]);
        p.arsenal.drop_left = 0.0;
        c.update(0.016, &p, false);
        assert_eq!(weapon(&mut c), vec!["ZHBO_BO2_Snd_fly_generic_raise_plr"]);
        // An alt toggle plays the incoming mode's alt-switch sound once; its raise is silent.
        let alt: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef {
            raise_sound: "ZHBO_BO2_Snd_alt_raise".into(),
            alt_switch_sound: "ZHBO_BO2_Snd_alt_mode_switch".into(),
            ..def.clone()
        }));
        let def_switch: &'static crate::weapons::WeaponDef = Box::leak(Box::new(crate::weapons::WeaponDef { alt_switch_sound: "ZHBO_BO2_Snd_main_mode_switch".into(), ..def.clone() }));
        p.arsenal.weapons[p.arsenal.active].def = def_switch;
        c.update(0.016, &p, false);
        let a = &mut p.arsenal;
        let active = a.active;
        a.weapons[active].def = alt;
        (a.raise_left, a.raise_kind) = (0.7, RaiseKind::Alt);
        c.update(0.016, &p, false);
        assert_eq!(weapon(&mut c), vec!["ZHBO_BO2_Snd_alt_mode_switch"], "the incoming mode's alt switch, no raise");
        p.arsenal.raise_left = 0.5;
        c.update(0.016, &p, false);
        assert!(weapon(&mut c).is_empty(), "once per toggle");
        // And back: the main mode's own alt-switch sound.
        let a = &mut p.arsenal;
        a.weapons[active].def = def_switch;
        a.raise_left = 0.7;
        c.update(0.016, &p, false);
        assert_eq!(weapon(&mut c), vec!["ZHBO_BO2_Snd_main_mode_switch"]);
        p.arsenal.raise_left = 0.0;
        p.arsenal.ads = 0.2;
        c.update(0.016, &p, false);
        assert_eq!(weapon(&mut c), vec!["ZHBO_BO2_Snd_fly_scope_zoom"], "zoom as aiming starts");
        p.arsenal.ads = 0.6;
        c.update(0.016, &p, false);
        assert!(weapon(&mut c).is_empty());
    }

    #[test]
    fn the_defs_own_events_replace_the_globals_when_present() {
        let leak = |d: crate::weapons::WeaponDef| -> &'static crate::weapons::WeaponDef { Box::leak(Box::new(d)) };
        let plain = leak(Default::default());
        assert_eq!((launch_for(plain), throw_for(plain), pullback_for(plain), melee_swing_for(plain)), (FIRE_LAUNCHER, THROW, PULL_PIN, MELEE_SWING));
        assert_eq!(explosion_for(""), EXPLOSION);
        let strela = leak(crate::weapons::WeaponDef { fire_sound: "A".into(), fire_last_sound: "LAST".into(), ..Default::default() });
        assert_eq!(launch_for(strela), "A");
        assert_eq!(shot_for(strela, 0, "A"), "LAST", "the last round's own shot");
        assert_eq!(shot_for(strela, 1, "A"), "A");
        assert_eq!(shot_for(plain, 0, FIRE), FIRE);
        let semtex = leak(crate::weapons::WeaponDef { fire_sound: "T".into(), pullback_sound: "P".into(), melee_swipe_sound: "S".into(), ..Default::default() });
        assert_eq!((throw_for(semtex), pullback_for(semtex), melee_swing_for(semtex)), ("T", "P", "S"));
        assert_eq!(explosion_for("B"), "B");
        // Overridden to nothing, or to BO2's silent alias: no cue.
        assert_eq!(some(NO_SOUND), None);
        assert_eq!(some(&crate::names::def_sound_event(NULL_SOUND_ALIAS)), None);
        assert_eq!(some("ZHBO_BO2_Snd_fly_scope_zoom"), Some("ZHBO_BO2_Snd_fly_scope_zoom"));
    }

    #[test]
    fn footsteps_follow_distance() {
        let mut p = PlayerSim::spawn_for_tests();
        let mut c = Cues::default();
        c.update(0.016, &p, false);
        events(&mut c);
        let mut steps = 0;
        for _ in 0..100 {
            p.body.pos.x += 1.0; // 100 units walked
            c.update(0.016, &p, false);
            steps += events(&mut c).iter().filter(|e| **e == STEP).count();
        }
        assert!((10..=12).contains(&steps), "{steps} steps over 100 units");
        // Sprinting takes longer strides.
        p.body.sprinting = true;
        let mut sprint_steps = 0;
        for _ in 0..100 {
            p.body.pos.x += 1.0;
            c.update(0.016, &p, false);
            sprint_steps += events(&mut c).iter().filter(|e| **e == SPRINT_STEP).count();
        }
        assert!(sprint_steps < steps, "{sprint_steps} sprint steps");
    }

    #[test]
    fn landing_heartbeat_and_dry_fire() {
        let mut p = PlayerSim::spawn_for_tests();
        let mut c = Cues::default();
        p.body.on_ground = false;
        p.body.vel.z = -60.0;
        c.update(0.016, &p, false);
        p.body.on_ground = true;
        c.update(0.016, &p, false);
        assert!(events(&mut c).contains(&LAND));

        p.health = p.max_health * 0.2;
        c.update(0.016, &p, false);
        assert!(events(&mut c).contains(&HEARTBEAT));
        c.update(0.5, &p, false);
        assert!(!events(&mut c).contains(&HEARTBEAT), "one beat per period");

        let w = p.arsenal.active;
        p.arsenal.weapons[w].ammo = 0;
        c.update(0.016, &p, true);
        assert!(events(&mut c).contains(&DRY_FIRE));
        c.update(0.016, &p, true);
        assert!(!events(&mut c).contains(&DRY_FIRE), "only on the press");
    }
}
