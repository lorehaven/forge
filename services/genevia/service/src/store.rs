//! The trees, kept in Postgres (schema `genevia`, created by the migrations in `migrations/`) and
//! read into `genevia_core::Tree`s on demand. A tree is loaded the first time it is asked for and
//! kept until it is imported again. Server only.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use genevia_core::text::fold;
use genevia_core::{Citation, Date, Event, Fact, Family, Person, Sex, Tree};
use quench_db::prelude::Db;
use quench_pg::{PgArray, Pool, Row};

use genevia_core::wire::{
    AncestorSlot, Ancestry, Card, CitationView, EventView, FactView, PersonDetail, TreeInfo,
    UnionView,
};

#[derive(Debug, quench_core::Error)]
pub enum StoreError {
    #[error("genevia needs Postgres: set DATABASE_URL")]
    NotPostgres,
    #[error("{0}")]
    BadFile(String),
    /// A form field that cannot be saved as it is.
    #[error("{field}: {message}")]
    Invalid { field: String, message: String },
    #[error(transparent)]
    Sql(#[from] quench_pg::Error),
}

pub mod edit;

struct Store {
    db: Db,
    schema: String,
    trees: RwLock<HashMap<String, Arc<Tree>>>,
}

static STORE: OnceLock<Store> = OnceLock::new();

/// The schema the migrations made (`DB_SCHEMA`, as in the other forge services).
fn schema_name() -> String {
    quench_core::env::get_or("DB_SCHEMA", "genevia")
}

/// Hands the database to the store. Call once, at start-up.
///
/// # Errors
/// When the database is not Postgres, or the store is already set up.
pub fn init(db: Db) -> Result<(), StoreError> {
    if !matches!(db, Db::Postgres(_)) {
        return Err(StoreError::NotPostgres);
    }
    let schema = schema_name();
    STORE
        .set(Store {
            db,
            schema,
            trees: RwLock::new(HashMap::new()),
        })
        .map_err(|_| StoreError::BadFile("the store is already set up".to_string()))
}

fn store() -> Result<&'static Store, StoreError> {
    STORE.get().ok_or(StoreError::NotPostgres)
}

