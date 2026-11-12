//! One tree: lookup, relatives, ancestors by generation and search.

use std::collections::HashMap;

use crate::date::Date;
use crate::gedcom::{Record, parse_records};
use crate::legacy;
use crate::model::{Citation, Event, Fact, Family, Person, Sex};
use crate::text::fold;

/// An ancestor's place in the Ahnentafel numbering: 1 is the root, 2n the father of n, 2n+1 the mother.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub ahn: u32,
    pub generation: u32,
    pub xref: String,
}

#[derive(Debug)]
pub struct Tree {
    pub name: String,
    people: Vec<Person>,
    families: Vec<Family>,
    person_ix: HashMap<String, usize>,
    family_ix: HashMap<String, usize>,
}

/// Titles of the `SOUR` records, by xref, so a citation can name what it cites.
type Sources = HashMap<String, String>;

fn source_titles(records: &[Record]) -> Sources {
    records
        .iter()
        .filter(|r| r.tag == "SOUR")
        .filter_map(|r| {
            let title = ["TITL", "ABBR", "TEXT"]
                .iter()
                .find_map(|t| r.value_of(t))?;
            Some((
                r.xref.clone()?,
                title.lines().next().unwrap_or("").to_string(),
            ))
        })
        .collect()
}

/// The citations under a record: `SOUR @S1@` pointing at a source (with `PAGE` and `DATA`/`TEXT`
/// for the detail), or `SOUR some text` written in place.
fn citations(r: &Record, src: &Sources) -> Vec<Citation> {
    r.all("SOUR")
        .map(|c| {
            let (title, mut detail) = if c.value.starts_with('@') {
                let key = c.value.trim_matches('@');
                (
                    src.get(key).cloned().unwrap_or_else(|| key.to_string()),
                    String::new(),
                )
            } else {
                (c.value.clone(), String::new())
            };
            if let Some(page) = c.value_of("PAGE") {
                detail = page.to_string();
            }
            if let Some(text) = c
                .child("DATA")
                .and_then(|d| d.value_of("TEXT"))
                .or_else(|| c.value_of("TEXT"))
            {
                if detail.is_empty() {
                    detail = text.to_string();
                } else {
                    detail = format!("{detail} — {text}");
                }
            }
            Citation { title, detail }
        })
        .collect()
}

fn event(r: &Record, src: &Sources) -> Event {
    let mut e = Event {
        date: r.value_of("DATE").map(Date::parse),
        place: r.value_of("PLAC").map(str::to_string),
        notes: r.all("NOTE").map(|n| n.value.clone()).collect(),
        cause: r.value_of("CAUS").map(str::to_string),
        // `_WITN`: a custom tag for godparents and wedding witnesses, one name each.
        witnesses: r.all("_WITN").map(|w| w.value.clone()).collect(),
        sources: citations(r, src),
    };
    legacy::event(&mut e);
    e
}

/// The tags that make a fact or event of a person, other than birth and death.
const FACTS: [&str; 22] = [
    "CHR", "BAPM", "BURI", "CREM", "OCCU", "EDUC", "RESI", "RELI", "NATI", "TITL", "IMMI", "EMIG",
    "NATU", "CENS", "PROB", "WILL", "GRAD", "RETI", "EVEN", "CAST", "DSCR", "PROP",
];

fn split_name(value: &str) -> (String, String) {
    match value.split_once('/') {
        Some((g, rest)) => (
            g.trim().to_string(),
            rest.split('/').next().unwrap_or("").trim().to_string(),
        ),
        None => (value.trim().to_string(), String::new()),
    }
}

