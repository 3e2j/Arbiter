//! Twilight Princess's BMG flow queries and events, `dMsgFlow_c`'s.

use crate::games::Versions;
use crate::games::entry::{Entry, entry};
use crate::games::tp::{GCN, Version, WII};

/// The query `version` reads for a branch node's `id`, or `None` when
/// nothing names it.
#[must_use]
pub fn find_query(version: Version, id: u16) -> Option<&'static Query> {
    queries(version).find(|query| query.id == id)
}

/// Every query `version` reads, for a picker to list.
pub fn queries(version: Version) -> impl Iterator<Item = &'static Query> {
    QUERIES
        .iter()
        .filter(move |query| query.versions.intersects(version.versions()))
}

/// The event for an event node's `id`, or `None` when nothing names it.
#[must_use]
pub fn find_event(id: u8) -> Option<&'static Event> {
    EVENTS.iter().find(|event| event.id == id)
}

/// Every event, for a picker to list.
#[must_use]
pub fn events() -> &'static [Event] {
    EVENTS
}

/// What a branch node asks. The game jumps to edge `next_node_idx + result`
/// without checking it against the node's `result_count`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Query {
    /// The id a branch node stores.
    pub id: u16,
    pub name: &'static str,
    /// The node's u16 argument, or `None` when the query ignores it.
    pub param: Option<Param>,
    pub results: Results,
    pub notes: Option<&'static str>,
    /// As [`Entry::versions`].
    pub versions: Versions,
}

/// The results a query returns, each one an edge out of the branch node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Results {
    /// One name per result, in order.
    Named(&'static [&'static str]),
    /// Results 0 up to the count, each the value itself.
    Count(u16),
    /// As many results as the param.
    Param,
}

/// What an event node does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    /// The id an event node stores.
    pub id: u8,
    pub name: &'static str,
    /// The fields the event reads from the node's 4 argument bytes.
    pub params: &'static [Param],
    pub control: Control,
    pub notes: Option<&'static str>,
}

/// How an event moves the flow on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Follows `next` once the event has run.
    Next,
    /// Ends the conversation once it has run. `next` is kept but never run.
    End,
    /// Starts the flow its param names. `next` is never followed.
    Jump,
    /// Closes the box and holds for its timer, then follows `next`.
    Wait,
    /// Makes the next text node a choice.
    Select(Select),
    /// Hands the nodes after it to another talk partner, then follows `next`.
    Speaker,
}

/// The shape a choice takes. Any other shape asserts in debug builds and
/// stalls the conversation in retail ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Select {
    /// The question's text node is followed by a text node holding the
    /// choices.
    Vertical,
    /// The question's text node holds the choices and is followed by a
    /// branch.
    Horizontal,
}

/// One field of a flow node's argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Param {
    /// Byte offset into the argument.
    pub offset: u8,
    /// 1, 2 or 4 bytes, big-endian.
    pub len: u8,
    pub name: &'static str,
    /// The highest value the game reads as meant. Past it, the game indexes
    /// past a table or drops the high bits.
    pub max: u32,
    pub kind: Kind,
}

/// What a param's value refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Read as is. No table names its values.
    Raw,
    /// One of the values the table names, read through [`super::values`].
    Values(&'static [Entry]),
    /// An index into the game's event flag labels.
    EventFlag,
    /// An index into the game's temp flag labels.
    TempFlag,
    Item,
    /// A flow id, looked up in the message group's flow roots.
    Flow,
    /// Given meaning by the speaking actor, so no table can name it.
    Speaker,
}

const fn query(id: u16, name: &'static str, results: Results) -> Query {
    Query {
        id,
        name,
        param: None,
        results,
        notes: None,
        versions: Versions::ANY,
    }
}

impl Query {
    const fn param(self, param: Param) -> Self {
        Self {
            param: Some(param),
            ..self
        }
    }

    const fn notes(self, notes: &'static str) -> Self {
        Self {
            notes: Some(notes),
            ..self
        }
    }

