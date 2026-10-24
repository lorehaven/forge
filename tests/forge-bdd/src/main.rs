//! One entry point for every Forge BDD suite.
//!
//! ```text
//! cargo run -p forge-bdd                        # every service
//! cargo run -p forge-bdd -- --tags @gatehouse   # one service
//! cargo run -p forge-bdd -- --tags '@sage and @chat'
//! cargo run -p forge-bdd -- --service warehouse # same, without writing a tag expression
//! ```
//!
//! Only the services a run actually needs are started, decided from the tag
//! expression (or `--service`). Every feature carries its service's tag, so the
//! two stay in step.

use quench_args::Parser;
use quench_test_bdd::{Feature, Options, Runner, Scenario, Stats, Summary};
use std::panic::AssertUnwindSafe;
use world::{ForgeWorld, Target};

mod mocks;
mod services;
mod steps;
mod world;

/// The command line: the feature runner's own options, and a few of ours.
#[derive(Parser, Clone, Debug)]
struct Cli {
    #[command(flatten)]
    bdd: Options,

    /// Service to test; repeatable. Defaults to the services named in --tags,
    /// or all of them.
    #[arg(long = "service", value_name = "NAME")]
    services: Vec<String>,

    /// Assume the services are already running and do not start them.
    #[arg(long)]
    no_spawn: bool,
}

#[tokio::main]
async fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    quench_core::dotenv::load_from(std::path::Path::new(manifest_dir).join(".env")).ok();

    let opts = Cli::parse();
    let selected = selected_targets(&opts.services);

    println!(
        "Running suites: {}",
        selected
            .iter()
            .map(|target| target.tag())
            .collect::<Vec<_>>()
            .join(", ")
    );

    // sage talks to switchboard and vLLM; the mocks stand in for both so the
    // sage suite stays independent of the real switchboard.
    if selected.contains(&Target::Sage) {
        mocks::start().await;
    }

    let fixture = services::Fixture::new(manifest_dir);
    let mut running = Vec::new();

    // Every service verifies its tokens against gatehouse's JWKS now, and
    // `ForgeWorld::new()` mints a token that way for every scenario - so
    // gatehouse has to be running and answering before anything else starts,
    // and before the very first `ForgeWorld` (the probe, right below) is
    // built, whether or not a suite selected `@gatehouse` itself.
    if !opts.no_spawn {
        running.push(fixture.start(Target::Gatehouse).await);
        services::wait_until_ready(&[(
            Target::Gatehouse,
            "http://127.0.0.1:5443/gatehouse/ui/login".to_string(),
        )])
        .await;
        for target in selected
            .iter()
            .filter(|target| **target != Target::Gatehouse)
        {
            running.push(fixture.start(*target).await);
        }
    }

    let probe = ForgeWorld::new().await;
    let health_urls: Vec<(Target, String)> = selected
        .iter()
        .map(|target| {
            let url = match target {
                Target::Sage => format!("{}/ui/login", probe.sage_url),
                Target::Switchboard => format!("{}/health", probe.switchboard_url),
                Target::Warehouse => format!("{}/health", probe.warehouse_url),
                Target::Gatehouse => format!("{}/ui/login", probe.gatehouse_url),
                Target::Conveyor => format!("{}/health", probe.conveyor_url),
                Target::Workbench => format!("{}/health", probe.workbench_url),
                Target::Daybook => format!("{}/health", probe.daybook_url),
            };
            (*target, url)
        })
        .collect();
    services::wait_until_ready(&health_urls).await;

    // The sage shutdown scenario terminates the service under test, so it
    // cannot share a concurrent run with anything else - it goes last, alone.
    if let Some(sage) = running.iter().find(|s| s.target == Target::Sage)
        && let Some(pid) = sage.pid()
    {
        steps::sage::shutdown::set_sage_pid(pid);
    }

    let features_path = std::path::Path::new(manifest_dir).join("features");

    // One pass per service, so the closing summary can report each separately.
    let outcome = quench_core::future::catch_unwind(AssertUnwindSafe(async {
        let mut results = Vec::new();

        for target in &selected {
            let target = *target;
            println!("\n─── {} ───", target.tag());
            let summary = Runner::<ForgeWorld>::new()
                .options(opts.bdd.clone())
                .title(&format!("forge-bdd-{}", target.tag()))
                .filter(move |feature, _rule, scenario| {
                    feature.name != SHUTDOWN_FEATURE
                        && matches_selection(&[target], feature, Some(scenario))
                })
                .run(features_path.clone())
                .await;
            results.push(SuiteResult::collect(target, "scenarios", &summary));

            // The sage shutdown scenario terminates the service under test, so
            // it runs after the rest of sage's suite, on its own.
            if target == Target::Sage && !opts.no_spawn {
                let summary = Runner::<ForgeWorld>::new()
                    .options(opts.bdd.clone())
                    .title("forge-bdd-sage-shutdown")
                    .filter(|feature, _rule, _scenario| feature.name == SHUTDOWN_FEATURE)
                    .run(features_path.clone())
                    .await;
                results.push(SuiteResult::collect(target, "shutdown", &summary));
            }
        }

        results
    }))
    .await;

    for service in running {
        println!("Stopping {}...", service.target.tag());
        service.stop().await;
    }

    match outcome {
        Ok(results) => {
            let failed = print_summary(&results);
            if failed {
                std::process::exit(1);
            }
        }
        Err(_) => {
            eprintln!("\nRun aborted: a step panicked outside of a scenario.");
            std::process::exit(1);
        }
    }
}