fn person(r: &Record, src: &Sources) -> Person {
    let mut given = String::new();
    let mut surname = String::new();
    let (mut nickname, mut prefix, mut suffix, mut maiden_name) =
        (String::new(), String::new(), String::new(), String::new());
    let mut akas = Vec::new();
    for n in r.all("NAME") {
        let (mut g, mut s) = split_name(&n.value);
        // Structured parts win over the slashes of the name line.
        if let Some(v) = n.value_of("GIVN") {
            g = v.to_string();
        }
        if let Some(v) = n.value_of("SURN") {
            s = v.to_string();
        }
        let kind = n.value_of("TYPE").unwrap_or("").to_lowercase();
        if kind == "aka" || kind == "nickname" {
            akas.push(format!("{g} {s}").trim().to_string());
        } else if kind == "birth" || kind == "maiden" {
            if maiden_name.is_empty() {
                maiden_name = s;
            }
        } else if given.is_empty() && surname.is_empty() {
            given = g;
            surname = s;
            nickname = n.value_of("NICK").unwrap_or("").to_string();
            prefix = n.value_of("NPFX").unwrap_or("").to_string();
            suffix = n.value_of("NSFX").unwrap_or("").to_string();
            if let Some(m) = n.value_of("_MARNM").or_else(|| n.value_of("_AKA")) {
                akas.push(m.to_string());
            }
        }
    }
    if nickname.is_empty() {
        nickname = r.value_of("NICK").unwrap_or("").to_string();
    }
    let facts = r
        .children
        .iter()
        .filter(|c| FACTS.contains(&c.tag.as_str()))
        .map(|c| {
            let mut event = event(c, src);
            let mut value = c.value.clone();
            if c.tag == "EVEN"
                && let Some(t) = c.value_of("TYPE")
            {
                value = if value.is_empty() {
                    t.to_string()
                } else {
                    format!("{t}: {value}")
                };
            }
            if value == "Y" {
                value.clear();
            }
            event.notes.retain(|n| !n.is_empty());
            Fact {
                kind: c.tag.clone(),
                value,
                event,
            }
        })
        .collect();
    let mut p = Person {
        xref: r.xref.clone().unwrap_or_default(),
        given,
        surname,
        akas,
        nickname,
        prefix,
        suffix,
        maiden_name,
        sex: match r.value_of("SEX") {
            Some("M") => Sex::Male,
            Some("F") => Sex::Female,
            _ => Sex::Unknown,
        },
        birth: r.child("BIRT").map(|e| event(e, src)),
        death: r.child("DEAT").map(|e| event(e, src)),
        facts,
        notes: r.all("NOTE").map(|n| n.value.clone()).collect(),
        sources: citations(r, src),
        famc: r
            .all("FAMC")
            .map(|f| f.value.trim_matches('@').to_string())
            .collect(),
        fams: r
            .all("FAMS")
            .map(|f| f.value.trim_matches('@').to_string())
            .collect(),
        ahn: r.value_of("_AHN").and_then(|v| v.trim().parse().ok()),
    };
    legacy::person(&mut p);
    p
}

fn family(r: &Record, src: &Sources) -> Family {
    let link = |t: &str| r.value_of(t).map(|v| v.trim_matches('@').to_string());
    Family {
        xref: r.xref.clone().unwrap_or_default(),
        husband: link("HUSB"),
        wife: link("WIFE"),
        children: r
            .all("CHIL")
            .map(|c| c.value.trim_matches('@').to_string())
            .collect(),
        marriage: r.child("MARR").map(|m| event(m, src)),
        notes: r.all("NOTE").map(|n| n.value.clone()).collect(),
    }
}

impl Tree {
    /// Reads a tree from GEDCOM text.
    ///
    /// # Errors
    /// When the text has no individuals.
    pub fn parse(name: &str, text: &str) -> Result<Self, String> {
        let records = parse_records(text);
        let src = source_titles(&records);
        let people: Vec<Person> = records
            .iter()
            .filter(|r| r.tag == "INDI")
            .map(|r| person(r, &src))
            .collect();
        let families: Vec<Family> = records
            .iter()
            .filter(|r| r.tag == "FAM")
            .map(|r| family(r, &src))
            .collect();
        if people.is_empty() {
            return Err("no individuals in the file".to_string());
        }
        let person_ix = people
            .iter()
            .enumerate()
            .map(|(i, p)| (p.xref.clone(), i))
            .collect();
        let family_ix = families
            .iter()
            .enumerate()
            .map(|(i, f)| (f.xref.clone(), i))
            .collect();
        Ok(Self {
            name: name.to_string(),
            people,
            families,
            person_ix,
            family_ix,
        })
    }

    /// Builds a tree from people and families that were stored apart (a database, say). The
    /// links people hold to their families (`famc`, `fams`) are worked out from the families.
    ///
    /// # Errors
    /// When there are no people.
    pub fn from_parts(
        name: &str,
        mut people: Vec<Person>,
        families: Vec<Family>,
    ) -> Result<Self, String> {
        if people.is_empty() {
            return Err("no individuals".to_string());
        }
        let person_ix: HashMap<String, usize> = people
            .iter()
            .enumerate()
            .map(|(i, p)| (p.xref.clone(), i))
            .collect();
        for p in &mut people {
            p.famc.clear();
            p.fams.clear();
        }
        for f in &families {
            for spouse in f.husband.iter().chain(f.wife.iter()) {
                if let Some(&i) = person_ix.get(spouse) {
                    people[i].fams.push(f.xref.clone());
                }
            }
            for child in &f.children {
                if let Some(&i) = person_ix.get(child) {
                    people[i].famc.push(f.xref.clone());
                }
            }
        }
        let family_ix = families
            .iter()
            .enumerate()
            .map(|(i, f)| (f.xref.clone(), i))
            .collect();
        Ok(Self {
            name: name.to_string(),
            people,
            families,
            person_ix,
            family_ix,
        })
    }