    const fn only(self, versions: Versions) -> Self {
        Self { versions, ..self }
    }
}

const fn event(id: u8, name: &'static str) -> Event {
    Event {
        id,
        name,
        params: &[],
        control: Control::Next,
        notes: None,
    }
}

impl Event {
    const fn params(self, params: &'static [Param]) -> Self {
        Self { params, ..self }
    }

    const fn control(self, control: Control) -> Self {
        Self { control, ..self }
    }

    const fn notes(self, notes: &'static str) -> Self {
        Self {
            notes: Some(notes),
            ..self
        }
    }
}

const fn param(offset: u8, len: u8, name: &'static str) -> Param {
    let max = match len {
        1 => 0xFF,
        2 => 0xFFFF,
        _ => u32::MAX,
    };
    Param {
        offset,
        len,
        name,
        max,
        kind: Kind::Raw,
    }
}

/// A branch node's argument.
const fn arg(name: &'static str) -> Param {
    param(0, 2, name)
}

/// An event's whole argument.
const fn p(name: &'static str) -> Param {
    param(0, 4, name)
}

/// An event argument's first half.
const fn p0(name: &'static str) -> Param {
    param(0, 2, name)
}

/// An event argument's second half.
const fn p1(name: &'static str) -> Param {
    param(2, 2, name)
}

impl Param {
    const fn max(self, max: u32) -> Self {
        Self { max, ..self }
    }

    const fn kind(self, kind: Kind) -> Self {
        Self { kind, ..self }
    }

    const fn values(self, values: &'static [Entry]) -> Self {
        self.kind(Kind::Values(values))
    }
}

const SET: Results = Results::Named(&["Set", "Unset"]);
const ENOUGH: Results = Results::Named(&["Enough", "Short"]);
const FITS: Results = Results::Named(&["Over max", "Fits"]);
const YES_NO: Results = Results::Named(&["No", "Yes"]);

