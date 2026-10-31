//! Who may run a privileged service.
//!
//! A pipeline asks for it (`privileged = true` on a `[[stage.job.service]]`), which is the whole of what a
//! repository can do by itself: the file is the repository's, so what it asks for is not to be trusted. The
//! deployment decides, twice: the executor must allow privileged containers at all
//! (`CONVEYOR_K8S_ALLOW_PRIVILEGED`), and the scheduler - here - says for which repositories and which runs.
//!
//! * A **pull request never** gets one: its `.conveyor.toml` is the contributor's, and a privileged container is
//!   the node. Nothing in the configuration lifts this.
//! * Any other run gets one only if its repository is in `CONVEYOR_PRIVILEGED_REPOS` (`owner/name`, comma or space
//!   separated, or `*` for every repository). The default is nobody.

use crate::domain::Trigger;
use conveyor_pipeline::Service;

/// Why a job may not run, or `Ok` when none of its services is privileged or all may be.
///
/// # Errors
///
/// The reason, in words a person reads, naming the services.
pub fn check(
    services: &[Service],
    trigger: Trigger,
    repo: &str,
    allowed_repos: &[String],
) -> Result<(), String> {
    let asked: Vec<&str> = services
        .iter()
        .filter(|service| service.privileged)
        .map(|service| service.name.as_str())
        .collect();
    if asked.is_empty() {
        return Ok(());
    }
    let names = asked.join(", ");
    if trigger == Trigger::PullRequest {
        return Err(format!(
            "a pull request cannot run the privileged service(s) {names}: its pipeline is the contributor's, and a privileged container can do what the node can"
        ));
    }
    if allowed_repos
        .iter()
        .any(|allowed| allowed == "*" || allowed.eq_ignore_ascii_case(repo))
    {
        return Ok(());
    }
    Err(format!(
        "{repo} may not run the privileged service(s) {names}: add it to CONVEYOR_PRIVILEGED_REPOS"
    ))
}
