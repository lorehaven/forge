//! Starting and stopping the services under test.
//!
//! Services are built first and then launched from `target/debug` directly:
//! `cargo run` would leave the service alive when the suite kills its parent,
//! and a leaked process holds the port, so the next run silently tests the
//! stale instance instead of the one it just built.

use crate::world::Target;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::{Duration, sleep};

/// Lines gatehouse has printed to stdout, captured live.
///
/// `LoggingSender` (`services/gatehouse/service/src/email.rs`) writes the
/// registration/reset link nowhere else, by design - this is what lets the
/// registration and password-reset BDD scenarios read a link they were never
/// otherwise given. Every other service inherits the harness's own stdout
/// unchanged; only gatehouse needs its own tapped and re-piped, so only
/// gatehouse pays for it.
static GATEHOUSE_LOG: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();

pub fn gatehouse_log() -> Arc<Mutex<Vec<String>>> {
    GATEHOUSE_LOG
        .get_or_init(|| Arc::new(Mutex::new(Vec::new())))
        .clone()
}

/// Waits for a gatehouse log line containing `needle`, most recent first: the
/// line lands a beat after the HTTP response that triggered it, not within it.
pub async fn wait_for_gatehouse_log(needle: &str) -> Option<String> {
    let log = gatehouse_log();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(line) = log
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|line| line.contains(needle))
        {
            return Some(line.clone());
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        sleep(Duration::from_millis(50)).await;
    }
}

/// A service the suite started, killed on drop as a backstop.
pub struct Service {
    pub target: Target,
    child: Child,
}

impl Service {
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    pub async fn stop(mut self) {
        let _ = self.child.kill().await;
    }
}

pub struct Fixture {
    workspace_root: PathBuf,
}

impl Fixture {
    pub fn new(manifest_dir: &str) -> Self {
        let workspace_root = Path::new(manifest_dir)
            .parent()
            .expect("tests/ directory")
            .parent()
            .expect("workspace root")
            .to_path_buf();

        Self { workspace_root }
    }

    /// Where a service's crate lives: the `service/` directory of its project (`daybook-service` is
    /// `services/daybook/service`).
    fn service_dir(&self, package: &str) -> PathBuf {
        let app = package.strip_suffix("-service").unwrap_or(package);
        self.workspace_root
            .join("services")
            .join(app)
            .join("service")
    }

    fn binary(&self, package: &str) -> PathBuf {
        let target_dir = std::env::var("CARGO_TARGET_DIR")
            .map_or_else(|_| self.workspace_root.join("target"), PathBuf::from);
        target_dir.join("debug").join(package)
    }

    async fn build(&self, package: &str) {
        // The package is the project's name; the binary and the directory keep the `-service` suffix.
        let package = package.strip_suffix("-service").unwrap_or(package);
        println!("Building {package}...");
        let status = Command::new("cargo")
            .arg("build")
            .arg("-p")
            .arg(package)
            .current_dir(&self.workspace_root)
            .envs(std::env::vars())
            .status()
            .await
            .unwrap_or_else(|e| panic!("failed to build {package}: {e}"));
        assert!(status.success(), "{package} build failed");
        self.ensure_client(package).await;
    }

    /// The browser client of a service that has one (`services/<name>/web`).
    ///
    /// The module is git-ignored (`scripts/web.sh` or the image build makes it), so a fresh checkout - the CI pod - has
    /// none, and the service then answers its pages with a 404. Builds it when it is missing.
    async fn ensure_client(&self, package: &str) {
        let app = package.strip_suffix("-service").unwrap_or(package);
        let crate_dir = self.workspace_root.join("services").join(app).join("web");
        let module = self.service_dir(package).join("web").join("app.wasm");
        if !crate_dir.join("Cargo.toml").is_file() || module.is_file() {
            return;
        }
        let web_package = format!("{app}-web");
        println!("Building {web_package} (wasm)...");
        let status = Command::new("cargo")
            .args([
                "build",
                "-q",
                "--target",
                "wasm32-unknown-unknown",
                "--profile",
                "wasm",
                "-p",
            ])
            .arg(&web_package)
            .current_dir(&self.workspace_root)
            .envs(std::env::vars())
            .status()
            .await
            .unwrap_or_else(|e| panic!("failed to build {web_package}: {e}"));
        assert!(
            status.success(),
            "{web_package} build failed (is the wasm32-unknown-unknown target installed?)"
        );
        let built = self
            .target_dir()
            .join("wasm32-unknown-unknown")
            .join("wasm")
            .join(format!("{}.wasm", web_package.replace('-', "_")));
        std::fs::create_dir_all(module.parent().expect("a web dir")).expect("create the web dir");
        std::fs::copy(&built, &module)
            .unwrap_or_else(|e| panic!("copy {} to {}: {e}", built.display(), module.display()));
    }