/// The game's queries, which `mQueryList` doesn't hold in handler order.
#[rustfmt::skip]
static QUERIES: &[Query] = &[
    query(0,  "Select 2",              Results::Named(&["First", "Second"])).notes("The choice picked. Same as query 4: the choices message sets how many"),
    query(1,  "Event flag",            SET)           .param(arg("Flag").max(821).kind(Kind::EventFlag)),
    query(2,  "Player form",           Results::Named(&["Human", "Wolf", "Riding"])),
    query(3,  "Random",                Results::Param).param(arg("Results")).notes("Results 0 always picks 0"),
    query(4,  "Select 3",              Results::Named(&["First", "Second", "Third"])).notes("The choice picked. Same as query 0: the choices message sets how many"),
    query(5,  "Talk distance",         Results::Named(&["Out of range", "In range"])).param(arg("Max distance")).notes("Max distance 0 uses the speaker's own"),
    query(6,  "Rupees",                ENOUGH)        .param(arg("Rupees")).notes("Rupees 0 checks for a full wallet"),
    query(7,  "Sword tutorial step",   Results::Named(&["Matches", "Differs"])).param(arg("Step")).notes("Matches when the scarecrow tutorial's step minus 1 is the param"),
    query(8,  "Sword tutorial result", Results::Named(&["Success", "Failure"])),
    query(9,  "Sword tutorial count",  Results::Named(&["First success", "Other"])),
    query(10, "Temp flag",             SET)           .param(arg("Flag").max(184).kind(Kind::TempFlag)),
    query(11, "Chest flag",            SET)           .param(arg("Flag").max(63)),
    query(12, "Save switch",           SET)           .param(arg("Switch").max(127)),
    query(13, "Save item flag",        SET)           .param(arg("Flag").max(31)).notes("The debug check allows up to 63, but 32 and up read past the flags"),
    query(14, "Dungeon switch",        SET)           .param(arg("Switch").max(63)),
    query(15, "Dungeon item flag",     SET)           .param(arg("Flag").max(127)),
    query(16, "Zone switch",           SET)           .param(arg("Switch").max(31)).notes("In the speaker's room"),
    query(17, "Zone item flag",        SET)           .param(arg("Flag").max(31)).notes("In the speaker's room"),
    query(18, "One-zone switch",       SET)           .param(arg("Switch").max(15)).notes("In the speaker's room"),
    query(19, "One-zone item flag",    SET)           .param(arg("Flag").max(15)).notes("In the speaker's room"),
    query(20, "Equipped",              Results::Named(&["Not equipped", "Equipped"])).param(arg("Item").max(255).kind(Kind::Item)).only(GCN).notes("Equipment, or one of 3 item slots"),
    query(20, "Equipped",              Results::Named(&["Not equipped", "Equipped"])).param(arg("Item").max(255).kind(Kind::Item)).only(WII).notes("Equipment, or one of 4 item slots"),
    query(21, "Item owned",            Results::Named(&["Owned", "Not owned"])).param(arg("Item").max(255).kind(Kind::Item)),
    query(22, "Bomb bags",             Results::Named(&["None", "One", "Two", "Three"])),
    query(23, "Arrows",                ENOUGH)        .param(arg("Arrows").max(255)),
    query(24, "Empty bottles",         ENOUGH)        .param(arg("Bottles").max(255)),
    query(25, "Shop conversation",     Results::Named(&["Unset", "Set"])).notes("The shop's flag 7. The speaker must be a shop"),
    query(26, "Tears of light",        ENOUGH)        .param(arg("Tears").max(255)).notes("Tears 0 uses the count the area needs"),
    query(27, "Herding time",          Results::Named(&["In time", "Over"])).param(arg("Seconds")).notes("Publishes the difference for the time tag"),
    query(28, "Lantern oil",           Results::Named(&["Full", "Partial", "Empty"])).notes("Empty also without a lantern"),
    query(29, "Register",              Results::Count(256)).notes("The flow register, set by event 25"),
    query(30, "Goats remaining",       ENOUGH)        .param(arg("Goats")).notes("20 minus the ranch's runaway goat register, as the goat count tag shows"),
    query(31, "Life",                  ENOUGH)        .param(arg("Life")).notes("In quarter hearts"),
    query(32, "Holding lantern",       Results::Named(&["Holding", "Not holding"])),
    query(33, "Time of day",           Results::Count(24)).notes("The hour"),
    query(34, "Magic",                 ENOUGH)        .param(arg("Magic")),
    query(35, "Select 2, cancel",      Results::Named(&["First", "Second", "Cancelled"])).notes("The choice picked, or Cancelled on B"),
    query(36, "Select 3, cancel",      Results::Named(&["First", "Second", "Third", "Cancelled"])).notes("The choice picked, or Cancelled on B"),
    query(37, "Bomb bag contents",     Results::Named(&["Other", "Bombs", "Water bombs", "Bomblings"])).param(arg("Bag").values(BOMB_BAGS)),
    query(38, "Bombs fit",             FITS)          .param(arg("Bombs")).notes("Into the bag the register picks"),
    query(39, "Bomb bag fill",         Results::Named(&["Empty", "Partial", "Full"])).param(arg("Bag").values(BOMB_BAGS)),
    query(40, "Water bombs fit",       FITS)          .param(arg("Bombs")).notes("Into the bag the register picks"),
    query(41, "Transform blocked",     Results::Named(&["Clear", "NPC near", "NPC far", "Environment", "Castle Town"])).notes("Castle Town once dungeon switch 60 is set there"),
    query(42, "Bomblings fit",         FITS)          .param(arg("Bombs")).notes("Into the bag the register picks"),
    query(43, "Warp allowed",          Results::Named(&["Allowed", "Refused"])).notes("A dungeon warp, from here"),
    query(44, "Golden bugs",           Results::Named(&["None", "1 to 11", "12 to 22", "23", "All 24"])),
    query(45, "Undelivered bug",       YES_NO)        .notes("A golden bug not yet given to Agitha"),
    query(46, "Unused",                Results::Named(&["Always"])).notes("Asserts in debug builds"),
    query(47, "New letters",           Results::Named(&["None", "One", "More"])).notes("One also stores its sender for the letter name tag"),
    query(48, "Poe souls",             Results::Named(&["None", "1 to 19", "20 to 39", "40 to 59", "60 or more"])),
    query(49, "Offering total",        ENOUGH)        .param(arg("Rupees")),
    query(50, "Message count",         Results::Named(&["Zero", "Under 1000", "Under 10000", "Under 61454", "61454 or more"])).notes("The game calls it the balloon score, but it reads the count the fishing and letter menus set"),
    query(51, "In water",              YES_NO),
    query(52, "Iron boots",            YES_NO),
];

