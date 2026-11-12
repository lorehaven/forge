//! The notes the spreadsheet-to-GEDCOM conversion left (`Chrzestni: …`, `Akt urodzenia: …`,
//! `Zawód: …`) turned back into the fields they stand for. A file that already uses the proper
//! tags has none of these and passes through untouched.

use crate::model::{Citation, Event, Fact, Person};

fn split_names(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn citation(title: &str, detail: &str) -> Citation {
    Citation {
        title: title.to_string(),
        detail: detail.trim().to_string(),
    }
}

/// Moves the recognised notes of an event into its witnesses and sources; the rest stay notes.
pub fn event(e: &mut Event) {
    let mut keep = Vec::new();
    for note in std::mem::take(&mut e.notes) {
        if let Some(v) = note
            .strip_prefix("Swiadkowie: ")
            .or_else(|| note.strip_prefix("Chrzestni: "))
        {
            e.witnesses.extend(split_names(v));
        } else if let Some(v) = note.strip_prefix("Akt slubu: ") {
            e.sources.push(citation("Akt ślubu", v));
        } else {
            keep.push(note);
        }
    }
    e.notes = keep;
}

/// The same for a person's own notes, which may hold lines about their birth and death.
pub fn person(p: &mut Person) {
    let mut keep = Vec::new();
    for note in std::mem::take(&mut p.notes) {
        if let Some(v) = note.strip_prefix("Chrzestni: ") {
            p.birth
                .get_or_insert_with(Event::default)
                .witnesses
                .extend(split_names(v));
        } else if let Some(v) = note.strip_prefix("Akt urodzenia: ") {
            p.birth
                .get_or_insert_with(Event::default)
                .sources
                .push(citation("Akt urodzenia", v));
        } else if let Some(v) = note.strip_prefix("Akt zgonu: ") {
            p.death
                .get_or_insert_with(Event::default)
                .sources
                .push(citation("Akt zgonu", v));
        } else if let Some(v) = note.strip_prefix("Zawód: ") {
            p.facts.push(Fact {
                kind: "OCCU".to_string(),
                value: v.trim().to_string(),
                event: Event::default(),
            });
        } else if let Some(v) = note.strip_prefix("Zrodlo: arkusz ") {
            p.sources.push(citation("Arkusz kalkulacyjny", v));
        } else {
            keep.push(note);
        }
    }
    p.notes = keep;
    for e in p.birth.iter_mut().chain(p.death.iter_mut()) {
        event(e);
    }
}