    fn target_dir(&self) -> PathBuf {
        std::env::var_os("CARGO_TARGET_DIR")
            .map_or_else(|| self.workspace_root.join("target"), PathBuf::from)
    }

    /// Environment every service shares: one realm (gatehouse, always
    /// started alongside whatever was selected - see `main.rs`), an
    /// in-memory database so the suite needs no Postgres.
    fn common_env(command: &mut Command, service_name: &str) {
        let (certificate, key) = dev_certificate();
        command
            .env("SERVICE_NAME", service_name)
            .env("SERVER_CERT_PATH", certificate)
            .env("SERVER_KEY_PATH", key)
            .env("DATABASE_URL", "")
            .env("POSTGRES_URL", "")
            // The suite deliberately runs on a throwaway in-memory store.
            .env("ALLOW_IN_MEMORY_DB", "true")
            .env("DB_RECREATE", "false")
            .env("SERVICE_USERNAME", "admin")
            .env("SERVICE_PASSWORD", "password")
            // Each service runs standalone here, with its own in-memory realm,
            // so it seeds the user itself instead of relying on gatehouse.
            .env("AUTH_BOOTSTRAP", "true")
            // Gatehouse owns login: a relying party with no GATEHOUSE_URL
            // cannot sign anyone in, so the fixture always points at it even
            // when this run does not start it.
            .env("GATEHOUSE_URL", "http://127.0.0.1:5443/gatehouse")
            .env("GATEHOUSE_TLS_VERIFY", "false")
            // This service's own OAuth client identity toward gatehouse
            // (`tests/forge-bdd/clients.toml`) - matched on gatehouse's side
            // by the identically-named CLIENT_SECRET_* in `start_gatehouse`.
            .env("GATEHOUSE_CLIENT_ID", service_name)
            .env(
                "GATEHOUSE_CLIENT_SECRET",
                format!("bdd-client-secret-{service_name}"),
            )
            .kill_on_drop(true);
    }

    pub async fn start(&self, target: Target) -> Service {
        match target {
            Target::Sage => self.start_sage().await,
            Target::Switchboard => self.start_switchboard().await,
            Target::Warehouse => self.start_warehouse().await,
            Target::Gatehouse => self.start_gatehouse().await,
            Target::Conveyor => self.start_conveyor().await,
            Target::Workbench => self.start_workbench().await,
            Target::Daybook => self.start_daybook().await,
            Target::Mailroom => self.start_mailroom().await,
            Target::Gauge => self.start_gauge().await,
        }
    }

    /// Conveyor, with no database.
    ///
    /// Its queue needs Postgres and says so; the suite deliberately runs on an
    /// in-memory store, so these scenarios cover what a database cannot change:
    /// the UI shell, gatehouse delegation, which routes need a token, and the
    /// webhook endpoint's refusals. Everything that does touch the queue is
    /// covered by `services/conveyor/service/tests/integration`, against a real
    /// Postgres.
    /// Points `command` at the suite's Postgres (`FORGE_BDD_DATABASE_URL`), in the schema the service's module owns.
    fn use_database(command: &mut Command, schema: &str) {
        if let Some(url) = database_url() {
            command
                .env("DATABASE_URL", &url)
                .env("POSTGRES_URL", url)
                .env("DB_SCHEMA", schema)
                .env("ALLOW_IN_MEMORY_DB", "false");
        }
    }

