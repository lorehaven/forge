//! Starting and stopping the services under test.
//!
//! Services are built first and then launched from `target/debug` directly:
//! `cargo run` would leave the service alive when the suite kills its parent,
//! and a leaked process holds the port, so the next run silently tests the
//! stale instance instead of the one it just built.

use crate::world::Target;
use std::path::{Path, PathBuf};
use tokio::process::{Child, Command};
use tokio::time::{Duration, sleep};

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
    /// Shared by every service so one realm token verifies everywhere.
    jwt_secret: String,
}

impl Fixture {
    pub fn new(manifest_dir: &str) -> Self {
        let workspace_root = Path::new(manifest_dir)
            .parent()
            .expect("tests/ directory")
            .parent()
            .expect("workspace root")
            .to_path_buf();

        Self {
            workspace_root,
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "forge-bdd-shared-secret".to_string()),
        }
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

    /// Environment every service shares: one realm, one signing key, an
    /// in-memory database so the suite needs no Postgres.
    fn common_env(&self, command: &mut Command, service_name: &str) {
        command
            .env("JWT_SECRET", &self.jwt_secret)
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
            .kill_on_drop(true);
    }

    pub async fn start(&self, target: Target) -> Service {
        match target {
            Target::Sage => self.start_sage().await,
            Target::Switchboard => self.start_switchboard().await,
            Target::Warehouse => self.start_warehouse().await,
            Target::Gatehouse => self.start_gatehouse().await,
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
            .env("SWITCHBOARD_TECH_USERNAME", "admin")
            .env("SWITCHBOARD_TECH_PASSWORD", "password")
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
            .env("SERVICE_AUTH_ENABLED", "false")
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
            .env("STORAGE_PATH", storage_dir.join("docker"));

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
            // its own, but the dev script (run.fish) symlinks one in, and the
            // suite must not change behaviour depending on whether that ran.
            .env("SERVER_CERT_PATH", "/nonexistent/bdd-no-tls.pem")
            .env("SERVER_KEY_PATH", "/nonexistent/bdd-no-tls.key")
            // Tokens gatehouse mints are valid across the whole realm.
            .env("SERVICE_AUDIENCES", "sage,switchboard,warehouse,gatehouse")
            .env(
                "AUTH_REDIRECT_HOSTS",
                "https://127.0.0.1:8443,http://127.0.0.1:8443",
            )
            // Home page fixture: two services configured, one turned off by its
            // feature flag, so the suite covers both halves of the gating rule.
            .env("SAGE_UI_URL", "https://127.0.0.1:7777/sage/ui/home")
            .env(
                "SWITCHBOARD_UI_URL",
                "https://127.0.0.1:8554/switchboard/ui/home",
            )
            .env(
                "WAREHOUSE_UI_URL",
                "https://127.0.0.1:8443/warehouse/ui/home",
            )
            .env("FEATURE_WAREHOUSE_ENABLED", "false");

        Service {
            target: Target::Gatehouse,
            child: command.spawn().expect("failed to start gatehouse-service"),
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
