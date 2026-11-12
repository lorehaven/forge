//! Changing what is stored. Every change is one transaction on a freshly loaded tree. A person or a
//! family is one record however many others point at it, so a change to a wedding, a name or a
//! parent shows up wherever it is read; links between people live in the families, and adding or
//! removing a relative rewrites just the families concerned.

use genevia_core::date::{from_input, to_input};
use genevia_core::{Citation, Date, Event, Fact, Family, Person, Sex, Tree};
use quench_pg::Transaction;

use super::{Rows, Store, StoreError, card, load, store, write_rows};
use genevia_core::wire::{EventEdit, FACT_KINDS, FactEdit, FamilyEdit, FamilyForm, PersonEdit};

fn invalid(field: &str, message: &str) -> StoreError {
    StoreError::Invalid {
        field: field.to_string(),
        message: message.to_string(),
    }
}

// ---------------------------------------------------------------------------------------------
// Text in forms and values in the model
// ---------------------------------------------------------------------------------------------

fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

fn items(text: &str) -> Vec<String> {
    text.split(';')
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

fn sources_from(text: &str) -> Vec<Citation> {
    lines(text)
        .into_iter()
        .map(|l| match l.split_once(": ") {
            Some((t, d)) => Citation {
                title: t.trim().to_string(),
                detail: d.trim().to_string(),
            },
            None => Citation {
                title: l,
                detail: String::new(),
            },
        })
        .collect()
}

fn sources_text(list: &[Citation]) -> String {
    list.iter()
        .map(|c| {
            if c.detail.is_empty() {
                c.title.clone()
            } else {
                format!("{}: {}", c.title, c.detail)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn event_edit(e: Option<&Event>) -> EventEdit {
    let Some(e) = e else {
        return EventEdit::default();
    };
    EventEdit {
        date: e
            .date
            .as_ref()
            .map(|d| to_input(&d.raw))
            .unwrap_or_default(),
        place: e.place.clone().unwrap_or_default(),
        cause: e.cause.clone().unwrap_or_default(),
        witnesses: e.witnesses.join("; "),
        sources: sources_text(&e.sources),
        notes: e.notes.join("\n"),
    }
}

fn date_of(text: &str, field: &str) -> Result<Option<Date>, StoreError> {
    let raw = from_input(text).map_err(|m| invalid(field, &m))?;
    Ok(if raw.is_empty() {
        None
    } else {
        Some(Date::parse(&raw))
    })
}

fn event_from(e: &EventEdit, field: &str) -> Result<Option<Event>, StoreError> {
    let event = Event {
        date: date_of(&e.date, &format!("{field}.date"))?,
        place: Some(e.place.trim().to_string()).filter(|p| !p.is_empty()),
        notes: lines(&e.notes),
        cause: Some(e.cause.trim().to_string()).filter(|c| !c.is_empty()),
        witnesses: items(&e.witnesses),
        sources: sources_from(&e.sources),
    };
    Ok(if event.is_empty() { None } else { Some(event) })
}

fn person_to_edit(p: &Person) -> PersonEdit {
    PersonEdit {
        given: p.given.clone(),
        surname: p.surname.clone(),
        maiden_name: p.maiden_name.clone(),
        nickname: p.nickname.clone(),
        prefix: p.prefix.clone(),
        suffix: p.suffix.clone(),
        akas: p.akas.join("; "),
        sex: match p.sex {
            Sex::Male => "M",
            Sex::Female => "K",
            Sex::Unknown => "",
        }
        .to_string(),
        birth: event_edit(p.birth.as_ref()),
        death: event_edit(p.death.as_ref()),
        facts: p
            .facts
            .iter()
            .map(|f| FactEdit {
                kind: f.kind.clone(),
                value: f.value.clone(),
                date: f
                    .event
                    .date
                    .as_ref()
                    .map(|d| to_input(&d.raw))
                    .unwrap_or_default(),
                place: f.event.place.clone().unwrap_or_default(),
                sources: sources_text(&f.event.sources),
                notes: f.event.notes.join("\n"),
            })
            .collect(),
        notes: p.notes.join("\n"),
        sources: sources_text(&p.sources),
    }
}

/// Builds a person from a form, keeping what the form does not carry (the Ahnentafel number).
fn person_from(xref: &str, e: &PersonEdit, base: Option<&Person>) -> Result<Person, StoreError> {
    if e.given.trim().is_empty() && e.surname.trim().is_empty() {
        return Err(invalid("given", "Wpisz imię lub nazwisko."));
    }
    let mut facts = Vec::new();
    for (i, f) in e.facts.iter().enumerate() {
        if f.value.trim().is_empty() && f.date.trim().is_empty() && f.place.trim().is_empty() {
            continue;
        }
        if !FACT_KINDS.iter().any(|(k, _)| *k == f.kind) {
            return Err(invalid(
                &format!("facts.{i}.kind"),
                "Nieznany rodzaj faktu.",
            ));
        }
        let event = Event {
            date: date_of(&f.date, &format!("facts.{i}.date"))?,
            place: Some(f.place.trim().to_string()).filter(|p| !p.is_empty()),
            notes: lines(&f.notes),
            cause: None,
            witnesses: Vec::new(),
            sources: sources_from(&f.sources),
        };
        facts.push(Fact {
            kind: f.kind.clone(),
            value: f.value.trim().to_string(),
            event,
        });
    }
    Ok(Person {
        xref: xref.to_string(),
        given: e.given.trim().to_string(),
        surname: e.surname.trim().to_string(),
        akas: items(&e.akas),
        nickname: e.nickname.trim().to_string(),
        prefix: e.prefix.trim().to_string(),
        suffix: e.suffix.trim().to_string(),
        maiden_name: e.maiden_name.trim().to_string(),
        sex: match e.sex.as_str() {
            "M" => Sex::Male,
            "K" | "F" => Sex::Female,
            _ => Sex::Unknown,
        },
        birth: event_from(&e.birth, "birth")?,
        death: event_from(&e.death, "death")?,
        facts,
        notes: lines(&e.notes),
        sources: sources_from(&e.sources),
        famc: Vec::new(),
        fams: Vec::new(),
        ahn: base.and_then(|b| b.ahn),
    })
}

// ---------------------------------------------------------------------------------------------
// Writing records
// ---------------------------------------------------------------------------------------------

async fn clear_owner(
    tx: &mut Transaction<'_>,
    s: &str,
    tree: &str,
    owner: &str,
) -> Result<(), StoreError> {
    for table in ["notes", "citations", "witnesses"] {
        tx.execute(
            &format!("DELETE FROM {s}.{table} WHERE tree_id = $1 AND owner = $2"),
            &[&tree, &owner],
        )
        .await?;
    }
    Ok(())
}

async fn position(
    tx: &mut Transaction<'_>,
    s: &str,
    table: &str,
    tree: &str,
    xref: &str,
) -> Result<usize, StoreError> {
    let sql = format!("SELECT pos FROM {s}.{table} WHERE tree_id = $1 AND xref = $2");
    let at = tx
        .query(&sql, &[&tree, &xref])
        .await?
        .first()
        .map(|r| r.get::<i32, _>("pos"))
        .transpose()?;
    if let Some(p) = at {
        return Ok(usize::try_from(p).unwrap_or(0));
    }
    let next = tx
        .query(
            &format!(
                "SELECT COALESCE(max(pos) + 1, 0) AS next FROM {s}.{table} WHERE tree_id = $1"
            ),
            &[&tree],
        )
        .await?;
    let n: i32 = next[0].get("next")?;
    Ok(usize::try_from(n).unwrap_or(0))
}

async fn put_person(
    tx: &mut Transaction<'_>,
    s: &str,
    tree: &str,
    p: &Person,
) -> Result<(), StoreError> {
    let pos = position(tx, s, "people", tree, &p.xref).await?;
    clear_owner(tx, s, tree, &p.xref).await?;
    tx.execute(
        &format!("DELETE FROM {s}.people WHERE tree_id = $1 AND xref = $2"),
        &[&tree, &p.xref],
    )
    .await?;
    let mut rows = Rows::default();
    rows.person(pos, p);
    write_rows(tx, s, tree, rows).await
}

async fn put_family(
    tx: &mut Transaction<'_>,
    s: &str,
    tree: &str,
    f: &Family,
) -> Result<(), StoreError> {
    let pos = position(tx, s, "families", tree, &f.xref).await?;
    clear_owner(tx, s, tree, &f.xref).await?;
    tx.execute(
        &format!("DELETE FROM {s}.families WHERE tree_id = $1 AND xref = $2"),
        &[&tree, &f.xref],
    )
    .await?;
    let mut rows = Rows::default();
    rows.family(pos, f);
    write_rows(tx, s, tree, rows).await
}

async fn drop_family(
    tx: &mut Transaction<'_>,
    s: &str,
    tree: &str,
    xref: &str,
) -> Result<(), StoreError> {
    clear_owner(tx, s, tree, xref).await?;
    tx.execute(
        &format!("DELETE FROM {s}.families WHERE tree_id = $1 AND xref = $2"),
        &[&tree, &xref],
    )
    .await?;
    Ok(())
}

fn next_xref<'a>(taken: impl Iterator<Item = &'a str>, prefix: &str) -> String {
    let highest = taken
        .filter_map(|x| x.strip_prefix(prefix).and_then(|n| n.parse::<u64>().ok()))
        .max()
        .unwrap_or(0);
    format!("{prefix}{}", highest + 1)
}

async fn fresh(store: &Store, id: &str) -> Result<Tree, StoreError> {
    load(store, id)
        .await?
        .ok_or_else(|| StoreError::BadFile(format!("no tree {id}")))
}

/// Forgets the cached copy and notes the change.
async fn done(store: &Store, tx: &mut Transaction<'_>, id: &str) -> Result<(), StoreError> {
    tx.execute(
        &format!(
            "UPDATE {}.trees SET updated_at = NOW() WHERE id = $1",
            store.schema
        ),
        &[&id],
    )
    .await?;
    Ok(())
}

fn forget(store: &Store, id: &str) {
    if let Ok(mut m) = store.trees.write() {
        m.remove(id);
    }
}

// ---------------------------------------------------------------------------------------------
// People
// ---------------------------------------------------------------------------------------------

/// # Errors
/// When the database fails.
pub async fn person_edit(tree_id: &str, xref: &str) -> Result<Option<PersonEdit>, StoreError> {
    Ok(super::tree(tree_id)
        .await?
        .and_then(|t| t.person(xref).map(person_to_edit)))
}

/// # Errors
/// When the form is not valid, the person is not there, or the database fails.
pub async fn save_person(tree_id: &str, xref: &str, edit: &PersonEdit) -> Result<(), StoreError> {
    let store = store()?;
    let tree = fresh(store, tree_id).await?;
    let base = tree
        .person(xref)
        .ok_or_else(|| invalid("xref", "Nie ma takiej osoby."))?;
    let person = person_from(xref, edit, Some(base))?;
    let mut conn = store.pool()?.get().await?;
    let mut tx = conn.begin().await?;
    put_person(&mut tx, &store.schema, tree_id, &person).await?;
    done(store, &mut tx, tree_id).await?;
    tx.commit().await?;
    forget(store, tree_id);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Families
// ---------------------------------------------------------------------------------------------

/// # Errors
/// When the database fails.
pub async fn family_edit(tree_id: &str, xref: &str) -> Result<Option<FamilyForm>, StoreError> {
    let Some(tree) = super::tree(tree_id).await? else {
        return Ok(None);
    };
    let Some(f) = tree.family(xref) else {
        return Ok(None);
    };
    let pick = |x: &Option<String>| {
        x.as_deref()
            .and_then(|x| tree.person(x))
            .map(|p| card(&tree, p))
    };
    Ok(Some(FamilyForm {
        xref: f.xref.clone(),
        edit: FamilyEdit {
            husband: f.husband.clone().unwrap_or_default(),
            wife: f.wife.clone().unwrap_or_default(),
            children: f.children.clone(),
            marriage: event_edit(f.marriage.as_ref()),
            notes: f.notes.join("\n"),
        },
        husband: pick(&f.husband),
        wife: pick(&f.wife),
        children: f
            .children
            .iter()
            .filter_map(|c| tree.person(c))
            .map(|p| card(&tree, p))
            .collect(),
    }))
}

fn family_from(tree: &Tree, xref: &str, e: &FamilyEdit) -> Result<Family, StoreError> {
    let known = |x: &str, field: &str| {
        if x.is_empty() || tree.person(x).is_some() {
            Ok(())
        } else {
            Err(invalid(field, "Nie ma takiej osoby w drzewie."))
        }
    };
    known(&e.husband, "husband")?;
    known(&e.wife, "wife")?;
    if !e.husband.is_empty() && e.husband == e.wife {
        return Err(invalid("wife", "Mąż i żona to ta sama osoba."));
    }
    let mut children: Vec<String> = Vec::new();
    for c in &e.children {
        known(c, "children")?;
        if *c == e.husband || *c == e.wife {
            return Err(invalid(
                "children",
                "Dziecko nie może być własnym rodzicem.",
            ));
        }
        if !children.contains(c) {
            children.push(c.clone());
        }
    }
    Ok(Family {
        xref: xref.to_string(),
        husband: Some(e.husband.clone()).filter(|x| !x.is_empty()),
        wife: Some(e.wife.clone()).filter(|x| !x.is_empty()),
        children,
        marriage: event_from(&e.marriage, "marriage")?,
        notes: lines(&e.notes),
    })
}

/// # Errors
/// When the form is not valid, or the database fails.
pub async fn save_family(tree_id: &str, xref: &str, edit: &FamilyEdit) -> Result<(), StoreError> {
    let store = store()?;
    let tree = fresh(store, tree_id).await?;
    if tree.family(xref).is_none() {
        return Err(invalid("xref", "Nie ma takiej rodziny."));
    }
    let family = family_from(&tree, xref, edit)?;
    let mut conn = store.pool()?.get().await?;
    let mut tx = conn.begin().await?;
    put_family(&mut tx, &store.schema, tree_id, &family).await?;
    done(store, &mut tx, tree_id).await?;
    tx.commit().await?;
    forget(store, tree_id);
    Ok(())
}

/// # Errors
/// When the database fails.
pub async fn delete_family(tree_id: &str, xref: &str) -> Result<(), StoreError> {
    let store = store()?;
    let mut conn = store.pool()?.get().await?;
    let mut tx = conn.begin().await?;
    drop_family(&mut tx, &store.schema, tree_id, xref).await?;
    done(store, &mut tx, tree_id).await?;
    tx.commit().await?;
    forget(store, tree_id);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Relatives
// ---------------------------------------------------------------------------------------------

/// What is left of a family when it has lost someone: nothing worth keeping when no spouse is
/// left, or when it has no children and is missing a spouse.
const fn worth_keeping(f: &Family) -> bool {
    !(f.husband.is_none() && f.wife.is_none())
        && !(f.children.is_empty() && (f.husband.is_none() || f.wife.is_none()))
}

/// # Errors
/// When the form is not valid, the request does not fit the tree, or the database fails.
pub async fn add_relative(
    tree_id: &str,
    of: &str,
    relation: &str,
    existing: &str,
    person: &PersonEdit,
    family: &str,
) -> Result<String, StoreError> {
    let store = store()?;
    let tree = fresh(store, tree_id).await?;
    let anchor = tree
        .person(of)
        .ok_or_else(|| invalid("of", "Nie ma takiej osoby."))?;

    // Who is being added: someone already there, or a new person.
    let mut new_person: Option<Person> = None;
    let who = if existing.is_empty() {
        let xref = next_xref(tree.people().iter().map(|p| p.xref.as_str()), "I");
        let mut edit = person.clone();
        if edit.sex.is_empty() {
            edit.sex = match relation {
                "father" => "M",
                "mother" => "K",
                _ => "",
            }
            .to_string();
        }
        let p = person_from(&xref, &edit, None)?;
        new_person = Some(p);
        xref
    } else {
        if tree.person(existing).is_none() {
            return Err(invalid("existing", "Nie ma takiej osoby w drzewie."));
        }
        if existing == of {
            return Err(invalid("existing", "Osoba nie może być własnym krewnym."));
        }
        existing.to_string()
    };

    let new_family_xref = || next_xref(tree.families().iter().map(|f| f.xref.as_str()), "F");
    let empty = |xref: String| Family {
        xref,
        husband: None,
        wife: None,
        children: Vec::new(),
        marriage: None,
        notes: Vec::new(),
    };
    let anchor_is_wife = anchor.sex == Sex::Female;
    let mut touched: Vec<Family> = Vec::new();

    match relation {
        "father" | "mother" => {
            let mut f = tree.parents_family(anchor).cloned().unwrap_or_else(|| {
                let mut f = empty(new_family_xref());
                f.children.push(of.to_string());
                f
            });
            let slot = if relation == "father" {
                &mut f.husband
            } else {
                &mut f.wife
            };
            if slot.is_some() {
                return Err(invalid(
                    "relation",
                    if relation == "father" {
                        "Ta osoba ma już ojca."
                    } else {
                        "Ta osoba ma już matkę."
                    },
                ));
            }
            *slot = Some(who.clone());
            touched.push(f);
        }
        "spouse" => {
            // A family of theirs that is missing its other half takes the new person.
            let open = tree
                .unions(anchor)
                .into_iter()
                .map(|(f, _, _)| f)
                .find(|f| {
                    if anchor_is_wife {
                        f.husband.is_none()
                    } else {
                        f.wife.is_none()
                    }
                });
            let mut f = open.cloned().unwrap_or_else(|| {
                let mut f = empty(new_family_xref());
                if anchor_is_wife {
                    f.wife = Some(of.to_string());
                } else {
                    f.husband = Some(of.to_string());
                }
                f
            });
            if anchor_is_wife {
                f.husband = Some(who.clone());
            } else {
                f.wife = Some(who.clone());
            }
            touched.push(f);
        }
        "child" => {
            let unions: Vec<&Family> = tree.unions(anchor).into_iter().map(|(f, _, _)| f).collect();
            let mut f = if !family.is_empty() {
                unions
                    .iter()
                    .find(|f| f.xref == family)
                    .map(|f| (*f).clone())
                    .ok_or_else(|| invalid("family", "To małżeństwo nie należy do tej osoby."))?
            } else if unions.len() == 1 {
                unions[0].clone()
            } else if unions.is_empty() {
                let mut f = empty(new_family_xref());
                if anchor_is_wife {
                    f.wife = Some(of.to_string());
                } else {
                    f.husband = Some(of.to_string());
                }
                f
            } else {
                return Err(invalid(
                    "family",
                    "Wybierz małżeństwo, do którego należy dziecko.",
                ));
            };
            if f.husband.as_deref() == Some(&who) || f.wife.as_deref() == Some(&who) {
                return Err(invalid(
                    "existing",
                    "Dziecko nie może być własnym rodzicem.",
                ));
            }
            if !f.children.contains(&who) {
                f.children.push(who.clone());
            }
            touched.push(f);
        }
        _ => return Err(invalid("relation", "Nieznany rodzaj pokrewieństwa.")),
    }

    let mut conn = store.pool()?.get().await?;
    let mut tx = conn.begin().await?;
    if let Some(p) = &new_person {
        put_person(&mut tx, &store.schema, tree_id, p).await?;
    }
    for f in &touched {
        put_family(&mut tx, &store.schema, tree_id, f).await?;
    }
    done(store, &mut tx, tree_id).await?;
    tx.commit().await?;
    forget(store, tree_id);
    Ok(who)
}

/// # Errors
/// When the person is not there, or the database fails.
pub async fn delete_person(tree_id: &str, xref: &str) -> Result<(), StoreError> {
    let store = store()?;
    let tree = fresh(store, tree_id).await?;
    if tree.person(xref).is_none() {
        return Err(invalid("xref", "Nie ma takiej osoby."));
    }
    let mut conn = store.pool()?.get().await?;
    let mut tx = conn.begin().await?;
    let s = &store.schema;
    // Every family the person was in loses them; one left empty goes too.
    for f in tree.families() {
        let involved = f.husband.as_deref() == Some(xref)
            || f.wife.as_deref() == Some(xref)
            || f.children.iter().any(|c| c == xref);
        if !involved {
            continue;
        }
        let mut f = f.clone();
        if f.husband.as_deref() == Some(xref) {
            f.husband = None;
        }
        if f.wife.as_deref() == Some(xref) {
            f.wife = None;
        }
        f.children.retain(|c| c != xref);
        if worth_keeping(&f) {
            put_family(&mut tx, s, tree_id, &f).await?;
        } else {
            drop_family(&mut tx, s, tree_id, &f.xref).await?;
        }
    }
    clear_owner(&mut tx, s, tree_id, xref).await?;
    tx.execute(
        &format!("DELETE FROM {s}.people WHERE tree_id = $1 AND xref = $2"),
        &[&tree_id, &xref],
    )
    .await?;
    done(store, &mut tx, tree_id).await?;
    tx.commit().await?;
    forget(store, tree_id);
    Ok(())
}
