//! Settings from the environment: plain variables, no file. `BASE_PATH`, `DATABASE_URL` and the auth settings are the
//! framework's own (read by `quench-starter`); everything else is prefixed `HOIST_`.

#[derive(Debug, Clone)]
pub struct Settings {
    pub fdroid_repo_url: String,
    pub izzyondroid_repo_url: String,
    pub aptoide_base_url: String,
    pub refresh_interval_minutes: u64,
    /// Pages of Aptoide's most downloaded apps pulled per run (30 apps a page).
    pub aptoide_pages: i64,
}

impl Settings {
    #[must_use]
    pub fn from_env() -> Self {
        let get = quench_core::env::get_or;
        Self {
            fdroid_repo_url: get("HOIST_FDROID_URL", "https://f-droid.org/repo"),
            izzyondroid_repo_url: get(
                "HOIST_IZZYONDROID_URL",
                "https://apt.izzysoft.de/fdroid/repo",
            ),
            aptoide_base_url: get("HOIST_APTOIDE_URL", "https://ws75.aptoide.com/api/7/"),
            refresh_interval_minutes: get("HOIST_REFRESH_MINUTES", "60")
                .parse()
                .ok()
                .filter(|m| *m > 0)
                .unwrap_or(60),
            aptoide_pages: get("HOIST_APTOIDE_PAGES", "100")
                .parse()
                .ok()
                .filter(|p| *p > 0)
                .unwrap_or(100),
        }
    }
}
