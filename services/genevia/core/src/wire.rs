//! What crosses the wire between the browser and the service: the shapes the API answers with and
//! the forms it takes. Shared by the screens, the browser client and the service.

use quench_serde::{Deserialize, Serialize};

/// A tree in the list on the home page.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct TreeInfo {
    pub id: String,
    pub name: String,
    pub people: u32,
    pub families: u32,
    /// The person the tree is about.
    pub root: String,
    pub root_name: String,
}

/// A person in brief: enough for a row, a card or a sector.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Card {
    pub xref: String,
    pub name: String,
    pub given: String,
    pub surname: String,
    /// Birth and death as displayed (`3 wrz 1993`, `ok. 1765`), empty when unknown.
    pub born: String,
    pub died: String,
    pub birth_year: Option<i32>,
    pub death_year: Option<i32>,
    pub birth_place: String,
    /// `M`, `F` or empty.
    pub sex: String,
    pub has_parents: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct AncestorSlot {
    pub ahn: u32,
    pub generation: u32,
    pub card: Card,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Ancestry {
    pub tree: String,
    pub root: Card,
    /// Ordered by Ahnentafel number.
    pub slots: Vec<AncestorSlot>,
    /// How many generations the tree has above the root, ignoring the limit asked for.
    pub depth: u32,
}

/// Where something was found out.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct CitationView {
    pub title: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct EventView {
    pub date: String,
    pub place: String,
    pub notes: Vec<String>,
    /// What a death was from.
    pub cause: String,
    /// Godparents at a baptism, witnesses at a wedding.
    pub witnesses: Vec<String>,
    pub sources: Vec<CitationView>,
}

/// A fact or event other than birth and death, under its GEDCOM tag (`CHR`, `BURI`, `OCCU`, ...).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct FactView {
    pub kind: String,
    pub value: String,
    pub event: EventView,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct UnionView {
    /// The family's id, to edit the union by.
    pub family: String,
    pub marriage: Option<EventView>,
    pub spouse: Option<Card>,
    pub children: Vec<Card>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PersonDetail {
    pub card: Card,
    pub akas: Vec<String>,
    pub nickname: String,
    pub prefix: String,
    pub suffix: String,
    /// The surname a woman was born with, when it differs from the one she goes by.
    pub maiden_name: String,
    pub birth: Option<EventView>,
    pub death: Option<EventView>,
    pub father: Option<Card>,
    pub mother: Option<Card>,
    /// The family they were born into, when they have one.
    pub parents_family: Option<String>,
    pub unions: Vec<UnionView>,
    pub facts: Vec<FactView>,
    pub notes: Vec<String>,
    /// Sources for the person as a whole.
    pub sources: Vec<CitationView>,
    pub ahn: Option<u32>,
    /// Whether the caller may change this tree (the API sets it; the screens show the edit controls by it).
    #[serde(default)]
    pub editable: bool,
}

// ---------------------------------------------------------------------------------------------
// Editing. Everything crosses the wire as text, the way it is typed: dates as `21.08.1967`, lists
// as text with `;` between items, notes and sources one to a line.
// ---------------------------------------------------------------------------------------------

/// A birth, a death or a wedding as a form has it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct EventEdit {
    pub date: String,
    pub place: String,
    /// What a death was from (unused elsewhere).
    pub cause: String,
    /// Godparents or witnesses, separated by `;`.
    pub witnesses: String,
    /// One source a line: `Akt urodzenia: USC Mikołów 1967 nr 893`.
    pub sources: String,
    pub notes: String,
}

/// A fact (occupation, burial, ...), by GEDCOM tag.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct FactEdit {
    pub kind: String,
    pub value: String,
    pub date: String,
    pub place: String,
    pub sources: String,
    pub notes: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct PersonEdit {
    pub given: String,
    pub surname: String,
    pub maiden_name: String,
    pub nickname: String,
    pub prefix: String,
    pub suffix: String,
    /// Other names, separated by `;`.
    pub akas: String,
    /// `M`, `K` or empty.
    pub sex: String,
    pub birth: EventEdit,
    pub death: EventEdit,
    pub facts: Vec<FactEdit>,
    pub notes: String,
    pub sources: String,
}

/// A couple, their wedding and their children, as a form has them. Spouses and children are
/// people's ids.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct FamilyEdit {
    pub husband: String,
    pub wife: String,
    pub children: Vec<String>,
    pub marriage: EventEdit,
    pub notes: String,
}

/// A family to edit, with the people in it named.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct FamilyForm {
    pub xref: String,
    pub edit: FamilyEdit,
    pub husband: Option<Card>,
    pub wife: Option<Card>,
    pub children: Vec<Card>,
}

/// What can be added to a person.
pub const RELATIONS: [(&str, &str); 4] = [
    ("father", "ojca"),
    ("mother", "matkę"),
    ("spouse", "małżonka"),
    ("child", "dziecko"),
];

/// The kinds of fact a person can have, by GEDCOM tag.
pub const FACT_KINDS: [(&str, &str); 14] = [
    ("OCCU", "Zawód"),
    ("BURI", "Pochówek"),
    ("CHR", "Chrzest"),
    ("CREM", "Kremacja"),
    ("EDUC", "Wykształcenie"),
    ("RESI", "Zamieszkanie"),
    ("RELI", "Wyznanie"),
    ("NATI", "Narodowość"),
    ("TITL", "Tytuł"),
    ("IMMI", "Imigracja"),
    ("EMIG", "Emigracja"),
    ("NATU", "Naturalizacja"),
    ("CENS", "Spis ludności"),
    ("EVEN", "Wydarzenie"),
];

/// What `POST /api/v1/trees` answers: the tree the file became.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Imported {
    pub id: String,
    pub name: String,
    pub people: u32,
    pub families: u32,
    /// A tree of that name was there and has been replaced.
    pub replaced: bool,
}

/// `POST .../people/{xref}/relatives`: who to add next to the person in the address.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct RelativeBody {
    /// `father`, `mother`, `spouse` or `child`.
    pub relation: String,
    /// Somebody already in the tree to link; blank adds `person` as new.
    pub existing: String,
    pub person: PersonEdit,
    /// For a child: the family it goes into (blank: the person's only family, or a new one).
    pub family: String,
}

/// What the API says when it refuses.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Problem {
    pub error: String,
    /// The form field at fault, when there is one.
    #[serde(default)]
    pub field: String,
}

/// `GET /api/v1/me`: who is signed in, and whether they may add and change trees.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Me {
    pub name: String,
    /// The blanket `write` grant: import files, edit any tree. (A grant on one tree is checked by the API per request.)
    pub write: bool,
}
