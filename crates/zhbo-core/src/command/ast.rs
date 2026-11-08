//! The command AST: one first-person command as the canonical language describes it
//! (docs/COMMANDS.md section 4). [`super::parse`] builds it from canonical text, [`super::print`]
//! writes it back, and each AST has exactly one spelling.
//!
//! Names (unit types, places, team names, powers) are kept as the canonical words of the match's
//! [`super::Vocabulary`]: a type by its singular form, the rest as listed. Whether a noun is
//! printed singular or plural follows from the [`Quantity`], so the AST never stores it.

/// A whole command: an optional selection and one or more orders chained with `then`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    /// `None`: the selection is implied by the first order (section 7, "Implied selection").
    pub selection: Option<Selection>,
    /// Never empty. Orders after the first are queued behind it.
    pub orders: Vec<Order>,
}

/// The units a command is for: groups joined with `and`, minus groups after `except`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    /// Never empty.
    pub include: Vec<Group>,
    pub exclude: Vec<Group>,
}

/// One term of a selection. Selections are always the player's own units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Group {
    Query(Query),
    /// `group N`: the engine's control group N (0-9).
    ControlGroup(u8),
    /// A team name the player saved this match (`name them …`).
    Team(String),
    /// `them`: the last resolved selection.
    Them,
    /// `that`: the object under the crosshair.
    That,
}

/// A description of a set of objects: how many, sorted how, in what state, of what kind, where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    pub quantity: Quantity,
    /// Only with [`Quantity::One`] or [`Quantity::Count`].
    pub superlative: Option<Superlative>,
    /// Strictly ascending (the canonical order is the enum's order), no repeats.
    pub adjectives: Vec<Adjective>,
    pub base: Base,
    pub filters: Filters,
}

/// How many of the matching objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quantity {
    /// A bare plural: all of them (`rangers`).
    All,
    /// A singular with `the` or `my` (`the ranger`).
    One,
    /// A number of 2 or more with a plural (`3 rangers`, `the 3 nearest rangers`).
    Count(u32),
    /// `half the` / `half my`, rounded up.
    Half,
}

/// The kind of object a query matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Base {
    /// A thing template, by the singular form of its display name.
    Type(String),
    /// A class word (KindOf), by its singular form.
    Class(String),
    /// `units`: what Zero Hour's select-all key selects.
    Units,
}

/// The filters after the noun, at most one of each, printed in this order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filters {
    /// `near <point>`.
    pub near: Option<Box<Point>>,
    /// `in the <region> group`: a cluster named by its compass region.
    pub region: Option<Region>,
    /// `in my group`: the cluster the General is in.
    pub my_group: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Superlative {
    Nearest,
    Farthest,
    /// `northernmost`, `southernmost`, `easternmost`, `westernmost`.
    Most(Cardinal),
    Weakest,
    Healthiest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cardinal {
    North,
    South,
    East,
    West,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Adjective {
    Idle,
    Damaged,
    Veteran,
    Elite,
    Heroic,
    Garrisoned,
    Loaded,
}

/// Whose objects a target query matches. Selections are always [`Relation::Any`] in the AST (they
/// are the player's own by rule, and never say so).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    Any,
    /// `my`, in the determiner's place.
    Own,
    Enemy,
    Allied,
    Neutral,
    Civilian,
}

/// An object named as a target or a place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectRef {
    /// `that`: the object under the crosshair.
    That,
    Query { relation: Relation, query: Query },
}

/// A place in the world: a point, optionally offset from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub offset: Option<Offset>,
    pub point: Point,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offset {
    Near,
    /// `north of`, `southwest of`, …
    Of(Direction),
    Behind,
    InFrontOf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Point {
    /// The General.
    Me,
    /// The crosshair's hit on the ground.
    There,
    Object(ObjectRef),
    /// A map place name.
    Place(String),
}

/// The eight compass directions, on the radar's map (north is the top of the radar).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    North,
    South,
    East,
    West,
    Northeast,
    Northwest,
    Southeast,
    Southwest,
}

/// A cluster's name: its compass region from the map centre, or the centre itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Northern,
    Southern,
    Eastern,
    Western,
    Northeastern,
    Northwestern,
    Southeastern,
    Southwestern,
    Central,
}

/// One order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Order {
    /// A verb from the vocabulary with the arguments its [`super::VerbDef`] takes.
    Verb {
        /// The verb's words without its preposition (`move`, `attack-move`, `set rally point`).
        verb: String,
        thing: Option<Thing>,
        /// Boxed: a target with a line's two ends is several times the other variants' size.
        target: Option<Box<Target>>,
    },
    /// `name them <name>`: save the selection as a team name.
    NameThem(String),
    /// `make them group N`: the engine's control group N.
    MakeGroup(u8),
}