    async fn start_conveyor(&self) -> Service {
        self.build("conveyor-service").await;
        println!("Starting conveyor-service...");

        let mut command = Command::new(self.binary("conveyor-service"));
        command
            .current_dir(self.service_dir("conveyor-service"))
            .envs(std::env::vars());
        Self::common_env(&mut command, "conveyor");
        command
            .env("SERVER_ADDR", "127.0.0.1:9999")
            .env("SERVER_HTTP_REDIRECT_ADDR", "127.0.0.1:9998")
            .env("BASE_PATH", "/conveyor")
            .env("DB_SCHEMA", "test_conveyor")
            .env("SERVICE_AUTH_ENABLED", "true")
            // No TLS here: conveyor ships no dev certificate of its own, and
            // the suite has no reason to borrow one.
            .env("SERVER_CERT_PATH", "missing.pem")
            .env("SERVER_KEY_PATH", "missing.pem")
            // Set, so the webhook endpoint's "not configured" path is the one
            // scenario that has to turn it off rather than the default.
            .env("CONVEYOR_WEBHOOK_SECRET", "conveyor-bdd-secret");
        // Its queue needs one: with a database the whole API is real, without one every route says so (503).
        Self::use_database(&mut command, "conveyor");

        Service {
            target: Target::Conveyor,
            child: command.spawn().expect("failed to start conveyor-service"),
        }
    }

    /// Workbench, with no database.
    ///
    /// Its domain layer needs Postgres and says so; the suite deliberately
    /// runs on an in-memory store, so these scenarios cover what a database
    /// cannot change: the UI shell, gatehouse delegation, and which routes
    /// need a token. Everything that touches the tables is covered by
    /// `services/workbench/service/tests/integration`, against a real Postgres.
    async fn start_workbench(&self) -> Service {
        self.build("workbench-service").await;
        println!("Starting workbench-service...");

        let mut command = Command::new(self.binary("workbench-service"));
        command
            .current_dir(self.service_dir("workbench-service"))
            .envs(std::env::vars());
        Self::common_env(&mut command, "workbench");
        command
            .env("SERVER_ADDR", "127.0.0.1:10443")
            .env("SERVER_HTTP_REDIRECT_ADDR", "127.0.0.1:10080")
            .env("BASE_PATH", "/workbench")
            .env("DB_SCHEMA", "test_workbench")
            .env("SERVICE_AUTH_ENABLED", "true")
            // No TLS here: workbench ships no dev certificate of its own, and
            // the suite has no reason to borrow one.
            .env("SERVER_CERT_PATH", "missing.pem")
            .env("SERVER_KEY_PATH", "missing.pem");

        if let Some(url) = database_url() {
            command
                .env("DATABASE_URL", &url)
                .env("POSTGRES_URL", url)
                .env("DB_SCHEMA", "workbench")
                .env("ALLOW_IN_MEMORY_DB", "false");
        }

        Service {
            target: Target::Workbench,
            child: command.spawn().expect("failed to start workbench-service"),
        }
    }

    /// Mailroom, over a Postgres schema of its own. It opens no connection to any mail server, so there is nothing else to fake.
    async fn start_mailroom(&self) -> Service {
        self.build("mailroom-service").await;
        println!("Starting mailroom-service...");

        let mut command = Command::new(self.binary("mailroom-service"));
        command
            .current_dir(self.service_dir("mailroom-service"))
            .envs(std::env::vars());
        Self::common_env(&mut command, "mailroom");
        command
            .env("SERVER_ADDR", "127.0.0.1:18443")
            .env("BASE_PATH", "/mailroom")
            .env(
                "DATABASE_URL",
                crate::mail::database_url().unwrap_or_default(),
            )
            .env("DB_SCHEMA", crate::mail::SCHEMA)
            .env("SERVICE_AUTH_ENABLED", "true")
            .env("SERVER_CERT_PATH", "missing.pem")
            .env("SERVER_KEY_PATH", "missing.pem")
            .env("MAILROOM_DOMAIN", crate::mail::DOMAIN)
            // The scenarios do not wait for the sweeper.
            .env("MAILROOM_BACKGROUND", "false");

        Service {
            target: Target::Mailroom,
            child: command.spawn().expect("failed to start mailroom-service"),
        }
    }

