//! The trees: list, import, and everything inside one (people, families, search, the ancestors view).

use super::authz::Access;
use super::ok;
use crate::import::Incoming;
use crate::store::{self, StoreError};
use genevia_core::wire::{FamilyEdit, Imported, Me, PersonEdit, Problem, RelativeBody, TreeInfo};
use quench_http::prelude::{Json, Path, Query, Response, delete, get, http::StatusCode, post, put};
use quench_serde::Deserialize;

fn problem(status: StatusCode, message: &str, field: &str) -> Response {
    Response::json(
        status,
        &Problem {
            error: message.to_string(),
            field: field.to_string(),
        },
    )
    .unwrap_or_else(|_| Response::new(status))
}

fn forbidden() -> Response {
    problem(StatusCode::FORBIDDEN, "no access to this tree", "")
}

fn missing(what: &str) -> Response {
    problem(StatusCode::NOT_FOUND, &format!("no such {what}"), "")
}

/// A store failure as the answer to give: a form field that cannot be saved is the caller's to fix, the rest is ours.
fn failed(e: &StoreError) -> Response {
    match e {
        StoreError::Invalid { field, message } => {
            problem(StatusCode::UNPROCESSABLE_ENTITY, message, field)
        }
        StoreError::BadFile(message) => problem(StatusCode::UNPROCESSABLE_ENTITY, message, ""),
        other => {
            quench_log::error!("genevia: {other}");
            problem(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the database refused",
                "",
            )
        }
    }
}

fn done(r: Result<(), StoreError>) -> Response {
    match r {
        Ok(()) => Response::new(StatusCode::NO_CONTENT),
        Err(e) => failed(&e),
    }
}

/// Who is asking and what they may do, for the client to show only what works.
#[get("/api/v1/me")]
pub async fn me(access: Access) -> Response {
    ok(&Me {
        name: access.who().to_string(),
        write: access.can_write_all(),
    })
}

/// The trees the caller may open.
#[get("/api/v1/trees")]
pub async fn list(access: Access) -> Response {
    let all = access.can_read_all();
    let named = access.granted_trees();
    match store::trees().await {
        Ok(trees) => {
            let trees: Vec<TreeInfo> = trees
                .into_iter()
                .filter(|t| all || named.contains(&t.id))
                .collect();
            ok(&trees)
        }
        Err(e) => failed(&e),
    }
}

/// A GEDCOM file becomes a tree (the `file` field of a multipart body); a file of the same name replaces that tree.
#[post("/api/v1/trees")]
pub async fn import(access: Access, Incoming(upload): Incoming) -> Response {
    let upload = match upload {
        Ok(upload) => upload,
        Err(refused) => return problem(refused.status, &refused.message, ""),
    };
    // A new tree needs the blanket grant; replacing one needs write on that tree.
    let id = store::slug(&upload.name);
    if !(access.can_write_all() || access.can_write(&id)) {
        return forbidden();
    }
    match store::import(&upload.name, &upload.text).await {
        Ok(done) => {
            quench_log::info!(
                "genevia: {} imported {} ({} people)",
                access.who(),
                done.id,
                done.people
            );
            ok(&Imported {
                id: done.id,
                name: done.name,
                people: u32::try_from(done.people).unwrap_or(u32::MAX),
                families: u32::try_from(done.families).unwrap_or(u32::MAX),
                replaced: done.replaced,
            })
        }
        Err(e) => failed(&e),
    }
}

#[derive(Debug, Deserialize)]
pub struct AncestryQuery {
    #[serde(default)]
    pub root: String,
    #[serde(default = "default_generations")]
    pub generations: u32,
}

const fn default_generations() -> u32 {
    4
}

/// The ancestors of `root` (the tree's own root when blank), `generations` back.
#[get("/api/v1/trees/{tree}/ancestry")]
pub async fn ancestry(
    access: Access,
    Path(tree): Path<String>,
    Query(q): Query<AncestryQuery>,
) -> Response {
    if !access.can_read(&tree) {
        return forbidden();
    }
    match store::ancestry(&tree, &q.root, q.generations.min(12)).await {
        Ok(Some(a)) => ok(&a),
        Ok(None) => missing("person"),
        Err(e) => failed(&e),
    }
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
}

/// People matching a query by name or birth place.
#[get("/api/v1/trees/{tree}/search")]
pub async fn search(
    access: Access,
    Path(tree): Path<String>,
    Query(q): Query<SearchQuery>,
) -> Response {
    if !access.can_read(&tree) {
        return forbidden();
    }
    match store::search(&tree, &q.q).await {
        Ok(Some(cards)) => ok(&cards),
        Ok(None) => missing("tree"),
        Err(e) => failed(&e),
    }
}