const SHUTDOWN_FEATURE: &str = "Graceful shutdown";

/// What one pass of one suite did.
struct SuiteResult {
    target: Target,
    pass: &'static str,
    scenarios: Stats,
    steps: Stats,
}

impl SuiteResult {
    fn collect(target: Target, pass: &'static str, summary: &Summary) -> Self {
        Self {
            target,
            pass,
            scenarios: summary.scenarios,
            steps: summary.steps,
        }
    }

    fn failed(&self) -> bool {
        self.scenarios.failed > 0 || self.steps.failed > 0
    }
}

/// Closing report across every suite that ran. Returns whether anything failed.
fn print_summary(results: &[SuiteResult]) -> bool {
    let mut scenarios = (0, 0, 0);
    let mut steps = (0, 0, 0);

    println!("\n═══ Forge BDD summary ═══════════════════════════════════════════════");
    println!(
        "{:<14} {:>21}   {:>21}   result",
        "suite", "scenarios", "steps"
    );

    for result in results {
        let name = if result.pass == "scenarios" {
            result.target.tag().to_string()
        } else {
            format!("{} ({})", result.target.tag(), result.pass)
        };

        println!(
            "{:<14} {:>21}   {:>21}   {}",
            name,
            counts(&result.scenarios),
            counts(&result.steps),
            if result.failed() { "FAILED" } else { "ok" },
        );

        scenarios.0 += result.scenarios.passed;
        scenarios.1 += result.scenarios.failed;
        scenarios.2 += result.scenarios.skipped;
        steps.0 += result.steps.passed;
        steps.1 += result.steps.failed;
        steps.2 += result.steps.skipped;
    }

    let any_failed = scenarios.1 > 0 || steps.1 > 0;
    println!("─────────────────────────────────────────────────────────────────────");
    println!(
        "{:<14} {:>21}   {:>21}   {}",
        "TOTAL",
        totals(scenarios),
        totals(steps),
        if any_failed { "FAILED" } else { "PASSED" },
    );
    println!("═════════════════════════════════════════════════════════════════════");

    any_failed
}

fn counts(stats: &Stats) -> String {
    totals((stats.passed, stats.failed, stats.skipped))
}

fn totals((passed, failed, skipped): (usize, usize, usize)) -> String {
    let mut parts = vec![format!("{passed} passed")];
    if failed > 0 {
        parts.push(format!("{failed} failed"));
    }
    if skipped > 0 {
        parts.push(format!("{skipped} skipped"));
    }
    parts.join(", ")
}

/// Services to start: `--service` if given, otherwise every service named in
/// the `--tags` expression, otherwise all of them.
fn selected_targets(explicit: &[String]) -> Vec<Target> {
    if !explicit.is_empty() {
        return explicit
            .iter()
            .map(|name| Target::parse(name).unwrap_or_else(|| panic!("unknown service '{name}'")))
            .collect();
    }

    // Read the raw expression rather than the parsed AST: any service tag it
    // mentions is a service the run may need.
    let tag_expression = std::env::args()
        .skip_while(|arg| arg != "--tags" && arg != "-t")
        .nth(1)
        .or_else(|| std::env::var("CUCUMBER_FILTER_TAGS").ok())
        .unwrap_or_default();

    let named: Vec<Target> = Target::ALL
        .into_iter()
        .filter(|target| tag_expression.contains(target.tag()))
        .collect();

    if named.is_empty() {
        Target::ALL.to_vec()
    } else {
        named
    }
}

/// Keeps scenarios whose service was not started out of the run, so a targeted
/// invocation does not fail against services that are not up.
fn matches_selection(selected: &[Target], feature: &Feature, scenario: Option<&Scenario>) -> bool {
    let tags = feature
        .tags
        .iter()
        .chain(scenario.into_iter().flat_map(|s| s.tags.iter()));

    let mut service_tags = tags.filter_map(|tag| Target::parse(tag)).peekable();

    // A feature with no service tag is generic and always runs.
    if service_tags.peek().is_none() {
        return true;
    }
    service_tags.any(|target| selected.contains(&target))
}
