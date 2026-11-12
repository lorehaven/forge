//! The screens' view of the service: one async function per call, over whatever sends a request - the browser's `fetch`
//! (the `genevia-web` crate) or a test's stand-in. The host installs its [`Host`] once, before the first screen is built.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use genevia_core::wire::{
    Ancestry, Card, FamilyEdit, FamilyForm, Me, PersonDetail, PersonEdit, Problem, RelativeBody,
    TreeInfo,
};
use quench_serde::json;
use quench_serde::{Deserialize, Serialize};

/// A request to send, under the service's prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: &'static str,
    /// Under the service (`/api/v1/trees`); the transport adds the prefix.
    pub path: String,
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// A future that is not sent between threads: a page has one.
pub type Boxed<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// What the screens need from the page they run in.
pub trait Host {
    /// Sends the request; the error is the reason it could not be sent (no network).
    fn send(&self, request: HttpRequest) -> Boxed<'_, Result<HttpResponse, String>>;
    /// Runs `work` in the background (the browser's task queue).
    fn spawn(&self, work: Boxed<'static, ()>);
    /// The service's path prefix: `/genevia`, or nothing when it is served at the root.
    fn base(&self) -> String;
}

thread_local! {
    static HOST: RefCell<Option<Rc<dyn Host>>> = const { RefCell::new(None) };
}

/// Hands the screens their host. Call before building the first screen; a second call replaces the first.
pub fn install(host: Rc<dyn Host>) {
    HOST.with(|h| *h.borrow_mut() = Some(host));
}

fn host() -> Rc<dyn Host> {
    HOST.with(|h| h.borrow().clone())
        .expect("the host is installed before the screens are built")
}

/// The service's prefix (`/genevia`), for addresses.
#[must_use]
pub fn base() -> String {
    host().base()
}

/// Runs `work` in the background.
pub fn spawn_local(work: impl Future<Output = ()> + 'static) {
    host().spawn(Box::pin(work));
}

/// What a call says when the session is gone.
pub const SIGNED_OUT: &str = "Sesja wygasła.";

/// Why a call did not give an answer, as the screens show it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApiError {
    message: String,
    /// The form field at fault, if the service named one.
    field: String,
    /// The session is gone: the person has to sign in again.
    pub signed_out: bool,
}

impl ApiError {
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// `(field, message)` pairs for a form: the field the service named, or none when the whole form is at fault.
    #[must_use]
    pub fn fields(&self) -> Vec<(String, String)> {
        vec![(self.field.clone(), self.message.clone())]
    }

    fn of(status: u16, body: &str) -> Self {
        let problem = json::from_str::<Problem>(body).unwrap_or_default();
        let (message, signed_out) = match status {
            401 => (SIGNED_OUT.to_string(), true),
            403 => ("Brak uprawnień do tego drzewa.".to_string(), false),
            404 if problem.error.is_empty() => ("Nie ma takiej strony.".to_string(), false),
            _ if problem.error.is_empty() => {
                (format!("Serwer odpowiedział kodem {status}."), false)
            }
            _ => (problem.error, false),
        };
        Self {
            message,
            field: problem.field,
            signed_out,
        }
    }

    fn offline(why: &str) -> Self {
        Self {
            message: format!("Brak połączenia z serwerem: {why}"),
            ..Self::default()
        }
    }

    fn unreadable(why: &impl std::fmt::Display) -> Self {
        Self {
            message: format!("Nie udało się odczytać odpowiedzi serwera: {why}"),
            ..Self::default()
        }
    }
}

