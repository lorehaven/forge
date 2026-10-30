use gantry_core::filter::{chips, matching};
use gantry_core::{Group, Summary};

fn group(package: &str, sync: &str) -> Group {
    Group {
        package: package.into(),
        description: None,
        installed: None,
        offered: None,
        status: String::new(),
        inventoried: true,
        rows: vec![],
        summary: Summary {
            sync: sync.into(),
            ..Summary::default()
        },
        can: vec![],
    }
}

fn names(groups: &[Group]) -> Vec<&str> {
    groups.iter().map(|g| g.package.as_str()).collect()
}

#[quench_test::test]
fn what_needs_attention_comes_first_then_by_name() {
    let all = [
        group("media", "synced"),
        group("forge", "missing"),
        group("ml", "out_of_sync"),
        group("alpha", "synced"),
    ];
    assert_eq!(
        names(&matching(&all, "", "")),
        ["forge", "ml", "alpha", "media"]
    );
}

#[quench_test::test]
fn the_filter_narrows_by_state_and_by_name() {
    let all = [group("media", "synced"), group("forge", "not_installed")];
    assert_eq!(names(&matching(&all, "not_installed", "")), ["forge"]);
    assert_eq!(names(&matching(&all, "", "MED")), ["media"]);
    assert!(matching(&all, "", "nothing").is_empty());
}

#[quench_test::test]
fn a_chip_is_offered_for_each_state_that_has_packages_and_for_the_chosen_one() {
    let all = [
        group("a", "synced"),
        group("b", "synced"),
        group("c", "missing"),
    ];
    assert_eq!(chips(&all, ""), [("synced", 2), ("missing", 1)]);
    assert_eq!(
        chips(&all, "unlisted"),
        [("synced", 2), ("missing", 1), ("unlisted", 0)]
    );
}
