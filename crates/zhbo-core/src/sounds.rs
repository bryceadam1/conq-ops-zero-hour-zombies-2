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
/// The Assault Shield: planted (the deploy anim's `wpn_riotshield_plant` notetrack), destroyed,
/// and struck by a round or a knife (`fly_riotshield_impact_*`; GSC plays the knife one,
/// `_turret_killstreak.gsc:910`).
pub const SHIELD_PLANT: &str = "ZHBO_BO2_ShieldPlant";
pub const SHIELD_DESTROY: &str = "ZHBO_BO2_ShieldDestroy";
pub const SHIELD_HIT_BULLET: &str = "ZHBO_BO2_ShieldHitBullet";
pub const SHIELD_HIT_KNIFE: &str = "ZHBO_BO2_ShieldHitKnife";
/// Each shield cue and the BO2 alias it is converted from.
pub const SHIELD_SOUNDS: &[(&str, &str)] = &[
    (SHIELD_PLANT, "wpn_riotshield_plant"),
    (SHIELD_DESTROY, "wpn_riotshield_destroy"),
    (SHIELD_HIT_BULLET, "fly_riotshield_impact_bullet"),
    (SHIELD_HIT_KNIFE, "fly_riotshield_impact_knife"),
];

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
/// The shot of a dual-wield weapon's left gun: its own converted BO2 sound (the `_lh` def's), else
/// the weapon's shot.
pub fn left_fire_for(def: &'static crate::weapons::WeaponDef) -> &'static str {
    if !def.left_fire_sound.is_empty() {
        def.left_fire_sound.as_str()
    } else {
        fire_for(def)
    }
}

/// A melee's swing: the melee weapon's own converted BO2 sound, else the class melee weapon's
/// (the global swing, converted from [`crate::weapons::CLASS_MELEE`]).
pub fn melee_for(def: &crate::weapons::WeaponDef) -> &str {
    if !def.melee_sound.is_empty() {
        def.melee_sound.as_str()
    } else {
        MELEE_SWING
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
/// Hold breath (`FUN_0052e3d0` notifies `playerbreathinsound` / `playerbreathoutsound` /
/// `playerbreathgaspsound`; `clientscripts/mp/_audio.csc` plays `vox_gen_sinper_hold` /
/// `_exhale` / `_gasp`).
pub const BREATH_IN: &str = "ZHBO_BO2_BreathIn";
pub const BREATH_OUT: &str = "ZHBO_BO2_BreathOut";
pub const BREATH_GASP: &str = "ZHBO_BO2_BreathGasp";

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
    breath: BreathSounds,
    pub queue: Vec<Request>,
}

/// The hold-breath sounds' state (`FUN_0052e3d0`, the client frame): how long the current breath
/// has been held (`cg+0x698bc`, `None` = not holding) and the gap left before another breath sound
/// (`cg+0x698c4`).
#[derive(Clone, Copy, Debug, Default)]
struct BreathSounds {
    held: Option<f32>,
    delay: f32,
}

impl BreathSounds {
    /// Starting a breath breathes in; letting it go past `player_breath_hold_time` gasps, earlier
    /// breathes out. In and out wait `player_breath_snd_delay` after the last one; the gasp never
    /// waits. The engine compares with the dvar alone, so a `holdbreath` perk's longer breath
    /// still gasps past 4.5 s.
    fn update(&mut self, dt: f32, holding: bool, t: &crate::tuning::Tuning) -> Option<&'static str> {
        if self.delay > 0.0 {
            self.delay -= dt;
        }
        let mut out = None;
        match (holding, self.held) {
            (false, Some(held)) => {
                if held + dt > t.player_breath_hold_time {
                    out = Some(BREATH_GASP);
                } else if self.delay <= 0.0 {
                    out = Some(BREATH_OUT);
                    self.delay = t.player_breath_snd_delay;
                }
                self.held = None;
            }
            (false, None) => {}
            (true, None) => {
                if self.delay <= 0.0 {
                    out = Some(BREATH_IN);
                    self.delay = t.player_breath_snd_delay;
                }
                self.held = Some(dt);
            }
            (true, Some(held)) => self.held = Some(held + dt),
        }
        out
    }
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

        if let Some(e) = self.breath.update(dt, p.idle.breath.holding, &p.arsenal.tuning) {
            self.play(e);
        }
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
    fn breath_sounds_in_out_and_gasp() {
        let t = crate::tuning::Tuning::default();
        let mut b = BreathSounds::default();
        assert_eq!(b.update(0.1, true, &t), Some(BREATH_IN));
        assert_eq!(b.update(0.1, true, &t), None);
        assert_eq!(b.update(0.1, false, &t), None, "out waits player_breath_snd_delay after the in");
        // Hold again after the gap, then let go early: out.
        for _ in 0..10 {
            b.update(0.1, false, &t);
        }
        assert_eq!(b.update(0.1, true, &t), Some(BREATH_IN));
        for _ in 0..15 {
            b.update(0.1, true, &t);
        }
        assert_eq!(b.update(0.1, false, &t), Some(BREATH_OUT));
        // Held past the hold time: the gasp, without waiting.
        for _ in 0..12 {
            b.update(0.1, false, &t);
        }
        b.update(0.1, true, &t);
        for _ in 0..50 {
            b.update(0.1, true, &t);
        }
        assert_eq!(b.update(0.1, false, &t), Some(BREATH_GASP));
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