/// One path segment, percent-encoded.
#[must_use]
pub fn segment(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// The service's path prefix from the page's own path: `/genevia/ui/t/x` is `/genevia`, `/ui` is no prefix.
#[must_use]
pub fn base_of(path: &str) -> String {
    let at = path
        .find("/ui/")
        .or_else(|| path.strip_suffix("/ui").map(str::len));
    at.map_or_else(String::new, |i| path[..i].trim_end_matches('/').to_string())
}

async fn send(
    method: &'static str,
    path: String,
    body: Option<String>,
) -> Result<String, ApiError> {
    let host = host();
    let response = host
        .send(HttpRequest { method, path, body })
        .await
        .map_err(|e| ApiError::offline(&e))?;
    if (200..300).contains(&response.status) {
        Ok(response.body)
    } else {
        Err(ApiError::of(response.status, &response.body))
    }
}

async fn read<T: Deserialize>(path: String) -> Result<T, ApiError> {
    let body = send("GET", path, None).await?;
    json::from_str(&body).map_err(|e| ApiError::unreadable(&e))
}

async fn write<B: Serialize>(
    method: &'static str,
    path: String,
    body: &B,
) -> Result<String, ApiError> {
    send(method, path, Some(json::to_string(body))).await
}

fn trees_path(tree: &str) -> String {
    format!("/api/v1/trees/{}", segment(tree))
}

/// Who is signed in and whether they may add and change trees.
pub async fn me() -> Result<Me, ApiError> {
    read("/api/v1/me".to_string()).await
}

/// All the trees the person may open.
pub async fn list_trees() -> Result<Vec<TreeInfo>, ApiError> {
    read("/api/v1/trees".to_string()).await
}

/// The ancestors of `root` (the tree's own root when blank), `generations` back.
pub async fn ancestry(tree: String, root: String, generations: u32) -> Result<Ancestry, ApiError> {
    read(format!(
        "{}/ancestry?root={}&generations={generations}",
        trees_path(&tree),
        segment(&root)
    ))
    .await
}

/// One person with relatives.
pub async fn person(tree: String, xref: String) -> Result<PersonDetail, ApiError> {
    read(format!("{}/people/{}", trees_path(&tree), segment(&xref))).await
}

/// People matching a query by name or birth place.
pub async fn search(tree: String, query: String) -> Result<Vec<Card>, ApiError> {
    read(format!(
        "{}/search?q={}",
        trees_path(&tree),
        segment(&query)
    ))
    .await
}

/// A person as a form has them.
pub async fn person_edit(tree: String, xref: String) -> Result<PersonEdit, ApiError> {
    read(format!(
        "{}/people/{}/edit",
        trees_path(&tree),
        segment(&xref)
    ))
    .await
}

/// Saves a person.
pub async fn save_person(tree: String, xref: String, edit: PersonEdit) -> Result<(), ApiError> {
    write(
        "PUT",
        format!("{}/people/{}", trees_path(&tree), segment(&xref)),
        &edit,
    )
    .await
    .map(drop)
}

/// A couple, their wedding and children as a form has them.
pub async fn family_edit(tree: String, family: String) -> Result<FamilyForm, ApiError> {
    read(format!(
        "{}/families/{}/edit",
        trees_path(&tree),
        segment(&family)
    ))
    .await
}

/// Saves a family.
pub async fn save_family(tree: String, family: String, edit: FamilyEdit) -> Result<(), ApiError> {
    write(
        "PUT",
        format!("{}/families/{}", trees_path(&tree), segment(&family)),
        &edit,
    )
    .await
    .map(drop)
}

/// Adds a person next to `of`: `relation` is `father`, `mother`, `spouse` or `child`. `existing` links someone already in
/// the tree; when it is empty `person` is added as new. Returns the person's id.
pub async fn add_relative(
    tree: String,
    of: String,
    relation: String,
    existing: String,
    person: PersonEdit,
    family: String,
) -> Result<String, ApiError> {
    let body = RelativeBody {
        relation,
        existing,
        person,
        family,
    };
    let text = write(
        "POST",
        format!("{}/people/{}/relatives", trees_path(&tree), segment(&of)),
        &body,
    )
    .await?;
    json::from_str(&text).map_err(|e| ApiError::unreadable(&e))
}

/// Removes a person from the tree and from every family they were in.
pub async fn delete_person(tree: String, xref: String) -> Result<(), ApiError> {
    send(
        "DELETE",
        format!("{}/people/{}", trees_path(&tree), segment(&xref)),
        None,
    )
    .await
    .map(drop)
}

/// Removes a couple's union; the people stay, without the link.
pub async fn delete_family(tree: String, family: String) -> Result<(), ApiError> {
    send(
        "DELETE",
        format!("{}/families/{}", trees_path(&tree), segment(&family)),
        None,
    )
    .await
    .map(drop)
}