#[rustfmt::skip]
static EVENTS: &[Event] = &[
    event(0,  "Set event flag")        .params(&[p0("Flag").max(821).kind(Kind::EventFlag), p1("Flag").max(821).kind(Kind::EventFlag)]).notes("Flag 0 sets nothing"),
    event(1,  "Clear event flag")      .params(&[p0("Flag").max(821).kind(Kind::EventFlag), p1("Flag").max(821).kind(Kind::EventFlag)]).notes("Flag 0 clears nothing"),
    event(2,  "Add rupees")            .params(&[p("Rupees")]),
    event(3,  "Remove rupees")         .params(&[p("Rupees")]),
    event(4,  "Add life")              .params(&[p("Life")]).notes("In quarter hearts"),
    event(5,  "Remove life")           .params(&[p("Life")]).notes("In quarter hearts"),
    event(6,  "Add magic")             .params(&[p("Magic")]),
    event(7,  "Remove magic")          .params(&[p("Magic")]),
    event(8,  "Start event")           .params(&[p0("Event").kind(Kind::Speaker), p1("Item").kind(Kind::Speaker)]).control(Control::End).notes("For the speaker to read. Event 27 also sets the fund-raising total: to 0 when 200 or more of 2000 is left, else to 200 minus what's left"),
    event(9,  "Jump flow")             .params(&[p("Flow").max(0xFFFF).kind(Kind::Flow)]).control(Control::Jump).notes("Flow 0 starts the stage's message group instead, or Midna's message when one is set"),
    event(10, "Set temp flag")         .params(&[p0("Flag").max(184).kind(Kind::TempFlag), p1("Flag").max(184).kind(Kind::TempFlag)]).notes("Flag 0 sets nothing"),
    event(11, "Clear temp flag")       .params(&[p0("Flag").max(184).kind(Kind::TempFlag), p1("Flag").max(184).kind(Kind::TempFlag)]).notes("Flag 0 clears nothing"),
    event(12, "Open door")             .notes("Marks the flow as a door's unlock path. Only the door check reads it"),
    event(13, "Select vertical")       .params(&[p("Cancel result").max(255)]).control(Control::Select(Select::Vertical)).notes("The result B picks"),
    event(14, "Set switch")            .params(&[p0("Scope").values(SWITCH_SCOPES), p1("Switch").max(127)]),
    event(15, "Clear switch")          .params(&[p0("Scope").values(SWITCH_SCOPES), p1("Switch").max(127)]),
    event(16, "Shop select")           .params(&[p("Shop params").kind(Kind::Speaker)]).notes("Starts shop item selection. The speaker must be a shop"),
    event(17, "Give item")             .params(&[p0("Item").max(255).kind(Kind::Item), p1("Count")]).notes("Count 0 or 1 gives one, and only marks a vessel of light got"),
    event(18, "Speaker motion")        .params(&[param(1, 1, "Motion").kind(Kind::Speaker), param(2, 1, "Face motion").kind(Kind::Speaker), param(3, 1, "Sound")]).notes("Byte 0 is passed on, but no actor reads it. 0xFF reaches the actor as -1"),
    event(19, "Set speaker")           .params(&[p1("Partner").kind(Kind::Speaker)]).control(Control::Speaker).notes("An index into the speaker's talk partners"),
    event(20, "Warp player")           .params(&[p("Spawn").max(255)]).notes("To the speaker's room spawn with this id"),
    event(21, "Wait")                  .params(&[p("Frames")]).control(Control::Wait),
    event(22, "Fill lantern")          .params(&[p("Percent").max(100)]).notes("Percent 0 fills it"),
    event(23, "Fill bottle")           .params(&[p("Contents").values(BOTTLES)]).notes("An empty bottle"),
    event(24, "Shop sold out")         .notes("The speaker must be a shop"),
    event(25, "Set register")          .params(&[param(3, 1, "Value")]).notes("The flow register, read by query 29"),
    event(26, "Stand sold out")        .notes("The speaker must be an unattended stand's item, or the Myna speaking for one"),
    event(27, "Fill bombs")            .params(&[param(0, 1, "Bag").values(BOMB_BAGS), param(1, 1, "Operation").values(BOMB_OPERATIONS), param(2, 2, "Count").max(255)]),
    event(28, "Sell bombs")            .notes("Pays for the bombs in the bag the register picks, then empties it"),
    event(29, "Select horizontal")     .params(&[p("Cancel result").max(255)]).control(Control::Select(Select::Horizontal)).notes("The result B picks"),
    event(30, "Fill arrows")           .params(&[p0("Display only"), p1("Count")]).notes("Count 0 is the quiver's max. Display only nonzero sets the count the text shows instead"),
    event(31, "Return rental bomb bag"),
    event(32, "Fade in")               .params(&[p0("Color").values(FADE_COLORS), p1("Frames")]).control(Control::Wait).notes("Holds for frames + 1"),
    event(33, "Fade out")              .params(&[p0("Color").values(FADE_COLORS), p1("Frames")]).control(Control::Wait).notes("Holds for frames + 1"),
    event(34, "Set trade item")        .params(&[p("Item").max(255).kind(Kind::Item)]),
    event(35, "Remove item")           .params(&[p("Item").values(REMOVABLE_ITEMS)]),
    event(36, "Set save switch")       .params(&[p0("Stage").max(31), p1("Switch").max(127)]),
    event(37, "Clear save switch")     .params(&[p0("Stage").max(31), p1("Switch").max(127)]),
    event(38, "Receive letter"),
    event(39, "Table map region")      .params(&[p("Region").max(255)]),
    event(40, "Empty bottle")          .params(&[p("Contents").values(BOTTLES)]).notes("A bottle holding the contents"),
    event(41, "Add offering")          .params(&[p("Rupees")]),
    event(42, "Unused"),
];

