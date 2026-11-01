//! MappedImage names produced by `zhbo_assets::hud` and `zhbo_assets::cac`.

use crate::classmenu::Wildcard;
use crate::weapons::Perk;

pub const DIGITS: [&str; 10] = [
    "ZHBO_BO2_Digit0",
    "ZHBO_BO2_Digit1",
    "ZHBO_BO2_Digit2",
    "ZHBO_BO2_Digit3",
    "ZHBO_BO2_Digit4",
    "ZHBO_BO2_Digit5",
    "ZHBO_BO2_Digit6",
    "ZHBO_BO2_Digit7",
    "ZHBO_BO2_Digit8",
    "ZHBO_BO2_Digit9",
];
pub const SLASH: &str = "ZHBO_BO2_Slash";
pub const BLOOD: &str = "ZHBO_BO2_Blood";
pub const RED_DOT: &str = "ZHBO_BO2_RedDot";
pub const HIT_MARKER: &str = "ZHBO_BO2_HitMarker";
pub const PLAYER_ARROW: &str = "ZHBO_BO2_PlayerArrow";
pub const PLAYER_BRACKET: &str = "ZHBO_BO2_PlayerBracket";
pub const DOT: &str = "ZHBO_BO2_Dot";
pub const COMPASS: &str = "ZHBO_BO2_Compass";
pub const LEFT_LOWER: &str = "ZHBO_BO2_LeftLower";
pub const LEFT_LOWER_BACK: &str = "ZHBO_BO2_LeftLowerBack";
pub const FACTION_BACKING: &str = "ZHBO_BO2_FactionBacking";
pub const FACTION_LIGHT: &str = "ZHBO_BO2_FactionLight";
pub const RIGHT_LOWER: &str = "ZHBO_BO2_RightLower";
pub const GUN_TITLE: [&str; 3] = ["ZHBO_BO2_GunTitleLeft", "ZHBO_BO2_GunTitleMid", "ZHBO_BO2_GunTitleRight"];
pub const STREAK_BAR: &str = "ZHBO_BO2_StreakBar";
pub const STREAK_BAR_BACK: &str = "ZHBO_BO2_StreakBarBack";
pub const STREAK_BOX: &str = "ZHBO_BO2_StreakBox";
pub const STREAK_BOX_LIT: &str = "ZHBO_BO2_StreakBoxLit";
pub const STREAK_GLOW: &str = "ZHBO_BO2_StreakGlow";
pub const STREAK_VSAT: &str = "ZHBO_BO2_StreakVsat";
/// The build menu's slot in the streak column (no BO2 art: it falls back to its name).
pub const BUILD_ICON: &str = "ZHBO_BuildIcon";

// Create-a-class icons. The perk order is `weapons::PERKS` (the converter uses the same order);
// the wildcard icons are indexed by the enum's own discriminant (`Wildcard::ALL` is the picker's
// display order, which BO2's grid sets and the icon list need not follow).
pub const PERK_ICONS: [&str; 16] = [
    "ZHBO_BO2_Perk_Lightweight",
    "ZHBO_BO2_Perk_Hardline",
    "ZHBO_BO2_Perk_BlindEye",
    "ZHBO_BO2_Perk_FlakJacket",
    "ZHBO_BO2_Perk_Ghost",
    "ZHBO_BO2_Perk_Toughness",
    "ZHBO_BO2_Perk_ColdBlooded",
    "ZHBO_BO2_Perk_FastHands",
    "ZHBO_BO2_Perk_HardWired",
    "ZHBO_BO2_Perk_Scavenger",
    "ZHBO_BO2_Perk_Dexterity",
    "ZHBO_BO2_Perk_ExtremeConditioning",
    "ZHBO_BO2_Perk_Engineer",
    "ZHBO_BO2_Perk_TacticalMask",
    "ZHBO_BO2_Perk_DeadSilence",
    "ZHBO_BO2_Perk_Awareness",
];
pub const WILDCARD_ICONS: [&str; 8] = [
    "ZHBO_BO2_Wildcard_Perk1Greed",
    "ZHBO_BO2_Wildcard_Perk2Greed",
    "ZHBO_BO2_Wildcard_Perk3Greed",
    "ZHBO_BO2_Wildcard_Overkill",
    "ZHBO_BO2_Wildcard_PrimaryGunfighter",
    "ZHBO_BO2_Wildcard_SecondaryGunfighter",
    "ZHBO_BO2_Wildcard_Tactician",
    "ZHBO_BO2_Wildcard_DangerClose",
];

