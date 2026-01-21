use anvil::commands::docker;

// API services

#[test]
#[ignore]
fn docker_build_audit() {
    let _ = docker::build("audit");
}

#[test]
#[ignore]
fn docker_build_mailbox() {
    let _ = docker::build("mailbox");
}

#[test]
#[ignore]
fn docker_build_access_control() {
    let _ = docker::build("access-control");
}

#[test]
#[ignore]
fn docker_build_knowledge_base() {
    let _ = docker::build("knowledge-base");
}

#[test]
#[ignore]
fn docker_build_worker() {
    let _ = docker::build("worker");
}

#[test]
#[ignore]
fn docker_build_job_manager() {
    let _ = docker::build("job-manager");
}

#[test]
#[ignore]
fn docker_build_quiz_manager() {
    let _ = docker::build("quiz-manager");
}

#[test]
#[ignore]
fn docker_build_gdrive_api() {
    let _ = docker::build("gdrive-api");
}

// Jobs

#[test]
#[ignore]
fn docker_build_postgres_init() {
    let _ = docker::build("postgres-init");
}

#[test]
#[ignore]
fn docker_build_gdrive_sync() {
    let _ = docker::build("gdrive-sync");
}

// Web

#[test]
#[ignore]
fn docker_build_frontend() {
    let _ = docker::build("frontend");
}

// build/release all operations

#[test]
#[ignore]
fn docker_build_all() {
    let _ = docker::build_all();
}

#[test]
#[ignore]
fn docker_release_all() {
    let _ = docker::release_all("ossiriand.arda:30021/ossiriand-1/ossiriand");
}
