use crate::support::job;
use conveyor_core::graph::stage_levels;

fn names(levels: &[Vec<Vec<&conveyor_core::Job>>]) -> Vec<Vec<Vec<String>>> {
    levels
        .iter()
        .map(|level| {
            level
                .iter()
                .map(|stage| stage.iter().map(|j| j.qualified_name()).collect())
                .collect()
        })
        .collect()
}

#[quench_test::test]
fn stages_that_wait_on_each_other_sit_in_later_rows() {
    let jobs = [
        job("check", "lint", &[]),
        job("check", "fmt", &[]),
        job("build", "cargo", &["check"]),
        job("security", "scan", &[]),
        job("publish", "image", &["build", "security"]),
    ];
    let levels = stage_levels(&jobs);
    assert_eq!(
        names(&levels),
        vec![
            vec![
                vec!["check/lint".to_string(), "check/fmt".to_string()],
                vec!["security/scan".to_string()]
            ],
            vec![vec!["build/cargo".to_string()]],
            vec![vec!["publish/image".to_string()]],
        ]
    );
}

#[quench_test::test]
fn a_cycle_or_a_missing_need_cannot_hang_or_hide_a_stage() {
    let jobs = [
        job("a", "x", &["b"]),
        job("b", "y", &["a"]),
        job("c", "z", &["gone"]),
    ];
    let levels = stage_levels(&jobs);
    let stages: usize = levels.iter().map(Vec::len).sum();
    assert_eq!(stages, 3);
    assert!(stage_levels(&[]).is_empty());
}
