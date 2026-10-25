//! `forge-sectest` command line.

use forge_sectest::census;
use forge_sectest::finding::{Severity, report_json};
use forge_sectest::scan::{self, ALL_CHECKS, Options};
use forge_sectest::wire::Target;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "\
forge-sectest - black-box security scanner for the forge services

USAGE
  forge-sectest census [--forge DIR] [--quench DIR]
  forge-sectest scan   --target NAME=URL [--target ...] [options]

SCAN OPTIONS
  --target NAME=URL     a running service (NAME is the docker/<NAME>-service directory name)
  --forge DIR           forge checkout to read routes from            [default: .]
  --quench DIR          quench checkout; adds quench-starter's shared routes  [default: found automatically]
  --token TOKEN         a valid bearer token, to test behaviour behind the auth wall
  --baseline FILE       reviewed public routes; only routes not listed are reported
  --write-baseline FILE write the current public routes there and exit 0
  --checks a,b,c        subset of: auth,tokens,traversal,headers,cors,injection,protocol,throttle
  --active              also send mutating verbs and credential bursts (state-changing)
  --slow                also run the 60s slowloris hold test
  --rps N               request rate limit                            [default: 20]
  --insecure            accept invalid TLS certificates
  --allow-remote        permit targets outside loopback/private ranges (only systems you own)
  --fail-on LEVEL       exit 1 at or above info|low|medium|high|critical   [default: high]
  --json FILE           also write findings as JSON

Non-destructive by default. Never point this at a system you are not authorised to test.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Ok(runtime) = tokio::runtime::Builder::new_multi_thread().enable_all().build() else {
        eprintln!("cannot start the async runtime");
        return ExitCode::from(2);
    };
    match runtime.block_on(real_main(args)) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

struct Args(Vec<String>);

impl Args {
    fn flag(&self, name: &str) -> bool {
        self.0.iter().any(|a| a == name)
    }

    fn values(&self, name: &str) -> Vec<String> {
        self.0
            .windows(2)
            .filter(|w| w[0] == name)
            .map(|w| w[1].clone())
            .collect()
    }

    fn value(&self, name: &str) -> Option<String> {
        self.values(name).into_iter().next()
    }
}

async fn real_main(raw: Vec<String>) -> Result<ExitCode, String> {
    let Some(command) = raw.first().cloned() else {
        print!("{HELP}");
        return Ok(ExitCode::SUCCESS);
    };
    let args = Args(raw[1..].to_vec());
    if matches!(command.as_str(), "-h" | "--help" | "help") {
        print!("{HELP}");
        return Ok(ExitCode::SUCCESS);
    }

    let forge = PathBuf::from(args.value("--forge").unwrap_or_else(|| ".".to_string()));
    let extra: Vec<PathBuf> = args.value("--quench").map_or_else(
        || census::quench_route_sources(&forge),
        |q| {
            let q = PathBuf::from(q);
            vec![q.join("quench-starter/src"), q.join("quench-auth/src")]
        },
    );
    let routes = census::scan_forge(&forge, &extra);
    if routes.is_empty() {
        return Err(format!("no routes found under {}/docker (is --forge a forge checkout?)", forge.display()));
    }

    match command.as_str() {
        "census" => {
            for route in &routes {
                println!("{:<12} {:<7} {}  ({})", route.service, route.method, route.pattern, route.source);
            }
            println!("\n{} routes", routes.len());
            Ok(ExitCode::SUCCESS)
        }
        "scan" => scan_command(&args, routes).await,
        other => Err(format!("unknown command {other:?}; try --help")),
    }
}

async fn scan_command(args: &Args, routes: Vec<census::Route>) -> Result<ExitCode, String> {
    let mut targets = Vec::new();
    for spec in args.values("--target") {
        let (name, url) = spec.split_once('=').ok_or_else(|| format!("--target {spec}: expected NAME=URL"))?;
        let mut target = Target::parse(name, url)?;
        target.insecure = args.flag("--insecure");
        targets.push(target);
    }
    let mut options = Options::new(targets, routes);
    options.token = args.value("--token");
    options.active = args.flag("--active");
    options.slow = args.flag("--slow");
    options.allow_remote = args.flag("--allow-remote");
    if let Some(rps) = args.value("--rps") {
        options.rps = rps.parse().map_err(|_| "--rps: not a number".to_string())?;
    }
    if let Some(list) = args.value("--checks") {
        options.checks = list.split(',').map(str::trim).map(str::to_string).collect();
        if let Some(bad) = options.checks.iter().find(|c| !ALL_CHECKS.contains(&c.as_str())) {
            return Err(format!("--checks: unknown check {bad:?}"));
        }
    }
    let write_baseline = args.value("--write-baseline");
    if let Some(path) = args.value("--baseline") {
        let text = fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
        options.baseline = Some(
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_string)
                .collect::<HashSet<_>>(),
        );
    }
    let fail_on = match args.value("--fail-on") {
        Some(level) => Severity::parse(&level).ok_or_else(|| format!("--fail-on: unknown level {level:?}"))?,
        None => Severity::High,
    };

    if write_baseline.is_some() {
        // Everything reachable anonymously becomes the baseline; nothing else is run.
        options.checks = vec!["auth".to_string()];
        options.baseline = None;
    }

    let (findings, requests) = scan::run(options).await?;

    if let Some(path) = write_baseline {
        let mut keys: Vec<String> = findings
            .iter()
            .filter(|f| f.check == "auth" && f.method != "-")
            .map(|f| format!("{} {} {}", f.service, f.method, f.path))
            .collect();
        keys.sort();
        keys.dedup();
        let body = format!(
            "# Reviewed anonymous routes (service METHOD pattern). Written by forge-sectest.\n# Every line is a deliberate decision that this route needs no credentials.\n{}\n",
            keys.join("\n")
        );
        fs::write(&path, body).map_err(|e| format!("{path}: {e}"))?;
        println!("wrote {} public routes to {path}; review them before committing", keys.len());
        return Ok(ExitCode::SUCCESS);
    }

    for finding in &findings {
        println!("{finding}");
    }
    if let Some(path) = args.value("--json") {
        fs::write(&path, report_json(&findings)).map_err(|e| format!("{path}: {e}"))?;
    }
    let worst = findings.iter().map(|f| f.severity).max();
    println!(
        "\n{} findings from {requests} requests{}",
        findings.len(),
        worst.map_or(String::new(), |w| format!("; worst: {w}"))
    );
    Ok(if worst.is_some_and(|w| w >= fail_on) { ExitCode::from(1) } else { ExitCode::SUCCESS })
}