    #[must_use]
    pub fn people(&self) -> &[Person] {
        &self.people
    }

    #[must_use]
    pub fn families(&self) -> &[Family] {
        &self.families
    }

    #[must_use]
    pub fn person(&self, xref: &str) -> Option<&Person> {
        self.person_ix.get(xref).map(|&i| &self.people[i])
    }

    #[must_use]
    pub fn family(&self, xref: &str) -> Option<&Family> {
        self.family_ix.get(xref).map(|&i| &self.families[i])
    }

    /// The family a person was born into (the first, when the file lists several).
    #[must_use]
    pub fn parents_family(&self, p: &Person) -> Option<&Family> {
        p.famc.iter().find_map(|f| self.family(f))
    }

    /// Father and mother of a person.
    #[must_use]
    pub fn parents(&self, p: &Person) -> (Option<&Person>, Option<&Person>) {
        self.parents_family(p).map_or((None, None), |f| {
            (
                f.husband.as_deref().and_then(|x| self.person(x)),
                f.wife.as_deref().and_then(|x| self.person(x)),
            )
        })
    }

    /// Families a person is a spouse in, with the spouse and the children of each.
    #[must_use]
    pub fn unions(&self, p: &Person) -> Vec<(&Family, Option<&Person>, Vec<&Person>)> {
        p.fams
            .iter()
            .filter_map(|x| self.family(x))
            .map(|f| {
                let spouse = if f.husband.as_deref() == Some(&p.xref) {
                    &f.wife
                } else {
                    &f.husband
                };
                let spouse = spouse.as_deref().and_then(|x| self.person(x));
                let kids = f.children.iter().filter_map(|c| self.person(c)).collect();
                (f, spouse, kids)
            })
            .collect()
    }

    /// Whom the tree is about: the person numbered 1 in the file, otherwise the first one.
    #[must_use]
    pub fn default_root(&self) -> &Person {
        self.people
            .iter()
            .find(|p| p.ahn == Some(1))
            .unwrap_or(&self.people[0])
    }

    /// The ancestors of `root` up to `generations` generations back (0 is the root itself),
    /// ordered by Ahnentafel number. A missing parent leaves a gap in the numbering.
    #[must_use]
    pub fn ancestors(&self, root: &str, generations: u32) -> Vec<Slot> {
        let mut out = Vec::new();
        if self.person(root).is_none() {
            return out;
        }
        let mut level = vec![(1_u32, root.to_string())];
        for generation in 0..=generations {
            let mut next = Vec::new();
            for (ahn, xref) in level {
                if let Some(p) = self.person(&xref) {
                    let (father, mother) = self.parents(p);
                    if let Some(f) = father {
                        next.push((ahn * 2, f.xref.clone()));
                    }
                    if let Some(m) = mother {
                        next.push((ahn * 2 + 1, m.xref.clone()));
                    }
                }
                out.push(Slot {
                    ahn,
                    generation,
                    xref,
                });
            }
            level = next;
        }
        out
    }

    /// People whose name, aka or birth place contains every word of the query, best matches first.
    #[must_use]
    pub fn search(&self, query: &str, limit: usize) -> Vec<&Person> {
        let words: Vec<String> = fold(query).split_whitespace().map(str::to_string).collect();
        if words.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<(u8, &Person)> = self
            .people
            .iter()
            .filter_map(|p| {
                let name = fold(&p.full_name());
                let aka = p.akas.iter().map(|a| fold(a)).collect::<Vec<_>>().join(" ");
                let place = p
                    .birth
                    .as_ref()
                    .and_then(|b| b.place.as_deref())
                    .map(fold)
                    .unwrap_or_default();
                let extra = fold(&format!("{} {}", p.maiden_name, p.nickname));
                let all = format!("{name} {aka} {place} {extra}");
                if !words.iter().all(|w| all.contains(w.as_str())) {
                    return None;
                }
                let rank = u8::from(!words.iter().all(|w| name.contains(w.as_str())));
                Some((rank, p))
            })
            .collect();
        hits.sort_by_key(|(rank, p)| (*rank, p.birth_year().unwrap_or(i32::MAX)));
        hits.into_iter().take(limit).map(|(_, p)| p).collect()
    }
}
