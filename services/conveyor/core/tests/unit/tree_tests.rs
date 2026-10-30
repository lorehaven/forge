use crate::support::{project, repo};
use conveyor_core::tree::{Node, ancestor_chain, build, project_path};

fn sample() -> Vec<conveyor_core::Project> {
    vec![
        project("root", "lorehaven", None),
        project("a", "forge", Some("root")),
        project("b", "quench", Some("root")),
        project("c", "empty", Some("root")),
        project("d", "deep", Some("a")),
    ]
}

#[quench_test::test]
fn a_path_runs_from_the_root_down() {
    assert_eq!(project_path("d", &sample()), "lorehaven/forge/deep");
    assert_eq!(project_path("missing", &sample()), "");
}

#[quench_test::test]
fn the_ancestors_are_root_first_and_a_loop_ends() {
    let all = sample();
    let chain = ancestor_chain("d", &all);
    let ids: Vec<&str> = chain.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["root", "a", "d"]);
    assert!(ancestor_chain("nope", &sample()).is_empty());
    let looped = vec![project("x", "x", Some("y")), project("y", "y", Some("x"))];
    assert_eq!(ancestor_chain("x", &looped).len(), 2);
}

#[quench_test::test]
fn leaves_fold_into_one_table_and_containers_come_last() {
    let repos = vec![repo("r-b", "b"), repo("r-d", "d")];
    let tree = build(&sample(), &repos, None);
    // "root" is the only top node and has children, so it is a container; its level: leaf repos (b), the empty leaf, the container a.
    assert_eq!(tree.len(), 1);
    let Node::Container {
        project, children, ..
    } = &tree[0]
    else {
        panic!("{tree:?}")
    };
    assert_eq!(project.id, "root");
    assert!(matches!(&children[0], Node::Repos(r) if r.len() == 1 && r[0].id == "r-b"));
    assert!(matches!(&children[1], Node::Empty(p) if p.id == "c"));
    let Node::Container {
        project: a,
        children: inner,
        ..
    } = &children[2]
    else {
        panic!("{children:?}")
    };
    assert_eq!(a.id, "a");
    assert!(matches!(&inner[0], Node::Repos(r) if r[0].id == "r-d"));
}

#[quench_test::test]
fn a_scoped_tree_starts_with_the_nodes_own_repositories() {
    let repos = vec![repo("own", "a"), repo("r-d", "d")];
    let tree = build(&sample(), &repos, Some("a"));
    assert!(matches!(&tree[0], Node::Repos(r) if r[0].id == "own"));
    assert!(matches!(&tree[1], Node::Repos(r) if r[0].id == "r-d"));
    assert_eq!(tree.len(), 2);
    assert!(build(&sample(), &[], Some("c")).is_empty());
}