/// Which bomb bag a query or event acts on.
#[rustfmt::skip]
static BOMB_BAGS: &[Entry] = &[
    entry(0, "Register")  .notes("The bag the flow register picks. So does any value past 4"),
    entry(1, "Bag 1"),
    entry(2, "Bag 2"),
    entry(3, "Bag 3"),
    entry(4, "Rental bag").notes("Undefined without one"),
];

/// Any other operation does nothing.
#[rustfmt::skip]
static BOMB_OPERATIONS: &[Entry] = &[
    entry(0,  "Bombs")              .notes("An empty bag takes bombs. Count 0 fills it to the bomb max"),
    entry(1,  "Water bombs")        .notes("An empty bag takes water bombs. Count 0 fills it to the water bomb max"),
    entry(2,  "Bomblings")          .notes("An empty bag takes bomblings. Count 0 fills it to the bombling max"),
    entry(3,  "None"),
    entry(4,  "Set count")          .notes("Count 0 empties it"),
    entry(5,  "Set count")          .notes("Count 0 empties it"),
    entry(6,  "Swap to bombs")      .notes("Fills the bag"),
    entry(7,  "Swap to water bombs").notes("Fills the bag"),
    entry(8,  "Swap to bomblings")  .notes("Fills the bag"),
    entry(9,  "None"),
    entry(10, "None"),
    entry(11, "None"),
];

