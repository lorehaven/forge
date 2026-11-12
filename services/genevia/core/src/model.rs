//! The people, families and events of one tree.

use crate::date::Date;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sex {
    Male,
    Female,
    Unknown,
}

/// Where something was found out: a record, a book, a web page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Citation {
    /// What the source is (`Akt urodzenia`, a book's title).
    pub title: String,
    /// Where in it: the parish, the year and number, the page.
    pub detail: String,
}

/// Something that happened to a person or a couple.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Event {
    pub date: Option<Date>,
    pub place: Option<String>,
    pub notes: Vec<String>,
    /// `CAUS`: what a death was from.
    pub cause: Option<String>,
    /// Godparents at a baptism, witnesses at a wedding.
    pub witnesses: Vec<String>,
    pub sources: Vec<Citation>,
}

impl Event {
    /// Whether there is nothing in it worth keeping.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.date.is_none()
            && self.place.is_none()
            && self.notes.is_empty()
            && self.cause.is_none()
            && self.witnesses.is_empty()
            && self.sources.is_empty()
    }
}

/// A fact or event of a person that is not their birth or death: `CHR`, `BURI`, `OCCU`, `EDUC`,
/// `RESI`, `RELI`, `NATI`, `TITL`, `IMMI`, `EMIG`, `EVEN` and the like, under their GEDCOM tags.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fact {
    pub kind: String,
    /// The text on the tag's own line: the occupation, the religion, the title.
    pub value: String,
    pub event: Event,
}

#[derive(Clone, Debug)]
pub struct Person {
    pub xref: String,
    pub given: String,
    pub surname: String,
    /// Other names (`NAME` with `TYPE aka`): given name and surname as written.
    pub akas: Vec<String>,
    pub nickname: String,
    /// `NPFX` and `NSFX`: Dr, Jr.
    pub prefix: String,
    pub suffix: String,
    /// The surname a woman was born with, when it differs from the one she goes by.
    pub maiden_name: String,
    pub sex: Sex,
    pub birth: Option<Event>,
    pub death: Option<Event>,
    pub facts: Vec<Fact>,
    pub notes: Vec<String>,
    /// Sources for the person as a whole, not for one event.
    pub sources: Vec<Citation>,
    /// Families this person is a child of.
    pub famc: Vec<String>,
    /// Families this person is a spouse in.
    pub fams: Vec<String>,
    /// Ahnentafel number from `_AHN`, when the file carries one.
    pub ahn: Option<u32>,
}

impl Person {
    #[must_use]
    pub fn full_name(&self) -> String {
        match (self.given.is_empty(), self.surname.is_empty()) {
            (false, false) => format!("{} {}", self.given, self.surname),
            (false, true) => self.given.clone(),
            (true, false) => self.surname.clone(),
            (true, true) => "?".to_string(),
        }
    }

    #[must_use]
    pub fn birth_year(&self) -> Option<i32> {
        self.birth.as_ref()?.date.as_ref()?.year
    }

    #[must_use]
    pub fn death_year(&self) -> Option<i32> {
        self.death.as_ref()?.date.as_ref()?.year
    }
}

#[derive(Clone, Debug)]
pub struct Family {
    pub xref: String,
    pub husband: Option<String>,
    pub wife: Option<String>,
    pub children: Vec<String>,
    pub marriage: Option<Event>,
    pub notes: Vec<String>,
}
