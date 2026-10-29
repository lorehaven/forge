//! The upstream sources. Each gives every listing it currently offers; `merge` turns the lists into one row per
//! package.

pub mod aptoide;
pub mod fdroid_format;

use quench_client::transport::Client;

use crate::config::Settings;
use crate::models::RawListing;

/// A client for the upstream stores: bounded time, an honest name.
#[must_use]
pub fn client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .user_agent("hoist-service")
        .build()
        .unwrap_or_default()
}

/// One source and how to reach it.
pub enum Source {
    /// F-Droid and IzzyOnDroid share the index format; `name` is `fdroid` or `izzyondroid`.
    FDroid {
        name: &'static str,
        repo_url: String,
    },
    Aptoide {
        base_url: String,
        pages: i64,
    },
}

impl Source {
    /// Every source the settings name, in the order they are preferred as the place to install from.
    #[must_use]
    pub fn all(settings: &Settings) -> Vec<Self> {
        vec![
            Self::FDroid {
                name: "fdroid",
                repo_url: settings.fdroid_repo_url.clone(),
            },
            Self::FDroid {
                name: "izzyondroid",
                repo_url: settings.izzyondroid_repo_url.clone(),
            },
            Self::Aptoide {
                base_url: settings.aptoide_base_url.clone(),
                pages: settings.aptoide_pages,
            },
        ]
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::FDroid { name, .. } => name,
            Self::Aptoide { .. } => "aptoide",
        }
    }

    /// Every listing this source offers (more than one per package is possible: Aptoide has several uploaders).
    ///
    /// # Errors
    /// A message saying what went wrong with the source.
    pub async fn fetch(&self, client: &Client) -> Result<Vec<RawListing>, String> {
        match self {
            Self::FDroid { name, repo_url } => fdroid_format::fetch(client, name, repo_url).await,
            Self::Aptoide { base_url, pages } => aptoide::fetch(client, base_url, *pages).await,
        }
    }
}