impl Store {
    const fn pool(&self) -> Result<&Pool, StoreError> {
        match &self.db {
            Db::Postgres(pg) => Ok(pg.pool()),
            Db::InMemory(_) => Err(StoreError::NotPostgres),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------------------------

fn event(
    date: Option<String>,
    place: Option<String>,
    notes: Vec<String>,
    cause: Option<String>,
    witnesses: Vec<String>,
    sources: Vec<Citation>,
) -> Option<Event> {
    let date = date
        .filter(|d| !d.trim().is_empty())
        .map(|d| Date::parse(&d));
    let place = place.filter(|p| !p.trim().is_empty());
    let cause = cause.filter(|c| !c.trim().is_empty());
    let e = Event {
        date,
        place,
        notes,
        cause,
        witnesses,
        sources,
    };
    if e.is_empty() { None } else { Some(e) }
}

type NoteKey = (String, String);

async fn load(store: &Store, id: &str) -> Result<Option<Tree>, StoreError> {
    let pool = store.pool()?;
    let s = &store.schema;
    let Some(row) = pool
        .query_opt(&format!("SELECT name FROM {s}.trees WHERE id = $1"), &[&id])
        .await?
    else {
        return Ok(None);
    };
    let name: String = row.get("name")?;

    let mut notes: HashMap<NoteKey, Vec<String>> = HashMap::new();
    for r in pool
        .query(&format!("SELECT owner, kind, text FROM {s}.notes WHERE tree_id = $1 ORDER BY owner, kind, seq"), &[&id])
        .await?
    {
        notes.entry((r.get("owner")?, r.get("kind")?)).or_default().push(r.get("text")?);
    }
    let mut take = |owner: &str, kind: &str| {
        notes
            .remove(&(owner.to_string(), kind.to_string()))
            .unwrap_or_default()
    };

    let mut akas: HashMap<String, Vec<String>> = HashMap::new();
    for r in pool
        .query(
            &format!(
                "SELECT xref, name FROM {s}.person_akas WHERE tree_id = $1 ORDER BY xref, seq"
            ),
            &[&id],
        )
        .await?
    {
        akas.entry(r.get("xref")?).or_default().push(r.get("name")?);
    }

    // Citations and witnesses, by (owner, kind); facts, by owner.
    let mut cites: HashMap<NoteKey, Vec<Citation>> = HashMap::new();
    for r in pool
        .query(
            &format!("SELECT owner, kind, title, detail FROM {s}.citations WHERE tree_id = $1 ORDER BY owner, kind, seq"),
            &[&id],
        )
        .await?
    {
        cites
            .entry((r.get("owner")?, r.get("kind")?))
            .or_default()
            .push(Citation { title: r.get("title")?, detail: r.get("detail")? });
    }
    let mut witnesses: HashMap<NoteKey, Vec<String>> = HashMap::new();
    for r in pool
        .query(&format!("SELECT owner, kind, name FROM {s}.witnesses WHERE tree_id = $1 ORDER BY owner, kind, seq"), &[&id])
        .await?
    {
        witnesses.entry((r.get("owner")?, r.get("kind")?)).or_default().push(r.get("name")?);
    }
    let mut facts: HashMap<String, Vec<Fact>> = HashMap::new();
    for r in pool
        .query(
            &format!("SELECT owner, seq, kind, value, date, place, cause, note FROM {s}.events WHERE tree_id = $1 ORDER BY owner, seq"),
            &[&id],
        )
        .await?
    {
        let owner: String = r.get("owner")?;
        let seq: i32 = r.get("seq")?;
        let note: Option<String> = r.get("note")?;
        let notes = note.filter(|n| !n.is_empty()).map(|n| n.split('\n').map(str::to_string).collect()).unwrap_or_default();
        let sources = cites.remove(&(owner.clone(), format!("fact:{seq}"))).unwrap_or_default();
        let ev = event(r.get("date")?, r.get("place")?, notes, r.get("cause")?, Vec::new(), sources).unwrap_or_default();
        facts.entry(owner).or_default().push(Fact { kind: r.get("kind")?, value: r.get("value")?, event: ev });
    }

    let mut people = Vec::new();
    for r in pool
        .query(
            &format!(
                "SELECT xref, given, surname, sex, ahn, birth_date, birth_place, death_date, death_place, \
                 nickname, prefix, suffix, maiden_name, death_cause \
                 FROM {s}.people WHERE tree_id = $1 ORDER BY pos"
            ),
            &[&id],
        )
        .await?
    {
        let xref: String = r.get("xref")?;
        let ahn: Option<i32> = r.get("ahn")?;
        people.push(Person {
            given: r.get("given")?,
            surname: r.get("surname")?,
            akas: akas.remove(&xref).unwrap_or_default(),
            nickname: r.get("nickname")?,
            prefix: r.get("prefix")?,
            suffix: r.get("suffix")?,
            maiden_name: r.get("maiden_name")?,
            sex: match r.get::<String, _>("sex")?.as_str() {
                "M" => Sex::Male,
                "F" => Sex::Female,
                _ => Sex::Unknown,
            },
            birth: event(
                r.get("birth_date")?,
                r.get("birth_place")?,
                take(&xref, "birth"),
                None,
                witnesses.remove(&(xref.clone(), "BIRT".to_string())).unwrap_or_default(),
                cites.remove(&(xref.clone(), "BIRT".to_string())).unwrap_or_default(),
            ),
            death: event(
                r.get("death_date")?,
                r.get("death_place")?,
                take(&xref, "death"),
                r.get("death_cause")?,
                witnesses.remove(&(xref.clone(), "DEAT".to_string())).unwrap_or_default(),
                cites.remove(&(xref.clone(), "DEAT".to_string())).unwrap_or_default(),
            ),
            facts: facts.remove(&xref).unwrap_or_default(),
            sources: cites.remove(&(xref.clone(), "person".to_string())).unwrap_or_default(),
            notes: take(&xref, "person"),
            famc: Vec::new(),
            fams: Vec::new(),
            ahn: ahn.and_then(|n| u32::try_from(n).ok()),
            xref,
        });
    }

    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for r in pool
        .query(
            &format!("SELECT family_xref, child_xref FROM {s}.family_children WHERE tree_id = $1 ORDER BY family_xref, seq"),
            &[&id],
        )
        .await?
    {
        children.entry(r.get("family_xref")?).or_default().push(r.get("child_xref")?);
    }
    let mut families = Vec::new();
    for r in pool
        .query(
            &format!("SELECT xref, husband, wife, marriage_date, marriage_place FROM {s}.families WHERE tree_id = $1 ORDER BY pos"),
            &[&id],
        )
        .await?
    {
        let xref: String = r.get("xref")?;
        families.push(Family {
            husband: r.get("husband")?,
            wife: r.get("wife")?,
            children: children.remove(&xref).unwrap_or_default(),
            marriage: event(
                r.get("marriage_date")?,
                r.get("marriage_place")?,
                take(&xref, "marriage"),
                None,
                witnesses.remove(&(xref.clone(), "MARR".to_string())).unwrap_or_default(),
                cites.remove(&(xref.clone(), "MARR".to_string())).unwrap_or_default(),
            ),
            notes: take(&xref, "family"),
            xref,
        });
    }
    Tree::from_parts(&name, people, families)
        .map(Some)
        .map_err(StoreError::BadFile)
}

async fn tree(id: &str) -> Result<Option<Arc<Tree>>, StoreError> {
    let store = store()?;
    if let Some(t) = store.trees.read().ok().and_then(|m| m.get(id).cloned()) {
        return Ok(Some(t));
    }
    let Some(loaded) = load(store, id).await? else {
        return Ok(None);
    };
    let loaded = Arc::new(loaded);
    if let Ok(mut m) = store.trees.write() {
        m.insert(id.to_string(), loaded.clone());
    }
    Ok(Some(loaded))
}

fn card(tree: &Tree, p: &Person) -> Card {
    let shown = |e: &Option<Event>| {
        e.as_ref()
            .and_then(|e| e.date.as_ref())
            .map(Date::display)
            .unwrap_or_default()
    };
    Card {
        xref: p.xref.clone(),
        name: p.full_name(),
        given: p.given.clone(),
        surname: p.surname.clone(),
        born: shown(&p.birth),
        died: shown(&p.death),
        birth_year: p.birth_year(),
        death_year: p.death_year(),
        birth_place: p
            .birth
            .as_ref()
            .and_then(|e| e.place.clone())
            .unwrap_or_default(),
        sex: match p.sex {
            Sex::Male => "M",
            Sex::Female => "F",
            Sex::Unknown => "",
        }
        .to_string(),
        has_parents: tree
            .parents_family(p)
            .is_some_and(|f| f.husband.is_some() || f.wife.is_some()),
    }
}

fn citation_view(c: &Citation) -> CitationView {
    CitationView {
        title: c.title.clone(),
        detail: c.detail.clone(),
    }
}

fn event_view(e: &Event) -> EventView {
    EventView {
        date: e.date.as_ref().map(Date::display).unwrap_or_default(),
        place: e.place.clone().unwrap_or_default(),
        notes: e.notes.clone(),
        cause: e.cause.clone().unwrap_or_default(),
        witnesses: e.witnesses.clone(),
        sources: e.sources.iter().map(citation_view).collect(),
    }
}

fn count(row: &Row, column: &str) -> Result<u32, StoreError> {
    let n: i64 = row.get(column)?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

/// All the trees, by name.
///
/// # Errors
/// When the database fails.
pub async fn trees() -> Result<Vec<TreeInfo>, StoreError> {
    let store = store()?;
    let s = &store.schema;
    let sql = format!(
        "SELECT t.id, t.name, \
           (SELECT count(*) FROM {s}.people p WHERE p.tree_id = t.id) AS people, \
           (SELECT count(*) FROM {s}.families f WHERE f.tree_id = t.id) AS families, \
           r.xref AS root, r.given, r.surname \
         FROM {s}.trees t \
         LEFT JOIN LATERAL ( \
           SELECT xref, given, surname FROM {s}.people p WHERE p.tree_id = t.id \
           ORDER BY COALESCE(p.ahn = 1, false) DESC, p.pos LIMIT 1 \
         ) r ON true \
         ORDER BY t.name"
    );
    let mut out = Vec::new();
    for r in store.pool()?.query(&sql, &[]).await? {
        let (given, surname): (String, String) = (r.get("given")?, r.get("surname")?);
        out.push(TreeInfo {
            id: r.get("id")?,
            name: r.get("name")?,
            people: count(&r, "people")?,
            families: count(&r, "families")?,
            root: r.get::<Option<String>, _>("root")?.unwrap_or_default(),
            root_name: format!("{given} {surname}").trim().to_string(),
        });
    }
    Ok(out)
}

/// # Errors
/// When the database fails.
pub async fn ancestry(
    tree_id: &str,
    root: &str,
    generations: u32,
) -> Result<Option<Ancestry>, StoreError> {
    let Some(tree) = tree(tree_id).await? else {
        return Ok(None);
    };
    let Some(root) = (if root.is_empty() {
        Some(tree.default_root())
    } else {
        tree.person(root)
    }) else {
        return Ok(None);
    };
    let all = tree.ancestors(&root.xref, 30);
    let depth = all.iter().map(|s| s.generation).max().unwrap_or(0);
    let slots = all
        .into_iter()
        .filter(|s| s.generation <= generations)
        .filter_map(|s| {
            let card = card(&tree, tree.person(&s.xref)?);
            Some(AncestorSlot {
                ahn: s.ahn,
                generation: s.generation,
                card,
            })
        })
        .collect();
    Ok(Some(Ancestry {
        tree: tree_id.to_string(),
        root: card(&tree, root),
        slots,
        depth,
    }))
}

/// # Errors
/// When the database fails.
pub async fn person(tree_id: &str, xref: &str) -> Result<Option<PersonDetail>, StoreError> {
    let Some(tree) = tree(tree_id).await? else {
        return Ok(None);
    };
    let Some(p) = tree.person(xref) else {
        return Ok(None);
    };
    let (father, mother) = tree.parents(p);
    let unions = tree
        .unions(p)
        .into_iter()
        .map(|(fam, spouse, kids)| UnionView {
            family: fam.xref.clone(),
            marriage: fam.marriage.as_ref().map(event_view),
            spouse: spouse.map(|s| card(&tree, s)),
            children: kids.into_iter().map(|k| card(&tree, k)).collect(),
        })
        .collect();
    Ok(Some(PersonDetail {
        card: card(&tree, p),
        akas: p.akas.clone(),
        nickname: p.nickname.clone(),
        prefix: p.prefix.clone(),
        suffix: p.suffix.clone(),
        maiden_name: p.maiden_name.clone(),
        birth: p.birth.as_ref().map(event_view),
        death: p.death.as_ref().map(event_view),
        father: father.map(|f| card(&tree, f)),
        mother: mother.map(|m| card(&tree, m)),
        parents_family: tree.parents_family(p).map(|f| f.xref.clone()),
        unions,
        facts: p
            .facts
            .iter()
            .map(|f| FactView {
                kind: f.kind.clone(),
                value: f.value.clone(),
                event: event_view(&f.event),
            })
            .collect(),
        notes: p.notes.clone(),
        sources: p.sources.iter().map(citation_view).collect(),
        ahn: p.ahn,
        editable: false,
    }))
}

/// # Errors
/// When the database fails.
pub async fn search(tree_id: &str, query: &str) -> Result<Option<Vec<Card>>, StoreError> {
    let Some(tree) = tree(tree_id).await? else {
        return Ok(None);
    };
    Ok(Some(
        tree.search(query, 25)
            .into_iter()
            .map(|p| card(&tree, p))
            .collect(),
    ))
}

/// Removes a tree and everything in it.
///
/// # Errors
/// When the database fails.
pub async fn delete(tree_id: &str) -> Result<bool, StoreError> {
    let store = store()?;
    let n = store
        .pool()?
        .execute(
            &format!("DELETE FROM {}.trees WHERE id = $1", store.schema),
            &[&tree_id],
        )
        .await?;
    if let Ok(mut m) = store.trees.write() {
        m.remove(tree_id);
    }
    Ok(n > 0)
}

// ---------------------------------------------------------------------------------------------
// Importing
// ---------------------------------------------------------------------------------------------

/// What an import made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Imported {
    pub id: String,
    pub name: String,
    pub people: usize,
    pub families: usize,
    /// Whether a tree of that name was there and has been replaced.
    pub replaced: bool,
}

/// `Walus family tree.ged` becomes `walus_family_tree`: the address of the tree.
#[must_use]
pub fn slug(file_name: &str) -> String {
    let stem = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let stem = stem
        .strip_suffix(".ged")
        .or_else(|| stem.strip_suffix(".GED"))
        .unwrap_or(stem);
    let slug: String = fold(stem)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if slug.is_empty() {
        "tree".to_string()
    } else {
        slug
    }
}

/// `walus_family_tree` becomes `Walus family tree`.
fn title(id: &str) -> String {
    let spaced = id.replace('_', " ");
    let mut chars = spaced.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}

fn arr(v: &[String]) -> PgArray<String> {
    PgArray(v.to_vec())
}

/// The rows of one import, a column at a time, ready to be unnested.
#[derive(Default)]
struct Rows {
    xref: Vec<String>,
    pos: Vec<i32>,
    given: Vec<String>,
    surname: Vec<String>,
    sex: Vec<String>,
    ahn: Vec<i32>,
    bd: Vec<String>,
    bp: Vec<String>,
    dd: Vec<String>,
    dp: Vec<String>,
    nick: Vec<String>,
    prefix: Vec<String>,
    suffix: Vec<String>,
    maiden: Vec<String>,
    cause: Vec<String>,
    ak_x: Vec<String>,
    ak_s: Vec<i32>,
    ak_n: Vec<String>,
    f_x: Vec<String>,
    f_pos: Vec<i32>,
    f_h: Vec<String>,
    f_w: Vec<String>,
    f_md: Vec<String>,
    f_mp: Vec<String>,
    c_f: Vec<String>,
    c_s: Vec<i32>,
    c_c: Vec<String>,
    e_owner: Vec<String>,
    e_seq: Vec<i32>,
    e_kind: Vec<String>,
    e_value: Vec<String>,
    e_date: Vec<String>,
    e_place: Vec<String>,
    e_cause: Vec<String>,
    e_note: Vec<String>,
    s_owner: Vec<String>,
    s_kind: Vec<String>,
    s_seq: Vec<i32>,
    s_title: Vec<String>,
    s_detail: Vec<String>,
    w_owner: Vec<String>,
    w_kind: Vec<String>,
    w_seq: Vec<i32>,
    w_name: Vec<String>,
    n_owner: Vec<String>,
    n_kind: Vec<String>,
    n_seq: Vec<i32>,
    n_text: Vec<String>,
}

fn at(i: usize) -> i32 {
    i32::try_from(i).unwrap_or(i32::MAX)
}

fn raw_date(e: Option<&Event>) -> String {
    e.and_then(|e| e.date.as_ref())
        .map(|d| d.raw.clone())
        .unwrap_or_default()
}

fn place_of(e: Option<&Event>) -> String {
    e.and_then(|e| e.place.clone()).unwrap_or_default()
}

impl Rows {
    fn notes(&mut self, owner: &str, kind: &str, notes: &[String]) {
        for (i, t) in notes.iter().enumerate() {
            self.n_owner.push(owner.to_string());
            self.n_kind.push(kind.to_string());
            self.n_seq.push(at(i));
            self.n_text.push(t.clone());
        }
    }

    fn cites(&mut self, owner: &str, kind: &str, list: &[Citation]) {
        for (i, c) in list.iter().enumerate() {
            self.s_owner.push(owner.to_string());
            self.s_kind.push(kind.to_string());
            self.s_seq.push(at(i));
            self.s_title.push(c.title.clone());
            self.s_detail.push(c.detail.clone());
        }
    }

    fn witnesses(&mut self, owner: &str, kind: &str, names: &[String]) {
        for (i, n) in names.iter().enumerate() {
            self.w_owner.push(owner.to_string());
            self.w_kind.push(kind.to_string());
            self.w_seq.push(at(i));
            self.w_name.push(n.clone());
        }
    }

    /// What an event carries besides its date and place, under the owner and the kind it is stored by.
    fn extras(&mut self, owner: &str, kind: &str, notes_kind: &str, e: &Event) {
        self.notes(owner, notes_kind, &e.notes);
        self.cites(owner, kind, &e.sources);
        self.witnesses(owner, kind, &e.witnesses);
    }

    fn person(&mut self, i: usize, p: &Person) {
        self.xref.push(p.xref.clone());
        self.pos.push(at(i));
        self.given.push(p.given.clone());
        self.surname.push(p.surname.clone());
        self.sex.push(
            match p.sex {
                Sex::Male => "M",
                Sex::Female => "F",
                Sex::Unknown => "",
            }
            .to_string(),
        );
        self.ahn
            .push(p.ahn.and_then(|n| i32::try_from(n).ok()).unwrap_or(0));
        self.bd.push(raw_date(p.birth.as_ref()));
        self.bp.push(place_of(p.birth.as_ref()));
        self.dd.push(raw_date(p.death.as_ref()));
        self.dp.push(place_of(p.death.as_ref()));
        self.nick.push(p.nickname.clone());
        self.prefix.push(p.prefix.clone());
        self.suffix.push(p.suffix.clone());
        self.maiden.push(p.maiden_name.clone());
        self.cause.push(
            p.death
                .as_ref()
                .and_then(|d| d.cause.clone())
                .unwrap_or_default(),
        );
        for (k, a) in p.akas.iter().enumerate() {
            self.ak_x.push(p.xref.clone());
            self.ak_s.push(at(k));
            self.ak_n.push(a.clone());
        }
        self.notes(&p.xref, "person", &p.notes);
        self.cites(&p.xref, "person", &p.sources);
        if let Some(b) = &p.birth {
            self.extras(&p.xref, "BIRT", "birth", b);
        }
        if let Some(d) = &p.death {
            self.extras(&p.xref, "DEAT", "death", d);
        }
        for (k, f) in p.facts.iter().enumerate() {
            self.e_owner.push(p.xref.clone());
            self.e_seq.push(at(k));
            self.e_kind.push(f.kind.clone());
            self.e_value.push(f.value.clone());
            self.e_date.push(raw_date(Some(&f.event)));
            self.e_place.push(f.event.place.clone().unwrap_or_default());
            self.e_cause.push(f.event.cause.clone().unwrap_or_default());
            self.e_note.push(f.event.notes.join("\n"));
            self.cites(&p.xref, &format!("fact:{k}"), &f.event.sources);
        }
    }

    fn family(&mut self, i: usize, f: &Family) {
        self.f_x.push(f.xref.clone());
        self.f_pos.push(at(i));
        self.f_h.push(f.husband.clone().unwrap_or_default());
        self.f_w.push(f.wife.clone().unwrap_or_default());
        self.f_md.push(raw_date(f.marriage.as_ref()));
        self.f_mp.push(place_of(f.marriage.as_ref()));
        for (k, c) in f.children.iter().enumerate() {
            self.c_f.push(f.xref.clone());
            self.c_s.push(at(k));
            self.c_c.push(c.clone());
        }
        self.notes(&f.xref, "family", &f.notes);
        if let Some(m) = &f.marriage {
            self.extras(&f.xref, "MARR", "marriage", m);
        }
    }
}

/// Writes the rows collected in `rows` into their tables, for tree `id`.
async fn write_rows(
    tx: &mut quench_pg::Transaction<'_>,
    s: &str,
    id: &str,
    rows: Rows,
) -> Result<(), StoreError> {
    tx.execute(
        &format!(
            "INSERT INTO {s}.people (tree_id, xref, pos, given, surname, sex, ahn, birth_date, birth_place, death_date, death_place, \
                                     nickname, prefix, suffix, maiden_name, death_cause) \
             SELECT $1, x, p, g, sn, sx, NULLIF(a, 0), NULLIF(bd, ''), NULLIF(bp, ''), NULLIF(dd, ''), NULLIF(dp, ''), \
                    nn, pf, sf, mn, NULLIF(dc, '') \
             FROM unnest($2::text[], $3::int[], $4::text[], $5::text[], $6::text[], $7::int[], $8::text[], $9::text[], $10::text[], $11::text[], \
                         $12::text[], $13::text[], $14::text[], $15::text[], $16::text[]) \
               AS t(x, p, g, sn, sx, a, bd, bp, dd, dp, nn, pf, sf, mn, dc)"
        ),
        &[&id, &arr(&rows.xref), &PgArray(rows.pos), &arr(&rows.given), &arr(&rows.surname), &arr(&rows.sex), &PgArray(rows.ahn),
          &arr(&rows.bd), &arr(&rows.bp), &arr(&rows.dd), &arr(&rows.dp),
          &arr(&rows.nick), &arr(&rows.prefix), &arr(&rows.suffix), &arr(&rows.maiden), &arr(&rows.cause)],
    )
    .await?;
    tx.execute(
        &format!(
            "INSERT INTO {s}.person_akas (tree_id, xref, seq, name) \
             SELECT $1, x, q, n FROM unnest($2::text[], $3::int[], $4::text[]) AS t(x, q, n)"
        ),
        &[&id, &arr(&rows.ak_x), &PgArray(rows.ak_s), &arr(&rows.ak_n)],
    )
    .await?;
    tx.execute(
        &format!(
            "INSERT INTO {s}.families (tree_id, xref, pos, husband, wife, marriage_date, marriage_place) \
             SELECT $1, x, p, NULLIF(h, ''), NULLIF(w, ''), NULLIF(md, ''), NULLIF(mp, '') \
             FROM unnest($2::text[], $3::int[], $4::text[], $5::text[], $6::text[], $7::text[]) AS t(x, p, h, w, md, mp)"
        ),
        &[&id, &arr(&rows.f_x), &PgArray(rows.f_pos), &arr(&rows.f_h), &arr(&rows.f_w), &arr(&rows.f_md), &arr(&rows.f_mp)],
    )
    .await?;
    tx.execute(
        &format!(
            "INSERT INTO {s}.family_children (tree_id, family_xref, seq, child_xref) \
             SELECT $1, f, q, c FROM unnest($2::text[], $3::int[], $4::text[]) AS t(f, q, c)"
        ),
        &[&id, &arr(&rows.c_f), &PgArray(rows.c_s), &arr(&rows.c_c)],
    )
    .await?;
    tx.execute(
        &format!(
            "INSERT INTO {s}.events (tree_id, owner, seq, kind, value, date, place, cause, note) \
             SELECT $1, o, q, k, v, NULLIF(d, ''), NULLIF(pl, ''), NULLIF(c, ''), NULLIF(n, '') \
             FROM unnest($2::text[], $3::int[], $4::text[], $5::text[], $6::text[], $7::text[], $8::text[], $9::text[]) \
               AS t(o, q, k, v, d, pl, c, n)"
        ),
        &[&id, &arr(&rows.e_owner), &PgArray(rows.e_seq), &arr(&rows.e_kind), &arr(&rows.e_value), &arr(&rows.e_date),
          &arr(&rows.e_place), &arr(&rows.e_cause), &arr(&rows.e_note)],
    )
    .await?;
    tx.execute(
        &format!(
            "INSERT INTO {s}.citations (tree_id, owner, kind, seq, title, detail) \
             SELECT $1, o, k, q, t, d FROM unnest($2::text[], $3::text[], $4::int[], $5::text[], $6::text[]) AS t2(o, k, q, t, d)"
        ),
        &[&id, &arr(&rows.s_owner), &arr(&rows.s_kind), &PgArray(rows.s_seq), &arr(&rows.s_title), &arr(&rows.s_detail)],
    )
    .await?;
    tx.execute(
        &format!(
            "INSERT INTO {s}.witnesses (tree_id, owner, kind, seq, name) \
             SELECT $1, o, k, q, n FROM unnest($2::text[], $3::text[], $4::int[], $5::text[]) AS t(o, k, q, n)"
        ),
        &[&id, &arr(&rows.w_owner), &arr(&rows.w_kind), &PgArray(rows.w_seq), &arr(&rows.w_name)],
    )
    .await?;
    tx.execute(
        &format!(
            "INSERT INTO {s}.notes (tree_id, owner, kind, seq, text) \
             SELECT $1, o, k, q, t FROM unnest($2::text[], $3::text[], $4::int[], $5::text[]) AS t2(o, k, q, t)"
        ),
        &[&id, &arr(&rows.n_owner), &arr(&rows.n_kind), &PgArray(rows.n_seq), &arr(&rows.n_text)],
    )
    .await?;
    Ok(())
}

/// Reads a GEDCOM file and stores it as a tree named after the file. A tree of the same name is
/// replaced, in one transaction: readers see the old tree or the new one, never half of each.
///
/// # Errors
/// When the file has no individuals, or the database fails.
pub async fn import(file_name: &str, text: &str) -> Result<Imported, StoreError> {
    let store = store()?;
    let id = slug(file_name);
    let name = title(&id);
    let tree =
        Tree::parse(&name, text).map_err(|e| StoreError::BadFile(format!("{file_name}: {e}")))?;
    let s = &store.schema;

    let mut conn = store.pool()?.get().await?;
    let mut tx = conn.begin().await?;
    let replaced = tx
        .execute(&format!("DELETE FROM {s}.trees WHERE id = $1"), &[&id])
        .await?
        > 0;
    tx.execute(
        &format!("INSERT INTO {s}.trees (id, name, source) VALUES ($1, $2, $3)"),
        &[&id, &name, &file_name],
    )
    .await?;

    // One statement per table: the columns go in as arrays and are unnested into rows.
    let mut rows = Rows::default();
    for (i, p) in tree.people().iter().enumerate() {
        rows.person(i, p);
    }
    for (i, f) in tree.families().iter().enumerate() {
        rows.family(i, f);
    }

    write_rows(&mut tx, s, &id, rows).await?;
    tx.commit().await?;

    if let Ok(mut m) = store.trees.write() {
        m.remove(&id);
    }
    quench_log::info!(
        "imported {file_name} as {id}: {} people, {} families",
        tree.people().len(),
        tree.families().len()
    );
    Ok(Imported {
        id,
        name,
        people: tree.people().len(),
        families: tree.families().len(),
        replaced,
    })
}