/// One person with relatives.
#[get("/api/v1/trees/{tree}/people/{xref}")]
pub async fn person(access: Access, Path((tree, xref)): Path<(String, String)>) -> Response {
    if !access.can_read(&tree) {
        return forbidden();
    }
    match store::person(&tree, &xref).await {
        Ok(Some(mut p)) => {
            p.editable = access.can_write(&tree);
            ok(&p)
        }
        Ok(None) => missing("person"),
        Err(e) => failed(&e),
    }
}

/// A person as a form has them.
#[get("/api/v1/trees/{tree}/people/{xref}/edit")]
pub async fn person_edit(access: Access, Path((tree, xref)): Path<(String, String)>) -> Response {
    if !access.can_write(&tree) {
        return forbidden();
    }
    match store::edit::person_edit(&tree, &xref).await {
        Ok(Some(p)) => ok(&p),
        Ok(None) => missing("person"),
        Err(e) => failed(&e),
    }
}

/// Saves a person. Everyone who points at them sees the change, since they are one record.
#[put("/api/v1/trees/{tree}/people/{xref}")]
pub async fn save_person(
    access: Access,
    Path((tree, xref)): Path<(String, String)>,
    Json(edit): Json<PersonEdit>,
) -> Response {
    if !access.can_write(&tree) {
        return forbidden();
    }
    done(store::edit::save_person(&tree, &xref, &edit).await)
}

/// Removes a person from the tree and from every family they were in.
#[delete("/api/v1/trees/{tree}/people/{xref}")]
pub async fn delete_person(access: Access, Path((tree, xref)): Path<(String, String)>) -> Response {
    if !access.can_write(&tree) {
        return forbidden();
    }
    done(store::edit::delete_person(&tree, &xref).await)
}

/// Adds a person next to the one in the address. Answers with the new person's id.
#[post("/api/v1/trees/{tree}/people/{xref}/relatives")]
pub async fn add_relative(
    access: Access,
    Path((tree, xref)): Path<(String, String)>,
    Json(body): Json<RelativeBody>,
) -> Response {
    if !access.can_write(&tree) {
        return forbidden();
    }
    match store::edit::add_relative(
        &tree,
        &xref,
        &body.relation,
        &body.existing,
        &body.person,
        &body.family,
    )
    .await
    {
        Ok(id) => Response::json(StatusCode::CREATED, &id)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(e) => failed(&e),
    }
}

/// A couple, their wedding and children as a form has them.
#[get("/api/v1/trees/{tree}/families/{family}/edit")]
pub async fn family_edit(access: Access, Path((tree, family)): Path<(String, String)>) -> Response {
    if !access.can_write(&tree) {
        return forbidden();
    }
    match store::edit::family_edit(&tree, &family).await {
        Ok(Some(f)) => ok(&f),
        Ok(None) => missing("family"),
        Err(e) => failed(&e),
    }
}

/// Saves a family: both spouses and every child see the new wedding data and the new line-up.
#[put("/api/v1/trees/{tree}/families/{family}")]
pub async fn save_family(
    access: Access,
    Path((tree, family)): Path<(String, String)>,
    Json(edit): Json<FamilyEdit>,
) -> Response {
    if !access.can_write(&tree) {
        return forbidden();
    }
    done(store::edit::save_family(&tree, &family, &edit).await)
}

/// Removes a couple's union; the people stay, without the link.
#[delete("/api/v1/trees/{tree}/families/{family}")]
pub async fn delete_family(
    access: Access,
    Path((tree, family)): Path<(String, String)>,
) -> Response {
    if !access.can_write(&tree) {
        return forbidden();
    }
    done(store::edit::delete_family(&tree, &family).await)
}

pub fn register_routes() {
    let _ = me as fn(_) -> _;
    let _ = list as fn(_) -> _;
    let _ = import as fn(_, _) -> _;
    let _ = ancestry as fn(_, _, _) -> _;
    let _ = search as fn(_, _, _) -> _;
    let _ = person as fn(_, _) -> _;
    let _ = person_edit as fn(_, _) -> _;
    let _ = save_person as fn(_, _, _) -> _;
    let _ = delete_person as fn(_, _) -> _;
    let _ = add_relative as fn(_, _, _) -> _;
    let _ = family_edit as fn(_, _) -> _;
    let _ = save_family as fn(_, _, _) -> _;
    let _ = delete_family as fn(_, _) -> _;
}