/// Zone scopes act on the speaker's room. Any other scope does nothing.
#[rustfmt::skip]
static SWITCH_SCOPES: &[Entry] = &[
    entry(0, "Save")    .notes("Switch up to 127"),
    entry(1, "Dungeon") .notes("Switch up to 63"),
    entry(2, "Zone")    .notes("Switch up to 31"),
    entry(3, "One-zone").notes("Switch up to 15"),
];

/// Any other value does nothing.
#[rustfmt::skip]
static BOTTLES: &[Entry] = &[
    entry(1, "Red potion"),
    entry(2, "Green potion"),
    entry(3, "Blue potion"),
    entry(4, "Milk"),
    entry(5, "Half milk"),
    entry(6, "Lantern oil"),
    entry(7, "Hot spring water"),
];

#[rustfmt::skip]
static FADE_COLORS: &[Entry] = &[
    entry(0, "Black"),
    entry(1, "White").notes("So does any value past 1"),
];

/// The items event 35 removes. Any other value does nothing.
#[rustfmt::skip]
static REMOVABLE_ITEMS: &[Entry] = &[
    entry(0x80, "Renado's letter")  .notes("Clears the trade item, whichever it is"),
    entry(0x81, "Invoice")          .notes("Clears the trade item, whichever it is"),
    entry(0x82, "Wooden statue")    .notes("Clears the trade item, whichever it is"),
    entry(0x83, "Ilia's charm")     .notes("Clears the trade item, whichever it is"),
    entry(0x90, "Auru's memo")      .notes("Empties item slot 19, whatever it holds"),
    entry(0x91, "Ashei's sketch")   .notes("Empties item slot 19, whatever it holds"),
    entry(0xF4, "Ordon pumpkin")    .notes("Clears its first-get flag"),
    entry(0xF5, "Ordon goat cheese").notes("Clears its first-get flag"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::entry::assert_one_meaning;

    fn flow_params() -> impl Iterator<Item = &'static Param> {
        let queries = QUERIES.iter().filter_map(|query| query.param.as_ref());
        queries.chain(EVENTS.iter().flat_map(|event| event.params))
    }

    #[test]
    fn every_flow_id_has_one_meaning_per_version() {
        assert_one_meaning(QUERIES, |query| (query.id, query.versions));
        assert_one_meaning(EVENTS, |event| (event.id, Versions::ANY));
    }

    /// Each event's params fit its 4 bytes without overlap, and every max
    /// fits its param.
    #[test]
    fn the_flow_params_fit_their_nodes() {
        for event in EVENTS {
            let mut spans: Vec<_> = event.params.iter().map(|p| (p.offset, p.len)).collect();
            spans.sort_unstable();
            let end = spans.iter().try_fold(0, |at, &(offset, len)| {
                (offset >= at).then_some(offset + len)
            });
            assert!(end.is_some_and(|end| end <= 4), "{event:?}");
        }
        for param in flow_params() {
            let width = u32::MAX >> (32 - 8 * u32::from(param.len));
            assert!(param.max <= width, "{param:?}");
        }
    }

    #[test]
    fn a_flow_id_is_found() {
        assert_eq!(
            find_query(Version::GcnUsa, 2).map(|q| q.name),
            Some("Player form")
        );
        assert_eq!(find_query(Version::GcnUsa, 53), None);
        assert_eq!(find_event(42).map(|e| e.name), Some("Unused"));
        assert_eq!(find_event(43), None);
    }

    #[test]
    fn every_value_has_one_meaning_per_version() {
        let tables = flow_params().filter_map(|param| match param.kind {
            Kind::Values(table) => Some(table),
            _ => None,
        });
        for table in tables {
            assert_one_meaning(table, |entry| (entry.value, entry.versions));
        }
    }
}
