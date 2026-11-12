//! Who may do what to which tree. `Auth` has already put the caller's claims on the request.
//!
//! The grants are `genevia:read` and `genevia:write` (every tree), or the same for one tree, `genevia:tree:<id>:<action>`.
//! `write` also lets the holder read: nobody edits what they cannot see. With `SERVICE_AUTH_ENABLED` off, as in
//! development, everything is allowed.

use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_core::async_trait;
use quench_http::prelude::{FromRequest, HttpError, Request};

const SERVICE: &str = "genevia";

/// The caller, as far as the API cares.
#[derive(Debug, Clone)]
pub struct Access {
    claims: Option<Claims>,
    /// Whether the realm checks anything at all.
    enforced: bool,
}

impl Access {
    #[must_use]
    pub const fn new(claims: Option<Claims>, enforced: bool) -> Self {
        Self { claims, enforced }
    }

    /// The account name, for the log.
    #[must_use]
    pub fn who(&self) -> &str {
        self.claims.as_ref().map_or("dev", |c| c.sub.as_str())
    }

    fn holds(&self, action: &str) -> bool {
        !self.enforced || self.claims.as_ref().is_some_and(|c| c.can(SERVICE, action))
    }

    fn holds_on(&self, tree: &str, action: &str) -> bool {
        !self.enforced
            || self.claims.as_ref().is_some_and(|c| {
                c.can(SERVICE, action) || c.can(SERVICE, &format!("tree:{tree}:{action}"))
            })
    }

    /// May open `tree`.
    #[must_use]
    pub fn can_read(&self, tree: &str) -> bool {
        self.can_write(tree) || self.holds_on(tree, "read")
    }

    /// May change `tree`, or replace it by importing a file of the same name.
    #[must_use]
    pub fn can_write(&self, tree: &str) -> bool {
        self.holds_on(tree, "write")
    }

    /// May add trees and change every tree (the blanket `write`).
    #[must_use]
    pub fn can_write_all(&self) -> bool {
        self.holds("write")
    }

    /// Whether the caller sees every tree, rather than only the ones granted by name.
    #[must_use]
    pub fn can_read_all(&self) -> bool {
        self.holds("write") || self.holds("read")
    }

    /// The trees granted by name (`genevia:tree:<id>:read` or `:write`).
    #[must_use]
    pub fn granted_trees(&self) -> Vec<String> {
        let Some(claims) = &self.claims else {
            return Vec::new();
        };
        claims
            .permissions()
            .get(SERVICE)
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let rest = entry.strip_prefix("tree:")?;
                let (id, action) = rest.rsplit_once(':')?;
                matches!(action, "read" | "write").then(|| id.to_string())
            })
            .collect()
    }
}

#[async_trait]
impl FromRequest for Access {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let enforced = req
            .container()
            .get::<JwtConfig>()
            .map_or(true, |config| config.auth_enabled);
        Ok(Self::new(
            req.extensions().get::<Claims>().cloned(),
            enforced,
        ))
    }
}