    /// Gauge, watching what `mocks::start_node_exporter` serves and one host that is never there. It keeps
    /// its history in memory (no `DATABASE_URL`), and a second between scrapes so a scenario need not wait.
    async fn start_gauge(&self) -> Service {
        self.build("gauge-service").await;
        println!("Starting gauge-service...");

        let mut command = Command::new(self.binary("gauge-service"));
        command
            .current_dir(self.service_dir("gauge-service"))
            .envs(std::env::vars());
        Self::common_env(&mut command, "gauge");
        command
            .env("SERVER_ADDR", "127.0.0.1:13443")
            .env("BASE_PATH", "/gauge")
            .env("DB_SCHEMA", "test_gauge")
            .env("SERVICE_AUTH_ENABLED", "true")
            .env(
                "GAUGE_TARGETS",
                self.workspace_root
                    .join("tests/forge-bdd/gauge-targets.toml")
                    .display()
                    .to_string(),
            )
            .env("SERVER_CERT_PATH", "missing.pem")
            .env("SERVER_KEY_PATH", "missing.pem");

        Service {
            target: Target::Gauge,
            child: command.spawn().expect("failed to start gauge-service"),
        }
    }

    /// Daybook's API is exercised here with an in-memory database. CRUD
    /// persistence and owner isolation are covered by its Postgres-backed
    /// integration suite.
    async fn start_daybook(&self) -> Service {
        self.build("daybook-service").await;
        println!("Starting daybook-service...");

        let mut command = Command::new(self.binary("daybook-service"));
        command
            .current_dir(self.service_dir("daybook-service"))
            .envs(std::env::vars());
        Self::common_env(&mut command, "daybook");
        command
            .env("SERVER_ADDR", "127.0.0.1:12443")
            .env("BASE_PATH", "/daybook")
            .env("DB_SCHEMA", "test_daybook")
            .env("SERVICE_AUTH_ENABLED", "true")
            .env("SERVER_CERT_PATH", "missing.pem")
            .env("SERVER_KEY_PATH", "missing.pem");
        if let Some(url) = database_url() {
            // The module's own schema, not the throwaway one the in-memory run names.
            command
                .env("DATABASE_URL", &url)
                .env("POSTGRES_URL", url)
                .env("DB_SCHEMA", "daybook")
                .env("ALLOW_IN_MEMORY_DB", "false");
        }

        Service {
            target: Target::Daybook,
            child: command.spawn().expect("failed to start daybook-service"),
        }
    }

