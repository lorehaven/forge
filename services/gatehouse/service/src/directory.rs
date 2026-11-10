//! What a scoped grant can name.
//!
//! A grant such as `warehouse:storage:media:read` names one storage, one project, one target. The permission editor offers
//! them as a list, so gatehouse asks the services that own them. It does so with a token of its own, made for the one call:
//! audience the service asked, and the one grant the listing needs (`<service>:read`), so what it can see is no more than
//! the service's own read-only listing. Nothing is cached and nothing is stored; a service that cannot answer is reported
//! on its own and does not hold the page back.

use std::time::Duration;

use gatehouse_core::{ResourceChoice, ResourceKind};
use quench_auth::domain::jwt::JwtConfig;
use quench_core::future::join_all;
use quench_serde::Deserialize;

use crate::catalog::PermissionCatalog;

/// How long one service gets to answer before the page goes on without its list.
const TIMEOUT: Duration = Duration::from_secs(3);

/// A service's listing of one kind of resource.
struct Source {
    service: &'static str,
    resource_type: &'static str,
    /// Under the service's address, with its path prefix.
    path: &'static str,
    id: Field,
    label: Label,
}

#[derive(Clone, Copy)]
enum Field {
    Id,
    Name,
}

#[derive(Clone, Copy)]
enum Label {
    Name,
    /// `KEY name`, for a project that has a short key.
    KeyAndName,
    /// `name (owner)`.
    NameAndOwner,
}

/// What each service lists. A scoped type with no row here still gets its held grants shown; it just offers no list.
const SOURCES: [Source; 4] = [
    Source {
        service: "warehouse",
        resource_type: "storage",
        path: "/api/v1/storages",
        id: Field::Name,
        label: Label::NameAndOwner,
    },
    Source {
        service: "workbench",
        resource_type: "project",
        path: "/api/v1/projects",
        id: Field::Id,
        label: Label::KeyAndName,
    },
    Source {
        service: "conveyor",
        resource_type: "project",
        path: "/api/v1/projects?all=true",
        id: Field::Id,
        label: Label::Name,
    },
    Source {
        service: "gantry",
        resource_type: "target",
        path: "/api/v1/targets",
        id: Field::Name,
        label: Label::Name,
    },
];

/// The few fields any of the listings has in common.
#[derive(Debug, Deserialize)]
struct Entry {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    owner: Option<String>,
}

/// `WORKBENCH_API_URL`, else `WORKBENCH_URL`: where this service can be asked from inside, with its path prefix.
fn base_url(service: &str) -> Option<String> {
    let prefix = service.to_ascii_uppercase().replace('-', "_");
    ["API_URL", "URL"].iter().find_map(|suffix| {
        let value = quench_core::env::get_or(format!("{prefix}_{suffix}"), "");
        let value = value.trim();
        (!value.is_empty()).then(|| value.trim_end_matches('/').to_string())
    })
}

/// The listing's answer as choices.
///
/// # Errors
/// The text of why it is not a listing.
fn parse(source: &Source, body: &str) -> Result<Vec<ResourceChoice>, String> {
    let entries: Vec<Entry> = quench_serde::json::from_str(body)
        .map_err(|err| format!("the {} listing is unreadable: {err}", source.service))?;
    let mut choices: Vec<ResourceChoice> = entries
        .into_iter()
        .filter_map(|entry| {
            let id = match source.id {
                Field::Id => entry.id.clone(),
                Field::Name => entry.name.clone(),
            }
            .filter(|id| !id.is_empty())?;
            let name = entry.name.clone().unwrap_or_else(|| id.clone());
            let label = match source.label {
                Label::Name => name,
                Label::KeyAndName => entry
                    .key
                    .as_deref()
                    .map_or_else(|| name.clone(), |key| format!("{key} · {name}")),
                // A fixed storage has no owner and is read as its name alone.
                Label::NameAndOwner => entry
                    .owner
                    .as_deref()
                    .map_or_else(|| name.clone(), |owner| format!("{name} ({owner})")),
            };
            Some(ResourceChoice { id, label })
        })
        .collect();
    choices.sort_by_key(|choice| choice.label.to_lowercase());
    Ok(choices)
}

/// The choices a service's listing `body` holds.
///
/// # Errors
/// The text of why it is not a listing, or that gatehouse knows no listing for `service`.
pub fn parse_listing(service: &str, body: &str) -> Result<Vec<ResourceChoice>, String> {
    SOURCES
        .iter()
        .find(|source| source.service == service)
        .ok_or_else(|| format!("no listing is known for {service}"))
        .and_then(|source| parse(source, body))
}

async fn list(config: &JwtConfig, source: &Source) -> ResourceKind {
    let kind = |items, error| ResourceKind {
        service: source.service.to_string(),
        resource_type: source.resource_type.to_string(),
        items,
        error,
    };
    let Some(base) = base_url(source.service) else {
        return kind(
            Vec::new(),
            Some(format!(
                "{} is not configured (set {}_API_URL)",
                source.service,
                source.service.to_ascii_uppercase()
            )),
        );
    };
    let token = match config
        .issue_access_token_for(
            "gatehouse".to_string(),
            vec![source.service.to_string()],
            format!("user {}:read", source.service),
            None,
        )
        .await
    {
        Ok(token) => token,
        Err(err) => return kind(Vec::new(), Some(format!("could not make a token: {err}"))),
    };
    let http = match quench_client::transport::Client::builder()
        .timeout(TIMEOUT)
        .danger_accept_invalid_certs(!quench_core::env::is_or("DIRECTORY_TLS_VERIFY", true))
        .build()
    {
        Ok(http) => http,
        Err(err) => return kind(Vec::new(), Some(err.to_string())),
    };
    let response = match http
        .get(format!("{base}{}", source.path))
        .bearer_auth(token)
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return kind(
                Vec::new(),
                Some(format!("{} did not answer: {err}", source.service)),
            );
        }
    };
    if !response.status().is_success() {
        return kind(
            Vec::new(),
            Some(format!(
                "{} answered {}",
                source.service,
                response.status().as_u16()
            )),
        );
    }
    match response.text().await {
        Ok(body) => match parse(source, &body) {
            Ok(items) => kind(items, None),
            Err(why) => kind(Vec::new(), Some(why)),
        },
        Err(err) => kind(Vec::new(), Some(err.to_string())),
    }
}

/// The resources of every scoped type the catalog declares and a service lists, asked at the same time.
pub async fn lookup(config: &JwtConfig, catalog: &PermissionCatalog) -> Vec<ResourceKind> {
    let asks = SOURCES
        .iter()
        .filter(|source| {
            catalog
                .resource_types_for(source.service)
                .iter()
                .any(|known| known == source.resource_type)
        })
        .map(|source| list(config, source));
    join_all(asks).await
}
