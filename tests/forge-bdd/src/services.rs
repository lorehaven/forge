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
/// `LoggingSender` (`docker/gatehouse-service/src/email.rs`) writes the
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

    fn service_dir(&self, package: &str) -> PathBuf {
        self.workspace_root.join("docker").join(package)
    }

    fn binary(&self, package: &str) -> PathBuf {
        let target_dir = std::env::var("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| self.workspace_root.join("target"));
        target_dir.join("debug").join(package)
    }

    async fn build(&self, package: &str) {
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
    }

    /// Environment every service shares: one realm (gatehouse, always
    /// started alongside whatever was selected - see `main.rs`), an
    /// in-memory database so the suite needs no Postgres.
    fn common_env(&self, command: &mut Command, service_name: &str) {
        command
            .env("SERVICE_NAME", service_name)
            .env("SERVER_CERT_PATH", "cert.pem")
            .env("SERVER_KEY_PATH", "key.pem")
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
        }
    }

    /// Conveyor, with no database.
    ///
    /// Its queue needs Postgres and says so; the suite deliberately runs on an
    /// in-memory store, so these scenarios cover what a database cannot change:
    /// the UI shell, gatehouse delegation, which routes need a token, and the
    /// webhook endpoint's refusals. Everything that does touch the queue is
    /// covered by `docker/conveyor-service/tests/integration`, against a real
    /// Postgres.
    async fn start_conveyor(&self) -> Service {
        self.build("conveyor-service").await;
        println!("Starting conveyor-service...");

        let mut command = Command::new(self.binary("conveyor-service"));
        command
            .current_dir(self.service_dir("conveyor-service"))
            .envs(std::env::vars());
        self.common_env(&mut command, "conveyor");
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
    /// `docker/workbench-service/tests/integration`, against a real Postgres.
    async fn start_workbench(&self) -> Service {
        self.build("workbench-service").await;
        println!("Starting workbench-service...");

        let mut command = Command::new(self.binary("workbench-service"));
        command
            .current_dir(self.service_dir("workbench-service"))
            .envs(std::env::vars());
        self.common_env(&mut command, "workbench");
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

        Service {
            target: Target::Workbench,
            child: command.spawn().expect("failed to start workbench-service"),
        }
    }

    async fn start_sage(&self) -> Service {
        self.build("sage-service").await;
        println!("Starting sage-service...");

        let mut command = Command::new(self.binary("sage-service"));
        command
            .current_dir(self.service_dir("sage-service"))
            .envs(std::env::vars());
        self.common_env(&mut command, "sage");
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
        self.common_env(&mut command, "switchboard");
        command
            .env("SERVER_ADDR", "127.0.0.1:8554")
            .env("SERVER_HTTP_REDIRECT_ADDR", "127.0.0.1:8081")
            .env("BASE_PATH", "/switchboard")
            .env("DB_SCHEMA", "test_switchboard")
            .env("SERVICE_AUTH_ENABLED", "true")
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
        self.common_env(&mut command, "warehouse");
        command
            .env("SERVER_ADDR", "127.0.0.1:8443")
            .env("SERVER_HTTP_REDIRECT_ADDR", "127.0.0.1:8080")
            .env("BASE_PATH", "/warehouse")
            .env("DB_SCHEMA", "test_warehouse")
            .env("SERVICE_AUTH_ENABLED", "true")
            .env("FEATURE_CRATES_ENABLED", "true")
            .env("FEATURE_DOCKER_ENABLED", "true")
            .env("FEATURE_FILES_ENABLED", "true")
            .env("CRATES_STORAGE_PATH", storage_dir.join("crates"))
            .env("STORAGE_PATH", storage_dir.join("docker"))
            .env(
                "FILE_STORAGES",
                format!("test={}", storage_dir.join("files").display()),
            );

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
        self.common_env(&mut command, "gatehouse");
        command
            .env("SERVER_ADDR", "127.0.0.1:5443")
            .env("SERVER_HTTP_REDIRECT_ADDR", "127.0.0.1:5080")
            .env("BASE_PATH", "/gatehouse")
            .env("SERVICE_AUTH_ENABLED", "true")
            // Plain HTTP, deterministically: gatehouse ships no certificate of
            // its own, but the dev environment (foreman) symlinks one in, and the
            // suite must not change behaviour depending on whether that ran.
            .env("SERVER_CERT_PATH", "/nonexistent/bdd-no-tls.pem")
            .env("SERVER_KEY_PATH", "/nonexistent/bdd-no-tls.key")
            // Ignored now (the permission catalog decides audiences - see
            // `docker/gatehouse-service/config/permissions.toml`, which
            // already lists every BDD service), kept for anyone still
            // reading this env block for the old story.
            .env(
                "SERVICE_AUDIENCES",
                "sage,switchboard,warehouse,conveyor,workbench,gatehouse",
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
            // Mirrors `docker/gatehouse-service/config/clients.toml`, with
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
            // `redirect_base_url` in `docker/gatehouse-service/src/clients.rs`.
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
            .stdout(Stdio::piped());

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
pub async fn wait_until_ready(urls: &[(Target, String)]) {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("http client");

    for (target, url) in urls {
        let mut ready = false;
        for _ in 0..60 {
            sleep(Duration::from_millis(500)).await;
            if client.get(url).send().await.is_ok() {
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