    async fn start_sage(&self) -> Service {
        self.build("sage-service").await;
        println!("Starting sage-service...");

        let mut command = Command::new(self.binary("sage-service"));
        command
            .current_dir(self.service_dir("sage-service"))
            .envs(std::env::vars());
        Self::common_env(&mut command, "sage");
        command
            .env("SERVER_ADDR", "127.0.0.1:7777")
            .env("SERVER_HTTP_REDIRECT_ADDR", "127.0.0.1:7778")
            .env("BASE_PATH", "/sage")
            .env("DB_SCHEMA", "test_sage")
            .env("SERVICE_AUTH_ENABLED", "true")
            .env("SKIP_SWITCHBOARD_CHECK", "true")
            .env("VLLM_TLS_VERIFY", "false")
            // Mock backends started by the suite.
            .env("SWITCHBOARD_URL", "http://127.0.0.1:19554/switchboard")
            .env("VLLM_BASE_URL", "http://127.0.0.1:18000")
            // sage's machine identity toward switchboard (client_credentials) -
            // matches the "sage-switchboard" entry in `clients.toml` and the
            // identically-named var on gatehouse's own env in `start_gatehouse`.
            .env(
                "CLIENT_SECRET_SAGE_SWITCHBOARD",
                "bdd-client-secret-sage-switchboard",
            )
            .env("SWITCHBOARD_TLS_VERIFY", "false")
            .env("SAGE_CAPABILITY_PROFILE", "web_assistant")
            .env("SAGE_DEFAULT_MODELS", r#"[{"name":"test-model"}]"#)
            .env("SAGE_SUPPORTED_MODELS", "*")
            // Exercise the graceful default-model teardown on shutdown.
            .env("SAGE_STOP_MODELS_ON_SHUTDOWN", "true");

        // Files and the conversations' history are kept in it; without one the files API answers 501.
        Self::use_database(&mut command, "sage");

        Service {
            target: Target::Sage,
            child: command.spawn().expect("failed to start sage-service"),
        }
    }

    async fn start_switchboard(&self) -> Service {
        self.build("switchboard-service").await;
        println!("Starting switchboard-service...");

        let mut command = Command::new(self.binary("switchboard-service"));
        command
            .current_dir(self.service_dir("switchboard-service"))
            .envs(std::env::vars());
        Self::common_env(&mut command, "switchboard");
        command
            .env("SERVER_ADDR", "127.0.0.1:8554")
            .env("SERVER_HTTP_REDIRECT_ADDR", "127.0.0.1:8081")
            .env("BASE_PATH", "/switchboard")
            .env("DB_SCHEMA", "test_switchboard")
            .env("SERVICE_AUTH_ENABLED", "true")
            .env("FEATURE_MODELS_DASHBOARD_ENABLED", "true")
            .env("FEATURE_VLLM_MANAGEMENT_ENABLED", "true")
            .env("VLLM_MANAGEMENT_MODE", "mock");

        Service {
            target: Target::Switchboard,
            child: command
                .spawn()
                .expect("failed to start switchboard-service"),
        }
    }

    async fn start_warehouse(&self) -> Service {
        self.build("warehouse-service").await;
        println!("Starting warehouse-service...");

        let storage_dir = self.workspace_root.join("target").join("bdd-storage");
        let _ = std::fs::remove_dir_all(&storage_dir);
        std::fs::create_dir_all(&storage_dir).ok();
        // Unlike the crates and docker registries, the files API confines a
        // write by canonicalizing the storage root (`routers::files::confined`)
        // - a root that does not exist on disk canonicalizes to nothing, so
        // every write into it is refused as "outside the storage" before it
        // gets anywhere near auth or permissions.
        std::fs::create_dir_all(storage_dir.join("files").join("test")).ok();

        let mut command = Command::new(self.binary("warehouse-service"));
        command
            .current_dir(self.service_dir("warehouse-service"))
            .envs(std::env::vars());
        Self::common_env(&mut command, "warehouse");
        command
            .env("SERVER_ADDR", "127.0.0.1:8443")
            .env("SERVER_HTTP_REDIRECT_ADDR", "127.0.0.1:8080")
            .env("BASE_PATH", "/warehouse")
            .env("DB_SCHEMA", "test_warehouse")
            .env("SERVICE_AUTH_ENABLED", "true")
            // Signs warehouse's own registry tokens; the service refuses to start without one, and its `.env` (where
            // a developer keeps it) is git-ignored, so a fresh checkout has none.
            .env("DOCKER_TOKEN_SECRET", "bdd-docker-token-secret")
            .env("FEATURE_CRATES_ENABLED", "true")
            .env("FEATURE_DOCKER_ENABLED", "true")
            .env("FEATURE_FILES_ENABLED", "true")
            .env("FEATURE_ARTIFACTS_ENABLED", "true")
            .env("ARTIFACT_STORAGE_PATH", storage_dir.join("artifacts"))
            .env("CRATES_STORAGE_PATH", storage_dir.join("crates"))
            .env("STORAGE_PATH", storage_dir.join("docker"))
            .env(
                "FILE_STORAGES",
                format!("test={}", storage_dir.join("files").display()),
            );

        // Dynamic storages and the registries' metadata live in it; without one the files API cannot provision a storage.
        Self::use_database(&mut command, "warehouse");

        Service {
            target: Target::Warehouse,
            child: command.spawn().expect("failed to start warehouse-service"),
        }
    }

    async fn start_gatehouse(&self) -> Service {
        self.build("gatehouse-service").await;
        println!("Starting gatehouse-service...");

        let mut command = Command::new(self.binary("gatehouse-service"));
        command
            .current_dir(self.service_dir("gatehouse-service"))
            .envs(std::env::vars());
        Self::common_env(&mut command, "gatehouse");
        command
            .env("SERVER_ADDR", "127.0.0.1:5443")
            .env("SERVER_HTTP_REDIRECT_ADDR", "127.0.0.1:5080")
            .env("BASE_PATH", "/gatehouse")
            // Emailed links come from configuration, never the request's Host
            // header - pin it to the address this suite reaches gatehouse on.
            .env("PUBLIC_BASE_URL", "http://127.0.0.1:5443")
            // The estate's daily cap on mail (250 by default). The security scenarios fuzz and scan every route that sends
            // some - registration, resets, invitations - and would use the day up before the functional ones had theirs.
            .env("MAIL_DAILY_LIMIT", "1000000")
            // Failed sign-ins one address may make in ten minutes (30 by default). Scenarios that do not send an address
            // share one bucket, and the suite signs in wrongly on purpose in a hundred places; the throttle's own
            // scenario sends an address of its own and goes over this.
            .env("LOGIN_FAILURES_PER_IP", "100")
            .env("SERVICE_AUTH_ENABLED", "true")
            // Plain HTTP, deterministically: gatehouse ships no certificate of
            // its own, but the dev environment (foreman) symlinks one in, and the
            // suite must not change behaviour depending on whether that ran.
            .env("SERVER_CERT_PATH", "/nonexistent/bdd-no-tls.pem")
            .env("SERVER_KEY_PATH", "/nonexistent/bdd-no-tls.key")
            // Ignored now (the permission catalog decides audiences - see
            // `services/gatehouse/service/config/permissions.toml`, which
            // already lists every BDD service), kept for anyone still
            // reading this env block for the old story.
            .env(
                "SERVICE_AUDIENCES",
                "sage,switchboard,warehouse,conveyor,workbench,daybook,gatehouse",
            )
            // Signing keys are encrypted at rest even in this throwaway
            // in-memory database - see `keys.rs`.
            .env(
                "GATEHOUSE_KEY_ENCRYPTION_KEY",
                "forge-bdd-key-encryption-key",
            )
            // Turns on `POST /api/v1/test/token` (`api/test_tokens.rs`), the
            // harness's replacement for self-signing tokens against a shared
            // secret: a real, JWKS-verifiable token for an arbitrary
            // subject/audience/scope, no user or session required.
            .env("GATEHOUSE_TEST_MODE", "true")
            // Mirrors `services/gatehouse/service/config/clients.toml`, with
            // redirect_uris pointed at the fixed ports below instead of a
            // real deployment's - see `tests/forge-bdd/clients.toml`. Powers
            // the `/ui/login` → gatehouse `/authorize` → code → `/ui/auth/callback`
            // round trip `ui_auth.feature` exercises.
            .env(
                "CLIENTS_CONFIG",
                self.workspace_root
                    .join("tests/forge-bdd/clients.toml")
                    .display()
                    .to_string(),
            )
            .env("CLIENT_SECRET_SAGE", "bdd-client-secret-sage")
            .env("CLIENT_SECRET_SWITCHBOARD", "bdd-client-secret-switchboard")
            .env("CLIENT_SECRET_WAREHOUSE", "bdd-client-secret-warehouse")
            .env("CLIENT_SECRET_CONVEYOR", "bdd-client-secret-conveyor")
            .env("CLIENT_SECRET_WORKBENCH", "bdd-client-secret-workbench")
            .env("CLIENT_SECRET_GAUGE", "bdd-client-secret-gauge")
            .env(
                "CLIENT_SECRET_SAGE_SWITCHBOARD",
                "bdd-client-secret-sage-switchboard",
            )
            .env(
                "AUTH_REDIRECT_HOSTS",
                "https://127.0.0.1:8443,http://127.0.0.1:8443",
            )
            // Home page fixture: two services configured, one turned off by its
            // feature flag, so the suite covers both halves of the gating rule.
            // Also what `clients.toml`'s redirect_uris are built from - see
            // `redirect_base_url` in `services/gatehouse/service/src/clients.rs`.
            .env("SAGE_UI_URL", "https://127.0.0.1:7777/sage/ui/home")
            .env(
                "SWITCHBOARD_UI_URL",
                "https://127.0.0.1:8554/switchboard/ui/home",
            )
            .env(
                "WAREHOUSE_UI_URL",
                "https://127.0.0.1:8443/warehouse/ui/home",
            )
            .env("CONVEYOR_UI_URL", "http://127.0.0.1:9999/conveyor/ui/home")
            .env(
                "WORKBENCH_UI_URL",
                "http://127.0.0.1:10443/workbench/ui/home",
            )
            .env("FEATURE_WAREHOUSE_ENABLED", "false")
            .env("MAILROOM_UI_URL", "http://127.0.0.1:18443/mailroom/ui/home")
            .env("CLIENT_SECRET_MAILROOM", "bdd-client-secret-mailroom")
            .env(
                "CLIENT_SECRET_GATEHOUSE_MAILROOM",
                "bdd-client-secret-gatehouse-mailroom",
            )
            .stdout(Stdio::piped());
        // With Mailroom in the run, gatehouse writes its mail into it (and still logs the links, which its own scenarios read).
        if crate::mail::enabled() {
            command
                .env("MAILROOM_URL", "http://127.0.0.1:18443/mailroom")
                .env("MAILROOM_CLIENT_ID", "gatehouse-mailroom")
                .env(
                    "MAILROOM_CLIENT_SECRET",
                    "bdd-client-secret-gatehouse-mailroom",
                );
        }

        let mut child = command.spawn().expect("failed to start gatehouse-service");
        let stdout = child.stdout.take().expect("gatehouse stdout not piped");
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            let log = gatehouse_log();
            while let Ok(Some(line)) = lines.next_line().await {
                println!("{line}");
                log.lock().unwrap().push(line);
            }
        });

