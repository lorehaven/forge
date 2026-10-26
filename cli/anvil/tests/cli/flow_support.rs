//! A throwaway two-crate workspace (`b` depends on `a`) in a git repository with a bare remote of its
//! own, and the tools anvil shells out to as shims that only record what they were asked.
//!
//! `anvil` runs as a child process, so none of this touches this repository or the test process's
//! own working directory. `cargo` is real for `metadata` (the planner needs it) and recorded for
//! everything else - nothing is ever built, published or pushed to a registry. `docker` is recorded.

use std::fs;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct Output {
    pub ok: bool,
    pub text: String,
}

pub struct Workspace {
    dir: quench_core::tmp::TempDir,
    log: PathBuf,
}

pub fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn real_cargo() -> PathBuf {
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join("cargo"))
                .find(|candidate| candidate.is_file())
        })
        .expect("cargo on PATH")
}

pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.email=ci@example.com", "-c", "user.name=ci"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

impl Workspace {
    pub fn new() -> Self {
        let dir = quench_core::tmp::TempDir::new().unwrap();
        let work = dir.path().join("work");
        write(
            &work.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/a\", \"crates/b\"]\nresolver = \"2\"\n",
        );
        write(
            &work.join("crates/a/Cargo.toml"),
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        write(&work.join("crates/a/src/lib.rs"), "pub fn a() {}\n");
        write(
            &work.join("crates/b/Cargo.toml"),
            "[package]\nname = \"b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\na = { path = \"../a\", version = \"0.1.0\" }\n",
        );
        write(&work.join("crates/b/src/lib.rs"), "pub fn b() { a::a() }\n");
        write(
            &work.join(".anvil.toml"),
            "[release]\nregistry = \"test\"\npackages = [\"a\", \"b\"]\n",
        );
        write(&work.join(".gitignore"), "target/\n");

        let status = Command::new(real_cargo())
            .current_dir(&work)
            .args(["generate-lockfile", "--offline"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());

        let remote = dir.path().join("remote.git");
        git(
            dir.path(),
            &["init", "-q", "--bare", remote.to_str().unwrap()],
        );
        git(&work, &["init", "-q", "-b", "main"]);
        git(&work, &["add", "."]);
        git(&work, &["commit", "-q", "-m", "initial"]);
        git(
            &work,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        git(&work, &["push", "-q", "-u", "origin", "main"]);

        let bin = dir.path().join("bin");
        write(
            &bin.join("cargo"),
            &format!(
                "#!/bin/sh
case \"$1\" in
  metadata|generate-lockfile|locate-project|version|--version) exec '{}' \"$@\" ;;
  publish)
    echo \"$@\" >> \"$ANVIL_TEST_LOG\"
    case \"$*\" in *\"--package $ANVIL_TEST_FAIL_PUBLISH\"*) echo 'publish refused' >&2; exit 1 ;; esac
    exit 0 ;;
  *)
    echo \"$@\" >> \"$ANVIL_TEST_LOG\"
    case \"$*\" in *\"$ANVIL_TEST_FAIL\"*) echo 'command failed' >&2; exit 1 ;; esac
    exit 0 ;;
esac
",
                real_cargo().display()
            ),
        );
        write(
            &bin.join("docker"),
            "#!/bin/sh
echo \"docker $@\" >> \"$ANVIL_TEST_LOG\"
case \"$*\" in *\"$ANVIL_TEST_FAIL\"*) echo 'docker failed' >&2; exit 1 ;; esac
exit 0
",
        );
        for tool in ["cargo", "docker"] {
            fs::set_permissions(bin.join(tool), fs::Permissions::from_mode(0o755)).unwrap();
        }

        let log = dir.path().join("cargo.log");
        Self { dir, log }
    }

    pub fn work(&self) -> PathBuf {
        self.dir.path().join("work")
    }

    pub fn run(&self, args: &[&str], stdin: &str) -> Output {
        self.run_failing(args, stdin, "-no-package-")
    }

    /// `fail_publish` names a package whose `cargo publish` is refused.
    pub fn run_failing(&self, args: &[&str], stdin: &str, fail_publish: &str) -> Output {
        self.run_with(args, stdin, &[("ANVIL_TEST_FAIL_PUBLISH", fail_publish)])
    }

    /// Runs `anvil` with extra environment. `ANVIL_TEST_FAIL` names text that makes a shimmed
    /// `cargo` or `docker` call fail when its arguments contain it.
    pub fn run_with(&self, args: &[&str], stdin: &str, envs: &[(&str, &str)]) -> Output {
        let path = format!(
            "{}:{}",
            self.dir.path().join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut command = Command::new(env!("CARGO_BIN_EXE_anvil"));
        command
            .current_dir(self.work())
            .args(args)
            .env("PATH", path)
            .env("ANVIL_TEST_LOG", &self.log)
            .env("ANVIL_TEST_FAIL_PUBLISH", "-no-package-")
            .env("ANVIL_TEST_FAIL", "-matches-nothing-")
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in envs {
            command.env(key, value);
        }
        let mut child = command.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        Output {
            ok: output.status.success(),
            text: format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        }
    }

    /// Every call the shims recorded, in order: `cargo ...` arguments, and `docker ...` calls.
    pub fn calls(&self) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The `cargo publish` / `cargo build` calls, in order.
    pub fn cargo_calls(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|c| c.starts_with("publish") || c.starts_with("build"))
            .collect()
    }

    /// Replaces `.anvil.toml`.
    pub fn configure(&self, toml: &str) {
        write(&self.work().join(".anvil.toml"), toml);
    }

    pub fn tags(&self) -> Vec<String> {
        git(&self.work(), &["tag", "--list"])
            .lines()
            .map(str::to_string)
            .collect()
    }

    pub fn remote_tags(&self) -> Vec<String> {
        let out = git(&self.work(), &["ls-remote", "--tags", "origin"]);
        let mut tags: Vec<String> = out
            .lines()
            .filter_map(|l| l.split('\t').nth(1))
            .map(|r| r.trim_start_matches("refs/tags/").to_string())
            .collect();
        tags.sort();
        tags
    }

    pub fn change_crate_a(&self) {
        write(
            &self.work().join("crates/a/src/lib.rs"),
            "pub fn a() { /* changed */ }\n",
        );
        git(&self.work(), &["commit", "-q", "-am", "change a"]);
    }
}
