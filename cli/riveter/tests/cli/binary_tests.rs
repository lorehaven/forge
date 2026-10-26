//! The `riveter` binary itself, run the way a user runs it: in a project directory, with a `kubectl`
//! that only records what it was asked. Everything `main` dispatches is reached from here.
//!
//! Each run is a child process with its own working directory and `PATH`, so nothing is shared with
//! the other tests and no lock is needed.

use quench_test::mock::matchers::{method, path};
use quench_test::mock::{Mock, MockServer, ResponseTemplate};
use std::fs;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const OVERLAY: &str = "\
env: dev
kube_context: ctx-dev
namespace_name: demo

resources:
  - kind: namespace
    immutable: true
  - kind: deployment
    name: api
    image: nginx
";

struct Run {
    ok: bool,
    /// stdout and stderr together: which of the two a status line goes to is not what is tested.
    out: String,
}

struct Project {
    dir: quench_core::tmp::TempDir,
    kubectl_log: PathBuf,
}

impl Project {
    fn new() -> Self {
        let dir = quench_core::tmp::TempDir::new().unwrap();
        for env in ["dev", "prod"] {
            let overlay = dir.path().join("overlays").join(env);
            fs::create_dir_all(&overlay).unwrap();
            fs::write(
                overlay.join("overlay.yaml"),
                OVERLAY
                    .replace("dev", env)
                    .replace("ctx-dev", &format!("ctx-{env}")),
            )
            .unwrap();
        }
        fs::write(
            dir.path().join("overlays/dev/rivet.toml"),
            "[package]\nname = \"dev\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        // A kubectl that records its arguments, answers the few questions riveter asks, and exits
        // with $KUBECTL_DIFF_EXIT (0 unless a test says otherwise) when asked for a diff.
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let kubectl_log = dir.path().join("kubectl.log");
        let script = "#!/bin/sh
echo \"$@\" >> \"$KUBECTL_LOG\"
case \" $* \" in
  *\" diff \"*) exit \"$KUBECTL_DIFF_EXIT\" ;;
  *\" get namespace \"*) echo namespace/demo ;;
  *\" config current-context \"*) echo test ;;
esac
exit 0
";
        fs::write(bin.join("kubectl"), script).unwrap();
        fs::set_permissions(bin.join("kubectl"), fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir, kubectl_log }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_riveter"));
        let path = format!(
            "{}:{}",
            self.path().join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        command
            .current_dir(self.path())
            .args(args)
            .env("PATH", path)
            .env("KUBECTL_LOG", &self.kubectl_log)
            .env("KUBECTL_DIFF_EXIT", "0")
            .env("NO_COLOR", "1")
            .env_remove("RIVETER_ENV")
            .env_remove("RIVETER_WAREHOUSE_URL")
            .env_remove("RIVETER_WAREHOUSE_TOKEN")
            .env_remove("RIVETER_GATEHOUSE_URL")
            .stdin(Stdio::null());
        command
    }

    fn run(&self, args: &[&str]) -> Run {
        finish(&self.command(args).output().unwrap())
    }

    fn kubectl_calls(&self) -> Vec<String> {
        fs::read_to_string(&self.kubectl_log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

fn finish(output: &std::process::Output) -> Run {
    Run {
        ok: output.status.success(),
        out: format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    }
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t"])
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

// --- environments ---------------------------------------------------------------

#[quench_test::test]
fn env_commands_list_set_and_show_the_environment() {
    let p = Project::new();
    let listed = p.run(&["env", "list"]);
    assert!(listed.ok, "{}", listed.out);
    assert!(
        listed.out.contains("dev") && listed.out.contains("prod"),
        "{}",
        listed.out
    );

    assert!(!p.run(&["env", "show"]).ok, "nothing is set yet");
    let bad = p.run(&["env", "set", "nope"]);
    assert!(
        !bad.ok && bad.out.contains("overlay not found"),
        "{}",
        bad.out
    );

    assert!(p.run(&["env", "set", "dev"]).ok);
    assert!(
        fs::read_to_string(p.path().join(".riveter.toml"))
            .unwrap()
            .contains("dev")
    );
    let shown = p.run(&["env", "show"]);
    assert!(
        shown.out.contains("current environment: dev"),
        "{}",
        shown.out
    );

    let from_var = p
        .command(&["env", "show"])
        .env("RIVETER_ENV", "prod")
        .output()
        .unwrap();
    let from_var = finish(&from_var);
    assert!(
        from_var.out.contains("prod (from $RIVETER_ENV)"),
        "{}",
        from_var.out
    );
}

#[quench_test::test]
fn list_and_render_work_on_the_chosen_environment() {
    let p = Project::new();
    let all = p.run(&["--env", "dev", "list"]);
    assert!(all.ok, "{}", all.out);
    assert!(
        all.out.contains("deployment") && all.out.contains("namespace"),
        "{}",
        all.out
    );

    let immutable = p.run(&["-e", "dev", "list", "--scope", "immutable"]);
    assert!(
        immutable.out.contains("namespace") && !immutable.out.contains("api"),
        "{}",
        immutable.out
    );
    let targeted = p.run(&["-e", "dev", "ls", "deployment/api"]);
    assert!(
        targeted.out.contains("api") && !targeted.out.contains("namespace/"),
        "{}",
        targeted.out
    );

    let rendered = p.run(&["-e", "dev", "render"]);
    assert!(rendered.ok, "{}", rendered.out);
    assert!(rendered.out.contains("rendered"), "{}", rendered.out);
    // `render` defaults to the mutable resources: the immutable namespace is left out, and said so
    let manifest =
        fs::read_to_string(p.path().join("manifests/dev-manifests.mutable.yaml")).unwrap();
    assert!(
        manifest.contains("kind: Deployment") && !manifest.contains("kind: Namespace"),
        "{manifest}"
    );
    assert!(
        rendered.out.contains("outside this scope: namespace/demo"),
        "{}",
        rendered.out
    );
    let everything = p.run(&["-e", "dev", "render", "--scope", "all"]);
    assert!(
        everything.ok && everything.out.contains("rendered 2 resource(s)"),
        "{}",
        everything.out
    );
    assert!(p.path().join("manifests/dev-manifests.yaml").is_file());

    assert!(!p.run(&["-e", "dev", "render", "statefulset/none"]).ok);
    assert!(!p.run(&["-e", "missing", "list"]).ok);
    assert!(!p.run(&["list"]).ok, "no environment chosen");
}

#[quench_test::test]
fn validate_checks_an_environment_or_files() {
    let p = Project::new();
    assert!(p.run(&["-e", "dev", "render", "--scope", "all"]).ok);
    let env = p.run(&["-e", "dev", "validate"]);
    assert!(env.ok, "{}", env.out);

    let file = p.run(&["validate", "-f", "manifests/dev-manifests.yaml"]);
    assert!(file.ok, "{}", file.out);

    let both = p.run(&[
        "validate",
        "-f",
        "manifests/dev-manifests.yaml",
        "deployment",
    ]);
    assert!(
        !both.ok && both.out.contains("with --file there is none"),
        "{}",
        both.out
    );
    assert!(!p.run(&["validate", "-f", "no-such-file.yaml"]).ok);
}

#[quench_test::test]
fn help_shows_the_tree_a_command_and_the_targets() {
    let p = Project::new();
    let tree = p.run(&["help"]);
    assert!(tree.ok && tree.out.contains("render"), "{}", tree.out);
    let one = p.run(&["help", "render"]);
    assert!(
        one.ok && one.out.to_lowercase().contains("scope"),
        "{}",
        one.out
    );
    // an alias finds the same command
    assert!(p.run(&["help", "r"]).ok);
    let targets = p.run(&["help", "targets"]);
    assert!(
        targets.ok && targets.out.contains("deployment"),
        "{}",
        targets.out
    );
    let unknown = p.run(&["help", "frobnicate"]);
    assert!(!unknown.ok, "{}", unknown.out);
}

// --- diff against a git ref --------------------------------------------------------

#[quench_test::test]
fn diff_against_compares_an_environment_with_its_history() {
    let p = Project::new();
    git(p.path(), &["init", "-q"]);
    git(p.path(), &["add", "overlays"]);
    git(p.path(), &["commit", "-q", "-m", "first"]);

    let same = p.run(&["-e", "dev", "diff", "--against", "HEAD"]);
    assert!(
        same.ok && same.out.contains("renders the same as HEAD"),
        "{}",
        same.out
    );

    let overlay = p.path().join("overlays/dev/overlay.yaml");
    fs::write(
        &overlay,
        fs::read_to_string(&overlay)
            .unwrap()
            .replace("nginx", "nginx:1.27"),
    )
    .unwrap();
    let changed = p.run(&["-e", "dev", "diff", "--against", "HEAD"]);
    assert!(
        changed.ok && changed.out.contains("renders differently from HEAD"),
        "{}",
        changed.out
    );

    let list = p.run(&["-e", "dev,prod", "diff", "--against", "HEAD"]);
    assert!(
        !list.ok && list.out.contains("name one `--env`"),
        "{}",
        list.out
    );
}

// --- commands that reach the cluster ----------------------------------------------------

#[quench_test::test]
fn apply_goes_through_kubectl_with_the_overlays_context() {
    let p = Project::new();
    let dry = p.run(&["-e", "dev", "apply", "--dry-run", "--scope", "all"]);
    assert!(dry.ok && dry.out.contains("would apply"), "{}", dry.out);
    let real = p.run(&["-e", "dev", "apply", "--no-wait", "--scope", "all"]);
    assert!(
        real.ok && real.out.contains("applied 2 resource(s)"),
        "{}",
        real.out
    );
    let calls = p.kubectl_calls();
    assert!(
        calls.iter().all(|c| c.starts_with("--context ctx-dev")),
        "{calls:?}"
    );
    assert!(
        calls.iter().any(|c| c.contains("--dry-run=client")),
        "{calls:?}"
    );

    // a selector that matches nothing in scope applies nothing
    let none = p.run(&[
        "-e",
        "dev",
        "apply",
        "--no-wait",
        "--scope",
        "immutable",
        "deployment",
    ]);
    assert!(
        !none.ok || none.out.contains("no resources matched"),
        "{}",
        none.out
    );

    // several environments are applied in the order given
    let many = p.run(&["-e", "dev,prod", "apply", "--no-wait", "--scope", "all"]);
    assert!(many.ok, "{}", many.out);
    let contexts: Vec<_> = p
        .kubectl_calls()
        .iter()
        .filter_map(|c| c.split_whitespace().nth(1).map(str::to_string))
        .collect();
    let (dev, prod) = (
        contexts.iter().rposition(|c| c == "ctx-dev").unwrap(),
        contexts.iter().rposition(|c| c == "ctx-prod").unwrap(),
    );
    assert!(dev < prod, "{contexts:?}");
}

#[quench_test::test]
fn diff_reports_whether_the_cluster_differs() {
    let p = Project::new();
    let same = p.run(&["-e", "dev", "diff", "--scope", "all"]);
    assert!(
        same.ok && same.out.contains("cluster matches"),
        "{}",
        same.out
    );

    let differs = finish(
        &p.command(&["-e", "dev", "diff", "--scope", "all"])
            .env("KUBECTL_DIFF_EXIT", "1")
            .output()
            .unwrap(),
    );
    assert!(
        differs.ok && differs.out.contains("the cluster differs"),
        "{}",
        differs.out
    );

    let broken = finish(
        &p.command(&["-e", "dev", "diff", "--scope", "all"])
            .env("KUBECTL_DIFF_EXIT", "2")
            .output()
            .unwrap(),
    );
    assert!(!broken.ok, "{}", broken.out);

    let many = p.run(&["-e", "dev,prod", "diff", "--scope", "all"]);
    assert!(many.ok, "{}", many.out);
}

#[quench_test::test]
fn delete_and_prune_go_through_kubectl() {
    let p = Project::new();
    let deleted = p.run(&["-e", "dev", "delete", "--scope", "all"]);
    assert!(
        deleted.ok && deleted.out.contains("deleted 2 resource(s)"),
        "{}",
        deleted.out
    );
    assert!(
        p.kubectl_calls().iter().any(|c| c.contains(" delete -f ")),
        "{:?}",
        p.kubectl_calls()
    );
    let nothing = p.run(&["-e", "dev", "delete", "--scope", "immutable", "deployment"]);
    assert!(
        !nothing.ok || nothing.out.contains("no resources matched"),
        "{}",
        nothing.out
    );

    // nothing is live, so there is nothing to prune
    let pruned = p.run(&["-e", "dev", "prune", "--dry-run"]);
    assert!(pruned.ok, "{}", pruned.out);
}

// --- packages ----------------------------------------------------------------------------

#[quench_test::test]
fn pack_then_install_a_package_file() {
    let p = Project::new();
    let packed = p.run(&["-e", "dev", "pack", "--no-pin", "--out", "packages"]);
    assert!(
        packed.ok && packed.out.contains("packed dev 0.1.0"),
        "{}",
        packed.out
    );
    assert!(p.path().join("packages/dev-0.1.0.rivet").is_file());

    let inventory = p.run(&[
        "install",
        "packages/dev-0.1.0.rivet",
        "--dry-run",
        "--inventory",
        "--except",
        "namespace",
    ]);
    assert!(inventory.ok, "{}", inventory.out);
    assert!(
        inventory.out.contains("riveter-inventory:"),
        "{}",
        inventory.out
    );
    assert!(
        inventory
            .out
            .contains("would apply 1 resource(s): deployment/api"),
        "{}",
        inventory.out
    );

    assert!(
        !p.run(&["pack", "--no-pin"]).ok,
        "pack needs an environment"
    );
    assert!(
        !p.run(&["-e", "dev", "pull", "dev"]).ok,
        "no Warehouse is configured"
    );
    assert!(!p.run(&["install", "ghost-package"]).ok);
}

#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_and_publish_talk_to_warehouse() {
    let p = Project::new();
    let server = MockServer::start().await;
    let record = quench_serde::json!({
        "name": "dev", "version": "0.1.0", "description": "d", "namespace": "demo",
        "size_bytes": 10, "sha256": "ab", "yanked": false, "uploaded_by": "ci",
    });
    Mock::given(method("GET"))
        .and(path("/w/api/v1/rivets"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            quench_serde::json::to_string(&quench_serde::json!([record.clone()])),
            "application/json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_raw(quench_serde::json::to_string(&record), "application/json"),
        )
        .mount(&server)
        .await;

    let url = format!("{}/w", server.uri());
    let with_warehouse = |args: &[&str]| {
        let mut command = p.command(args);
        command
            .env("RIVETER_WAREHOUSE_URL", &url)
            .env("RIVETER_WAREHOUSE_TOKEN", "tok");
        command
    };
    let listed = {
        let mut command = with_warehouse(&["remote", "list"]);
        tokio::task::spawn_blocking(move || command.output().unwrap())
            .await
            .unwrap()
    };
    let listed = finish(&listed);
    assert!(listed.ok && listed.out.contains("0.1.0"), "{}", listed.out);

    let published = {
        let mut command =
            with_warehouse(&["-e", "dev", "publish", "--no-pin", "--out", "packages"]);
        tokio::task::spawn_blocking(move || command.output().unwrap())
            .await
            .unwrap()
    };
    let published = finish(&published);
    assert!(
        published.ok && published.out.contains("published dev 0.1.0"),
        "{}",
        published.out
    );
}

// --- the rest ----------------------------------------------------------------------------------

#[quench_test::test]
fn schemas_and_secrets_commands_are_dispatched() {
    let p = Project::new();
    let schemas = p.run(&["schemas", "list"]);
    assert!(
        schemas.ok && schemas.out.contains("bundled"),
        "{}",
        schemas.out
    );

    let keygen = p.run(&["secrets", "keygen", "--out", "age.key"]);
    assert!(keygen.ok, "{}", keygen.out);
    assert!(p.path().join("age.key").is_file());
    assert!(!p.run(&["secrets", "list", "no-such-secrets.yaml"]).ok);
}

#[quench_test::test]
fn login_needs_a_gatehouse_and_a_terminal() {
    let p = Project::new();
    let no_url = p.run(&["login"]);
    assert!(
        !no_url.ok && no_url.out.contains("no Gatehouse URL"),
        "{}",
        no_url.out
    );

    // with a URL and a user, the password prompt needs a terminal that a test does not have
    let no_tty = p.run(&[
        "login",
        "--gatehouse-url",
        "http://127.0.0.1:1",
        "--username",
        "me",
    ]);
    assert!(!no_tty.ok, "{}", no_tty.out);
}

#[quench_test::test]
fn the_repl_is_the_default_and_runs_commands_from_stdin() {
    let p = Project::new();
    let mut child = p
        .command(&[])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"help\nenv list\nenv set dev\nlist\nrender\nbogus\nexit\n")
        .unwrap();
    let out = finish(&child.wait_with_output().unwrap());
    assert!(out.ok, "{}", out.out);
    assert!(out.out.contains("unknown command"), "{}", out.out);
    assert!(out.out.contains("deployment"), "{}", out.out);

    // end of input quits too, and `repl` names it explicitly
    let eof = p.run(&["repl"]);
    assert!(eof.ok, "{}", eof.out);

    // the version flag is answered by the argument parser
    assert!(p.run(&["--version"]).out.contains("riveter"));
    assert!(!p.run(&["no-such-command"]).ok);
}