        Service {
            target: Target::Gatehouse,
            child,
        }
    }
}

/// Waits until every started service answers, so a slow boot does not surface
/// as a scenario failure.
/// A Postgres the services under test may use instead of their in-memory stores (`FORGE_BDD_DATABASE_URL`): one database with
/// the daybook (and, as they gain scenarios, workbench and conveyor) modules installed by `foundry apply --install`. Scenarios
/// tagged `@postgres` run only when it is set; every other scenario is the same either way.
#[must_use]
pub fn database_url() -> Option<String> {
    std::env::var("FORGE_BDD_DATABASE_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
}

/// `ensure_user`, once per name for the whole run: every token the suite mints names a subject, and a service whose
/// tables point at `auth.users` refuses a subject that is not in it.
pub async fn ensure_user_once(username: &str) {
    static SEEN: OnceLock<tokio::sync::Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    // Held across the insert: a second scenario that needs the same name waits for the row instead of racing past it.
    let mut seen = SEEN.get_or_init(Default::default).lock().await;
    if seen.insert(username.to_string()) {
        ensure_user(username).await;
    }
}

/// Makes sure `username` has a row in the database's `auth.users`, which a service's own tables point at (a task's owner is a
/// foreign key to it). In the estate gatehouse writes it; the harness's gatehouse keeps its users in memory, so a scenario that
/// acts as somebody new on a real database asks for the row here first.
///
/// # Panics
///
/// When `FORGE_BDD_DATABASE_URL` is unset or the database cannot be reached.
pub async fn ensure_user(username: &str) {
    let url = database_url().expect("FORGE_BDD_DATABASE_URL");
    let pool = quench_pg::Pool::connect(&url)
        .await
        .unwrap_or_else(|e| panic!("FORGE_BDD_DATABASE_URL: {e}"));
    pool.execute(
        "INSERT INTO auth.users (username, password, roles) VALUES ($1, 'x', '[]'::jsonb) ON CONFLICT (username) DO NOTHING",
        &[&username as &dyn quench_pg::ToSql],
    )
    .await
    .expect("seed the user");
}

/// The certificate and key the HTTPS services (sage, switchboard, warehouse) are started with.
///
/// A service whose certificate is missing quietly serves plain HTTP, and the suites for those three speak HTTPS,
/// so with no certificate every request would wait on a handshake that never comes. Picked in this order, once:
/// `FORGE_BDD_CERT` with `FORGE_BDD_KEY`; the estate's shared dev certificate `certs/cert.pem` with `key.pem`;
/// otherwise a throwaway self-signed one made with `openssl` under `target/forge-bdd-cert` (so a fresh checkout,
/// such as a CI job's, needs nothing set up). Clients here accept any certificate.
fn dev_certificate() -> (PathBuf, PathBuf) {
    static CERTIFICATE: OnceLock<(PathBuf, PathBuf)> = OnceLock::new();
    CERTIFICATE
        .get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(Path::parent)
                .expect("workspace root")
                .to_path_buf();
            if let (Some(certificate), Some(key)) = (
                std::env::var_os("FORGE_BDD_CERT"),
                std::env::var_os("FORGE_BDD_KEY"),
            ) {
                return (PathBuf::from(certificate), PathBuf::from(key));
            }
            let shared = (root.join("certs/cert.pem"), root.join("certs/key.pem"));
            if shared.0.is_file() && shared.1.is_file() {
                return shared;
            }
            let target_dir = std::env::var("CARGO_TARGET_DIR")
                .map_or_else(|_| root.join("target"), PathBuf::from);
            let directory = target_dir.join("forge-bdd-cert");
            let made = (directory.join("cert.pem"), directory.join("key.pem"));
            if !(made.0.is_file() && made.1.is_file()) {
                make_certificate(&directory, &made.0, &made.1);
            }
            made
        })
        .clone()
}