// The create-a-class screen's chrome.
pub const MENU_BACKGROUND: &str = "ZHBO_BO2_MenuBackground";
pub const MENU_BACKDROP: &str = "ZHBO_BO2_MenuBackdrop";
pub const MENU_GRID: &str = "ZHBO_BO2_MenuGrid";
pub const MENU_GLOW: &str = "ZHBO_BO2_MenuGlow";
pub const ROW_FILL: &str = "ZHBO_BO2_RowFill";
pub const ROW_HIGHLIGHT: &str = "ZHBO_BO2_RowHighlight";
pub const ROW_LINE: &str = "ZHBO_BO2_RowLine";
pub const PANEL_FILL: &str = "ZHBO_BO2_PanelFill";
pub const PANEL_HIGHLIGHT: &str = "ZHBO_BO2_PanelHighlight";
pub const PANEL_LINE: &str = "ZHBO_BO2_PanelLine";
pub const PANEL_EDGE: &str = "ZHBO_BO2_PanelEdge";
pub const TILE_FILL: &str = "ZHBO_BO2_TileFill";
pub const TILE_HIGHLIGHT: &str = "ZHBO_BO2_TileHighlight";
pub const WILDCARD_FILL: &str = "ZHBO_BO2_WildcardFill";
pub const WILDCARD_HIGHLIGHT: &str = "ZHBO_BO2_WildcardHighlight";
pub const CAC_CHROME: [&str; 15] = [
    MENU_BACKGROUND,
    MENU_BACKDROP,
    MENU_GRID,
    MENU_GLOW,
    ROW_FILL,
    ROW_HIGHLIGHT,
    ROW_LINE,
    PANEL_FILL,
    PANEL_HIGHLIGHT,
    PANEL_LINE,
    PANEL_EDGE,
    TILE_FILL,
    TILE_HIGHLIGHT,
    WILDCARD_FILL,
    WILDCARD_HIGHLIGHT,
];

/// Every fixed MappedImage name the converter emits (`ZHBO_BO2_Icon_<weapon>` is per weapon).
pub const ALL: &[&str] = &[
    DIGITS[0], DIGITS[1], DIGITS[2], DIGITS[3], DIGITS[4], DIGITS[5], DIGITS[6], DIGITS[7], DIGITS[8],
    DIGITS[9], SLASH, BLOOD, RED_DOT, HIT_MARKER, PLAYER_ARROW, DOT, COMPASS, LEFT_LOWER, LEFT_LOWER_BACK,
    FACTION_BACKING, FACTION_LIGHT, RIGHT_LOWER, GUN_TITLE[0], GUN_TITLE[1], GUN_TITLE[2], STREAK_BAR,
    STREAK_BAR_BACK, STREAK_BOX, STREAK_BOX_LIT, STREAK_GLOW, STREAK_VSAT, PLAYER_BRACKET,
    PERK_ICONS[0], PERK_ICONS[1], PERK_ICONS[2], PERK_ICONS[3], PERK_ICONS[4], PERK_ICONS[5],
    PERK_ICONS[6], PERK_ICONS[7], PERK_ICONS[8], PERK_ICONS[9], PERK_ICONS[10], PERK_ICONS[11],
    PERK_ICONS[12], PERK_ICONS[13], PERK_ICONS[14], PERK_ICONS[15],
    WILDCARD_ICONS[0], WILDCARD_ICONS[1], WILDCARD_ICONS[2], WILDCARD_ICONS[3], WILDCARD_ICONS[4],
    WILDCARD_ICONS[5], WILDCARD_ICONS[6], WILDCARD_ICONS[7],
    MENU_BACKGROUND, MENU_BACKDROP, MENU_GRID, MENU_GLOW, ROW_FILL, ROW_HIGHLIGHT, ROW_LINE,
    PANEL_FILL, PANEL_HIGHLIGHT, PANEL_LINE, PANEL_EDGE, TILE_FILL, TILE_HIGHLIGHT, WILDCARD_FILL,
    WILDCARD_HIGHLIGHT,
];

/// The icon for a perk, in BO2's table order.
pub fn perk_icon(perk: Perk) -> &'static str {
    PERK_ICONS[perk.0 as usize]
}

/// The icon for a wildcard.
pub fn wildcard_icon(wildcard: Wildcard) -> &'static str {
    WILDCARD_ICONS[wildcard as usize]
}

/// The icon for an attachment short name: its create-a-class material's converted art
/// ([`crate::weapons::Catalog::attachment_image`], BO2's `mp/attachmenttable.csv` column 6), if the
/// table names one.
pub fn attachment_icon(short: &str) -> Option<String> {
    crate::weapons::catalog().attachment_image(short).map(crate::lui::materials::image)
}

/// Not BO2 art: Zero Hour's faction logo for the local player (the bridge resolves it).
pub const TEAM_LOGO: &str = "ZHBO_TeamLogo";
