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
/// Another player's footfalls, heard at him: BO2's third-person aliases
/// (`clientscripts/mp/_footsteps.csc` builds `fly_step_run_npc_<surface>`; the sprint's
/// `fly_step_sprint_npc_<surface>`), positional for everyone ([`RemoteSteps`]).
pub const STEP_NPC: &str = "ZHBO_BO2_StepNpc";
pub const SPRINT_STEP_NPC: &str = "ZHBO_BO2_SprintStepNpc";
/// Another player's landing (`_footsteps.csc` `playerland`: `fly_land_npc_<surface>`).
pub const LAND_NPC: &str = "ZHBO_BO2_LandNpc";
/// A player's voice when he is hurt and when he dies (`_battlechatter_mp.gsc` `painvox`,
/// `deathvox`: `level.teamprefix[team] + bcvoicenumber + "_pain_small"` / `"_pain_death"`, played
/// on his `J_Head` for everyone). One event per faction's voice bank (the faction letters of
/// `zhbo_assets::generals::FACTIONS`) and voice number (`bcVoiceNumber`, `randomintrange(0, 3)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    Pain,
    Death,
}

/// The faction letters whose voices convert, in [`VOICE_EVENTS`] order.
pub const VOICE_FACTIONS: [u8; 3] = *b"UCG";
/// BO2's voice numbers per team (`randomintrange(0, 3)`).
pub const VOICES: usize = 3;
/// `[voice][faction][number]`: the converted events ([`voice_event`]).
pub const VOICE_EVENTS: [[[&str; VOICES]; 3]; 2] = [
    [
        ["ZHBO_BO2_PainSmall_U0", "ZHBO_BO2_PainSmall_U1", "ZHBO_BO2_PainSmall_U2"],
        ["ZHBO_BO2_PainSmall_C0", "ZHBO_BO2_PainSmall_C1", "ZHBO_BO2_PainSmall_C2"],
        ["ZHBO_BO2_PainSmall_G0", "ZHBO_BO2_PainSmall_G1", "ZHBO_BO2_PainSmall_G2"],
    ],
    [
        ["ZHBO_BO2_PainDeath_U0", "ZHBO_BO2_PainDeath_U1", "ZHBO_BO2_PainDeath_U2"],
        ["ZHBO_BO2_PainDeath_C0", "ZHBO_BO2_PainDeath_C1", "ZHBO_BO2_PainDeath_C2"],
        ["ZHBO_BO2_PainDeath_G0", "ZHBO_BO2_PainDeath_G1", "ZHBO_BO2_PainDeath_G2"],
    ],
];

/// The voice event of seat `seat`'s General of faction `faction` (his letter; the first faction's
/// when unknown). BO2 draws a player's voice number at random once per team join; here the seat
/// picks it (`seat % 3`), the same on every machine and for the whole match.
pub fn voice_event(v: Voice, faction: Option<u8>, seat: usize) -> &'static str {
    let f = faction.and_then(|f| VOICE_FACTIONS.iter().position(|&x| x == f)).unwrap_or(0);
    VOICE_EVENTS[v as usize][f][seat % VOICES]
}

/// BO2's battle chatter lines the mod plays (`_battlechatter_mp.gsc`): the alias after
/// `<teamprefix><n>_` (`level.bcsounds[partone] + "_" + parttwo`). `reloadtracking` says
/// `reloading_gen`; `grenadetracking` an offhand's own line.
pub const CHATTER_LINES: [&str; 13] = [
    "reloading_gen",
    "attack_grenade",
    "attack_c4",
    "attack_emp",
    "attack_claymore",
    "attack_flash",
    "attack_sticky",
    "attack_gas",
    "attack_smoke",
    "attack_hatchet",
    "attack_concussion",
    "attack_scrambler",
    "attack_tactical",
];