/// A self-signed certificate for `127.0.0.1` and `localhost`, valid for a year.
fn make_certificate(directory: &Path, certificate: &Path, key: &Path) {
    std::fs::create_dir_all(directory).expect("create the certificate directory");
    let status = std::process::Command::new("openssl")
        .args(["req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1"])
        .args(["-nodes", "-days", "365", "-subj", "/CN=localhost"])
        .args(["-addext", "subjectAltName=DNS:localhost,IP:127.0.0.1"])
        .arg("-keyout")
        .arg(key)
        .arg("-out")
        .arg(certificate)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap_or_else(|e| {
            panic!(
                "the sage, switchboard and warehouse suites need a TLS certificate and `openssl` \
                 could not be run to make one ({e}); put one at certs/cert.pem and certs/key.pem, or \
                 set FORGE_BDD_CERT and FORGE_BDD_KEY"
            )
        });
    assert!(status.success(), "openssl could not make a certificate");
}

pub async fn wait_until_ready(urls: &[(Target, String)]) {
    let client = quench_client::transport::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("http client");

    for (target, url) in urls {
        let mut ready = false;
        for _ in 0..60 {
            sleep(Duration::from_millis(500)).await;
            // A listener that accepts and never answers (plain HTTP behind an HTTPS client) would wait forever.
            let answered =
                tokio::time::timeout(Duration::from_secs(5), client.get(url).send()).await;
            if matches!(answered, Ok(Ok(_))) {
                ready = true;
                break;
            }
        }
        if ready {
            println!("{} ready", target.tag());
        } else {
            eprintln!("warning: {} did not answer at {url}", target.tag());
        }
    }
}