/// What a `build`, `upgrade`, `use` or `purchase` order is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thing {
    /// The canonical name: a type's singular form, or the upgrade, power or science as listed.
    pub name: String,
    /// 1 unless the verb takes a count (`build 3 crusader tanks`).
    pub count: u32,
}

/// Where or at what an order is aimed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub location: Location,
    /// `facing <direction>`: a building's angle, for verbs that place one.
    pub facing: Option<Direction>,
    /// `from <location> to <end>`: a line structure's far end (a wall, `MSG_DOZER_CONSTRUCT_LINE`);
    /// `location` is then its near end.
    pub end: Option<Location>,
}

impl Superlative {
    pub const ALL: [Superlative; 8] = [
        Superlative::Nearest,
        Superlative::Farthest,
        Superlative::Most(Cardinal::North),
        Superlative::Most(Cardinal::South),
        Superlative::Most(Cardinal::East),
        Superlative::Most(Cardinal::West),
        Superlative::Weakest,
        Superlative::Healthiest,
    ];

    pub fn word(self) -> &'static str {
        match self {
            Superlative::Nearest => "nearest",
            Superlative::Farthest => "farthest",
            Superlative::Most(Cardinal::North) => "northernmost",
            Superlative::Most(Cardinal::South) => "southernmost",
            Superlative::Most(Cardinal::East) => "easternmost",
            Superlative::Most(Cardinal::West) => "westernmost",
            Superlative::Weakest => "weakest",
            Superlative::Healthiest => "healthiest",
        }
    }
}

impl Adjective {
    pub const ALL: [Adjective; 7] = [
        Adjective::Idle,
        Adjective::Damaged,
        Adjective::Veteran,
        Adjective::Elite,
        Adjective::Heroic,
        Adjective::Garrisoned,
        Adjective::Loaded,
    ];

    pub fn word(self) -> &'static str {
        match self {
            Adjective::Idle => "idle",
            Adjective::Damaged => "damaged",
            Adjective::Veteran => "veteran",
            Adjective::Elite => "elite",
            Adjective::Heroic => "heroic",
            Adjective::Garrisoned => "garrisoned",
            Adjective::Loaded => "loaded",
        }
    }
}

impl Relation {
    /// The relations spelled as a word before the noun ([`Relation::Own`] is `my`, in the
    /// determiner's place, and [`Relation::Any`] is unspoken).
    pub const WORDS: [Relation; 4] = [Relation::Enemy, Relation::Allied, Relation::Neutral, Relation::Civilian];

    pub fn word(self) -> Option<&'static str> {
        match self {
            Relation::Enemy => Some("enemy"),
            Relation::Allied => Some("allied"),
            Relation::Neutral => Some("neutral"),
            Relation::Civilian => Some("civilian"),
            Relation::Any | Relation::Own => None,
        }
    }
}

impl Direction {
    pub const ALL: [Direction; 8] = [
        Direction::North,
        Direction::South,
        Direction::East,
        Direction::West,
        Direction::Northeast,
        Direction::Northwest,
        Direction::Southeast,
        Direction::Southwest,
    ];

    pub fn word(self) -> &'static str {
        match self {
            Direction::North => "north",
            Direction::South => "south",
            Direction::East => "east",
            Direction::West => "west",
            Direction::Northeast => "northeast",
            Direction::Northwest => "northwest",
            Direction::Southeast => "southeast",
            Direction::Southwest => "southwest",
        }
    }
}

impl Region {
    pub const ALL: [Region; 9] = [
        Region::Northern,
        Region::Southern,
        Region::Eastern,
        Region::Western,
        Region::Northeastern,
        Region::Northwestern,
        Region::Southeastern,
        Region::Southwestern,
        Region::Central,
    ];

    pub fn word(self) -> &'static str {
        match self {
            Region::Northern => "northern",
            Region::Southern => "southern",
            Region::Eastern => "eastern",
            Region::Western => "western",
            Region::Northeastern => "northeastern",
            Region::Northwestern => "northwestern",
            Region::Southeastern => "southeastern",
            Region::Southwestern => "southwestern",
            Region::Central => "central",
        }
    }
}

impl Offset {
    pub fn words(self) -> &'static str {
        match self {
            Offset::Near => "near",
            Offset::Of(d) => match d {
                Direction::North => "north of",
                Direction::South => "south of",
                Direction::East => "east of",
                Direction::West => "west of",
                Direction::Northeast => "northeast of",
                Direction::Northwest => "northwest of",
                Direction::Southeast => "southeast of",
                Direction::Southwest => "southwest of",
            },
            Offset::Behind => "behind",
            Offset::InFrontOf => "in front of",
        }
    }

    pub fn all() -> impl Iterator<Item = Offset> {
        [Offset::Near, Offset::Behind, Offset::InFrontOf]
            .into_iter()
            .chain(Direction::ALL.into_iter().map(Offset::Of))
    }
}