/// `grenadetracking`'s `weaponname` → line (`_battlechatter_mp.gsc:589-700`), and whether its odds
/// are the trophy dvar's (the Bouncing Betty) rather than the toss dvar's.
pub const THROW_CHATTER: &[(&str, &str, bool)] = &[
    ("frag_grenade_mp", "attack_grenade", false),
    ("satchel_charge_mp", "attack_c4", false),
    ("emp_grenade_mp", "attack_emp", false),
    ("claymore_mp", "attack_claymore", false),
    ("flash_grenade_mp", "attack_flash", false),
    ("sticky_grenade_mp", "attack_sticky", false),
    ("tabun_gas_mp", "attack_gas", false),
    ("willy_pete_mp", "attack_smoke", false),
    ("hatchet_mp", "attack_hatchet", false),
    ("proximity_grenade_mp", "attack_hatchet", false),
    ("concussion_grenade_mp", "attack_concussion", false),
    ("scrambler_mp", "attack_scrambler", false),
    ("tactical_insertion_mp", "attack_tactical", false),
    ("bouncingbetty_mp", "attack_c4", true),
    ("sensor_grenade_mp", "attack_hatchet", false),
];
/// `checkweaponreload`: the weapons whose reload says nothing.
pub const RELOAD_SILENT: [&str; 5] = ["crossbow_mp", "fhj18_mp", "judge_mp", "smaw_mp", "usrpg_mp"];
/// The odds dvars (`_battlechatter_mp.gsc:64-73`): a line plays when `randomintrange(0, 100)` is
/// at least the dvar — `bcmp_weapon_reload_probability` 60, `bcmp_toss_grenade_probability` 20,
/// `bcmp_toss_trophy_probability` 80.
pub const CHATTER_RELOAD: u32 = 60;
pub const CHATTER_TOSS: u32 = 20;
pub const CHATTER_TROPHY: u32 = 80;

/// The line `kind` (a reload, a throw, a set-down) with `def` says, and the threshold its roll
/// must reach.
pub fn chatter_for(kind: &crate::sim::EventKind, def: &crate::weapons::WeaponDef) -> Option<(&'static str, u32)> {
    use crate::sim::EventKind;
    let base = def.id.split(['+', '~']).next().unwrap_or("");
    match kind {
        EventKind::ReloadStarted => (!RELOAD_SILENT.contains(&base)).then_some(("reloading_gen", CHATTER_RELOAD)),
        EventKind::Thrown { .. } | EventKind::Planted { .. } => THROW_CHATTER.iter().find(|t| t.0 == base).map(|t| (t.1, if t.2 { CHATTER_TROPHY } else { CHATTER_TOSS })),
        _ => None,
    }
}

/// Seat `seat`'s `n`-th chatter roll, 0..100 (BO2's `randomintrange(0, 100)`): a hash of the two,
/// so every machine, seeing the same events, rolls the same.
pub fn chatter_roll(seat: usize, n: u32) -> u32 {
    let mut h = (seat as u64) << 32 | n as u64;
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    (h % 100) as u32
}

/// The converted event of chatter `line` in faction `faction`'s voice `n`
/// (`ZHBO_BO2_Bc_<F><n>_<line>`).
pub fn chatter_event_name(faction: u8, n: usize, line: &str) -> String {
    format!("ZHBO_BO2_Bc_{}{n}_{line}", faction as char)
}

/// Seat `seat`'s General's (faction `faction`, the first one's when unknown) event for `line`.
pub fn chatter_event(faction: Option<u8>, seat: usize, line: &str) -> Option<&'static str> {
    static EVENTS: std::sync::OnceLock<Vec<(u8, usize, &'static str, &'static str)>> = std::sync::OnceLock::new();
    let all = EVENTS.get_or_init(|| {
        VOICE_FACTIONS
            .iter()
            .flat_map(|&f| (0..VOICES).flat_map(move |n| CHATTER_LINES.iter().map(move |&l| (f, n, l, &*Box::leak(chatter_event_name(f, n, l).into_boxed_str())))))
            .collect()
    });
    let f = faction.filter(|f| VOICE_FACTIONS.contains(f)).unwrap_or(VOICE_FACTIONS[0]);
    all.iter().find(|e| e.0 == f && e.1 == seat % VOICES && e.2 == line).map(|e| e.3)
}

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
    /// A multiplier on the event's own volume (1 for every cue but a third-person shot's layers,
    /// which carry BO2's distance curve at the listener: `soundlayers`).
    pub gain: f32,
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
    weapon: WeaponCues,
    pub queue: Vec<Request>,
    /// Looping events' gains this frame (`ClientHost::sound_loop`; 0 stops one): a shellshock's
    /// ringing (`shellshock::Presented`).
    pub loops: Vec<(&'static str, f32)>,
}

/// Other players' footsteps: each first-person General but the presented one puts a foot down
/// every stride he walks on the ground, at the stride the own steps use, heard at him
/// ([`STEP_NPC`]/[`SPRINT_STEP_NPC`]). BO2 plays every other player's steps this way
/// (`_footsteps.csc`'s `firstperson = 0` aliases), and his jump and landing too: `playerjump`
/// plays the step alias, `playerland` `fly_land_npc_<surface>` ([`LAND_NPC`]). A General with Dead
/// Silence (`specialty_quieter`) makes none. Presentation only: read off the sim's records.
#[derive(Clone, Debug, Default)]
pub struct RemoteSteps {
    seats: [Option<StepSeat>; crate::sim::MAX_PLAYERS],
}

/// One seat's footfall state: his General, where his body was last frame, the stride left to the
/// next step, and whether he was in the air (with his fastest fall).
#[derive(Clone, Copy, Debug)]
struct StepSeat {
    general: u32,
    last: Vec3,
    left: f32,
    airborne: bool,
    fall: f32,
}

impl RemoteSteps {
    /// This frame's footfalls of every seat's first-person General but `presented`'s.
    pub fn update(&mut self, players: &[Option<PlayerSim>], presented: i32, cues: &mut Cues) {
        for (seat, slot) in self.seats.iter_mut().enumerate() {
            let p = players.get(seat).and_then(|p| p.as_ref()).filter(|p| seat as i32 != presented && p.is_first_person());
            let Some(p) = p else {
                *slot = None;
                continue;
            };
            let pos = p.body.pos;
            let quiet = p.arsenal.has_specialty(crate::weapons::specialty::Specialty::Quieter);
            let (stride, event) = if p.body.sprinting { (SPRINT_STRIDE, SPRINT_STEP_NPC) } else { (STRIDE, STEP_NPC) };
            let mut s = match *slot {
                Some(s) if s.general == p.general => s,
                // A new General (or one just seen): his first step half a stride on.
                _ => StepSeat { general: p.general, last: pos, left: STRIDE * 0.5, airborne: !p.body.on_ground, fall: 0.0 },
            };
            let moved = (pos - s.last).length_xy();
            if p.body.on_ground {
                // He landed: BO2's landing (`playerland`), when he fell fast enough to be heard.
                if s.airborne && s.fall > LAND_SPEED && !quiet {
                    cues.play_at(LAND_NPC, pos);
                    s.left = STRIDE * 0.5;
                }
                // Teleports and respawns are not steps.
                if moved < 50.0 && !s.airborne {
                    s.left -= moved;
                }
                s.airborne = false;
                s.fall = 0.0;
            } else {
                // He left the ground going up: BO2's jump plays the step alias (`playerjump`).
                if !s.airborne && p.body.vel.z > 0.0 && !quiet {
                    cues.play_at(STEP_NPC, pos);
                }
                s.airborne = true;
                s.fall = s.fall.max(-p.body.vel.z);
            }
            if s.left <= 0.0 {
                if !quiet {
                    cues.play_at(event, pos);
                }
                s.left += stride;
                if s.left < 0.0 {
                    s.left = STRIDE;
                }
            }
            s.last = pos;
            *slot = Some(s);
        }
    }
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


/// The arsenal state the weapon cues compare against, from the last frame.
#[derive(Clone, Debug, Default)]
pub struct WeaponCues {
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

/// A set-down item going down (Claymore, Bouncing Betty, Trophy System, Tactical Insertion): the
/// item's own `fireSoundPlayer` — what BO2 plays when any offhand fires — else the global plant.
pub fn plant_for(def: &'static crate::weapons::WeaponDef) -> &'static str {
    some(def.fire_sound.as_str()).unwrap_or(PLANT)
}

/// What everyone but the thrower hears when an offhand is thrown or set down: its third-person
/// `fireSound` (`WeaponDef::npc_fire_sound`); nothing when it has none.
pub fn remote_offhand(def: &'static crate::weapons::WeaponDef) -> Option<&'static str> {
    some(def.npc_fire_sound.as_str())
}

/// The pull-back: the offhand's own (`pullbackSoundPlayer`), else nothing. BO2 names one for
/// three offhands only (the Semtex's charge, the Shock Charge's arm, the Sensor Grenade's twist);
/// the frag and the rest play none, so there is no global stand-in (the converter could never
/// build one: it came from the default lethal's own empty alias).
pub fn pullback_for(def: &'static crate::weapons::WeaponDef) -> Option<&'static str> {
    some(def.pullback_sound.as_str())
}

/// A melee swing with `def` in hand: its own swipe (`meleeSwipeSoundPlayer`), else the knife's.
pub fn melee_swing_for(def: &'static crate::weapons::WeaponDef) -> &'static str {
    some(def.melee_swipe_sound.as_str()).unwrap_or(MELEE_SWING)
}

/// A blast's cues, in BO2's order (`FUN_004c0ac0`): the impact table's sound for the def's
/// `impactType` on `surface` (`<family>_<surface>`, `impacts::sound_name`; nothing when the install
/// has no such alias, as in BO2), then the def's own `projExplosionSound` on top. The generic blast
/// only when the type has no converted sounds at all and the def none (an unconverted archive, a
/// mod-only def).
pub fn explosion_cues(sound: &'static str, impact_type: i32, surface: crate::impacts::Surface) -> [Option<&'static str>; 2] {
    let table = crate::impacts::sound_name(impact_type, surface).and_then(some);
    let own = some(sound);
    if table.is_none() && own.is_none() && !crate::impacts::type_has_sounds(impact_type) {
        return [Some(EXPLOSION), None];
    }
    [table, own]
}

/// A dud's cues, in BO2's order (t6mp `FUN_004c1590` event 0x51): the large bullet's hit alias for
/// `surface` (`prj_bullet_impact_large_<surface>`, `impacts::DUD_HIT_SOUND_TYPE`; nothing where
/// the install has none), then the def's own `projDudSound`.
pub fn dud_cues(sound: &'static str, surface: crate::impacts::Surface) -> [Option<&'static str>; 2] {
    [crate::impacts::sound_name(crate::impacts::DUD_HIT_SOUND_TYPE, surface).and_then(some), some(sound)]
}

/// Where a penetrating round leaves a solid: the type's exit family alias for the surface (BO2's
/// flag 4, `prj_bullet_impact_<size>_exit_<surface>`), nothing without a cell or on `NONE` (no
/// fixed fallback: an unconverted archive's exits are silent).
pub fn exit_cue(impact_type: i32, surface: crate::impacts::Surface) -> Option<&'static str> {
    crate::impacts::exit_sound_name(impact_type, surface).and_then(some)
}

/// A pellet's hit sound: the impact table's for the gun's `impactType` on `surface` (BO2's
/// `prj_bullet_impact_<size>_<surface>`, `FUN_00533210`), nothing where BO2 has no alias (`NONE`
/// rows: its hash miss is silent). The fixed flesh/hard cue only when the type has no converted
/// sound cells at all (an unconverted archive).
pub fn impact_cue(impact_type: i32, surface: crate::impacts::Surface) -> Option<&'static str> {
    if !crate::impacts::type_has_sounds(impact_type) {
        return Some(if surface == crate::impacts::Surface::Flesh { IMPACT_FLESH } else { IMPACT });
    }
    crate::impacts::sound_name(impact_type, surface).and_then(some)
}

impl Cues {
    pub fn play(&mut self, event: &'static str) {
        self.queue.push(Request { event, at: None, gain: 1.0 });
    }

    pub fn play_at(&mut self, event: &'static str, at: Vec3) {
        self.queue.push(Request { event, at: Some(at), gain: 1.0 });
    }

    /// A positional cue at `gain` × its event's volume.
    pub fn play_at_gain(&mut self, event: &'static str, at: Vec3, gain: f32) {
        self.queue.push(Request { event, at: Some(at), gain });
    }

    /// Another player's shot through FX weapon `fx` from `at`, heard from `listener` (`None` when
    /// the local player has no first-person body, e.g. overhead): BO2's chain layer by layer, each
    /// at its distance curve's gain (`soundlayers`), else the converted close mix with Zero Hour's
    /// falloff. Nothing for an FX weapon without converted layers (it keeps its own `FireSound`).
    pub fn third_person_shot(&mut self, fx: &str, at: Vec3, listener: Option<Vec3>) {
        match crate::soundlayers::shot(fx, listener.map(|l| (at - l).length())) {
            Some(Ok(layers)) => {
                for (event, gain) in layers {
                    self.play_at_gain(event, at, gain);
                }
            }
            Some(Err(chain)) => self.play_at(chain, at),
            None => {}
        }
    }

    /// Forget movement history (first person starts, or the General changed).
    pub fn reset(&mut self) {
        let (queue, loops) = (std::mem::take(&mut self.queue), std::mem::take(&mut self.loops));
        *self = Cues { queue, loops, ..Default::default() };
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
        self.weapon_moments(p);
    }

    /// BO2's def-level weapon sounds at their moments ([`WeaponCues::step`]), the player's own
    /// (`*SoundPlayer`) versions.
    fn weapon_moments(&mut self, p: &PlayerSim) {
        use crate::weapons::arsenal::RaiseKind;
        let mut queue = std::mem::take(&mut self.queue);
        self.weapon.step(&p.arsenal, |m| {
            let e = match m {
                Moment::Putaway(d) => some(d.putaway_sound.as_str()),
                Moment::AltSwitch(d) => some(d.alt_switch_sound.as_str()),
                Moment::Raise(d, RaiseKind::First) => some(d.first_raise_sound.as_str()),
                // An empty raise is a raise with its own time (switch-timing); its sound is the raise.
                Moment::Raise(d, RaiseKind::Normal | RaiseKind::Quick | RaiseKind::Empty) => some(d.raise_sound.as_str()),
                Moment::Raise(_, RaiseKind::Alt) => None,
                Moment::AdsZoom(d) => some(d.ads_zoom_sound.as_str()),
            };
            if let Some(e) = e {
                queue.push(Request { event: e, at: None, gain: 1.0 });
            }
        });
        self.queue = queue;
    }
}

/// A weapon moment BO2's client plays a def-level sound at ([`WeaponCues::step`]), with the def
/// whose field it plays.
#[derive(Clone, Copy, Debug)]
pub enum Moment {
    /// A drop begins: the weapon going away (EV_PUTAWAY_WEAPON).
    Putaway(&'static crate::weapons::WeaponDef),
    /// An alt toggle begins (EV_WEAPON_ALT).
    AltSwitch(&'static crate::weapons::WeaponDef),
    /// The drop is done and the raise runs (EV_RAISE_WEAPON; the first raise of a life its own kind).
    Raise(&'static crate::weapons::WeaponDef, crate::weapons::arsenal::RaiseKind),
    /// Aiming starts.
    AdsZoom(&'static crate::weapons::WeaponDef),
}

impl WeaponCues {
    /// The moments since the last frame, read off the arsenal's state changes: the putaway as a
    /// drop begins (the weapon going away), the raise — or the first raise — once the drop is
    /// done and the raise runs, and the zoom as aiming starts. An alt toggle is
    /// [`Moment::AltSwitch`] as it begins and its raise is [`RaiseKind::Alt`]: BO2's weapon change
    /// sends EV_WEAPON_ALT instead of EV_PUTAWAY_WEAPON for an alt pair (`FUN_008db500`) and
    /// EV_RAISE_PICKUP_WEAPON instead of EV_RAISE_WEAPON (`FUN_008df320`); the client plays the
    /// held def's `altSwitchSound(Player)` for the first and nothing for the second
    /// (`FUN_004c1590`). The held def is the one up after the toggle's command: with no alt drop
    /// (`iAltDropTime` 0, every launcher pair) the change finishes in that same pmove
    /// (`FUN_008e4da0`), so it is the incoming mode; a toggle with an alt drop (select fire's 500 ms)
    /// is the outgoing mode as the drop begins.
    ///
    /// [`RaiseKind::Alt`]: crate::weapons::arsenal::RaiseKind::Alt
    pub fn step(&mut self, a: &crate::weapons::Arsenal, mut on: impl FnMut(Moment)) {
        use crate::weapons::arsenal::RaiseKind;
        let prev = std::mem::take(self);
        let mut now = WeaponCues { seen: true, raise_left: a.raise_left, raise_played: prev.raise_played, drop_left: a.drop_left, ads: a.ads };
        if a.drop_left > prev.drop_left + 1e-6 {
            // An alt toggle with a drop (alt-timing's `iAltDropTime`) begins here: BO2 sends
            // EV_WEAPON_ALT, not EV_PUTAWAY_WEAPON, and the held def is still the outgoing mode.
            if a.alt_pending {
                on(Moment::AltSwitch(a.current().def));
            } else if let Some(w) = a.weapons.get(a.previous) {
                on(Moment::Putaway(w.def));
            }
        }
        // A new raise (the timer jumped up, or the first frame seen with one running) re-arms it.
        if !prev.seen || a.raise_left > prev.raise_left + 1e-6 {
            now.raise_played = false;
            // A zero-drop alt toggle (every launcher pair) changes the mode in the same pmove: the
            // sound is the incoming mode's. A toggle with a drop played it as the drop began.
            if a.raise_kind == RaiseKind::Alt && prev.drop_left <= 0.0 {
                on(Moment::AltSwitch(a.current().def));
            }
        }
        if a.raise_left > 0.0 && a.drop_left <= 0.0 && !now.raise_played {
            now.raise_played = true;
            on(Moment::Raise(a.current().def, a.raise_kind));
        }
        if prev.ads <= 0.0 && a.ads > 0.0 {
            on(Moment::AdsZoom(a.current().def));
        }
        *self = now;
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
        assert_eq!((launch_for(plain), throw_for(plain), pullback_for(plain), melee_swing_for(plain)), (FIRE_LAUNCHER, THROW, None, MELEE_SWING));
        assert_eq!(explosion_cues("", 0, crate::impacts::Surface::Dirt), [Some(EXPLOSION), None]);
        let strela = leak(crate::weapons::WeaponDef { fire_sound: "A".into(), fire_last_sound: "LAST".into(), ..Default::default() });
        assert_eq!(launch_for(strela), "A");
        assert_eq!(shot_for(strela, 0, "A"), "LAST", "the last round's own shot");
        assert_eq!(shot_for(strela, 1, "A"), "A");
        assert_eq!(shot_for(plain, 0, FIRE), FIRE);
        let semtex = leak(crate::weapons::WeaponDef { fire_sound: "T".into(), pullback_sound: "P".into(), melee_swipe_sound: "S".into(), ..Default::default() });
        assert_eq!((throw_for(semtex), pullback_for(semtex), melee_swing_for(semtex)), ("T", Some("P"), "S"));
        assert_eq!(explosion_cues("B", 0, crate::impacts::Surface::Dirt), [None, Some("B")], "no table cell: the def's own only");
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

    /// Another first-person General's steps are heard at him, one per stride, the sprint's with
    /// its own event; the presented seat's are not (he hears his own 2D steps), nor a General
    /// with Dead Silence's (`specialty_quieter`).
    #[test]
    fn other_generals_footsteps_are_heard_at_them() {
        let walker = |general: u32| {
            let mut p = PlayerSim::spawn_for_tests();
            p.general = general;
            p
        };
        let mut players: Vec<Option<PlayerSim>> = vec![Some(walker(1)), Some(walker(2)), None];
        let mut steps = RemoteSteps::default();
        let mut c = Cues::default();
        let heard = |players: &mut Vec<Option<PlayerSim>>, steps: &mut RemoteSteps, c: &mut Cues| {
            let mut n = Vec::new();
            for _ in 0..100 {
                for p in players.iter_mut().flatten() {
                    p.body.pos.x += 1.0;
                }
                steps.update(players, 0, c);
                n.extend(c.queue.drain(..).map(|r| (r.event, r.at)));
            }
            n
        };
        let n = heard(&mut players, &mut steps, &mut c);
        assert!((10..=12).contains(&n.len()), "{} steps over 100 units", n.len());
        assert!(n.iter().all(|(e, at)| *e == STEP_NPC && at.is_some_and(|a| a.y == 0.0 && a.x > 0.0)), "his, at him: {n:?}");
        players[1].as_mut().unwrap().body.sprinting = true;
        let n = heard(&mut players, &mut steps, &mut c);
        assert!(!n.is_empty() && n.iter().all(|(e, _)| *e == SPRINT_STEP_NPC), "{n:?}");
        // Dead Silence: none.
        let p = players[1].as_mut().unwrap();
        p.arsenal.loadout.perks.push(crate::weapons::Perk::by_id("DeadSilence").expect("Dead Silence"));
        assert!(p.arsenal.has_specialty(crate::weapons::specialty::Specialty::Quieter));
        assert!(heard(&mut players, &mut steps, &mut c).is_empty(), "Dead Silence");
        // A jump plays the step at him, the landing his landing.
        players[0] = None;
        let p = players[1].as_mut().unwrap();
        p.arsenal.loadout.perks.clear();
        let mut steps = RemoteSteps::default();
        steps.update(&players, 0, &mut c);
        c.queue.clear();
        let p = players[1].as_mut().unwrap();
        p.body.on_ground = false;
        p.body.vel.z = 120.0;
        steps.update(&players, 0, &mut c);
        assert_eq!(events(&mut c), [STEP_NPC], "the jump");
        players[1].as_mut().unwrap().body.vel.z = -200.0;
        steps.update(&players, 0, &mut c);
        players[1].as_mut().unwrap().body.on_ground = true;
        steps.update(&players, 0, &mut c);
        assert_eq!(events(&mut c), [LAND_NPC], "the landing");
        // Overhead (no first person): none either.
        players[1] = None;
        assert!(heard(&mut players, &mut steps, &mut c).is_empty());
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
