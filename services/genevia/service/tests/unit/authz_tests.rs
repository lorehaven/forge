//! Who may open and change which tree - see `services/genevia/service/src/routers/api/authz.rs`.

use genevia_service::routers::api::authz::Access;
use quench_auth::domain::jwt::Claims;

fn access(scope: &str) -> Access {
    Access::new(
        Some(Claims::for_audiences(
            "alice".to_string(),
            vec!["genevia".to_string()],
            scope.to_string(),
            None,
            3600,
        )),
        true,
    )
}

#[quench_test::test]
fn a_blanket_read_opens_every_tree_and_changes_none() {
    let a = access("genevia:read");
    assert!(a.can_read("walus") && a.can_read("other"));
    assert!(a.can_read_all());
    assert!(!a.can_write("walus") && !a.can_write_all());
}

#[quench_test::test]
fn a_blanket_write_reads_and_changes_every_tree_and_may_add_trees() {
    let a = access("genevia:write");
    assert!(a.can_read("walus") && a.can_write("other"));
    assert!(a.can_write_all() && a.can_read_all());
}

#[quench_test::test]
fn a_grant_on_one_tree_covers_only_that_tree() {
    let a = access("genevia:tree:walus:write genevia:tree:kowal:read");
    assert!(
        a.can_read("walus") && a.can_write("walus"),
        "write includes read"
    );
    assert!(a.can_read("kowal") && !a.can_write("kowal"));
    assert!(!a.can_read("nowak"));
    assert!(
        !a.can_read_all() && !a.can_write_all(),
        "a branch is not the whole"
    );
    let mut named = a.granted_trees();
    named.sort();
    assert_eq!(named, vec!["kowal".to_string(), "walus".to_string()]);
}

#[quench_test::test]
fn another_service_grants_nothing_here() {
    let a = access("workbench:write songbook:read");
    assert!(!a.can_read("walus") && !a.can_read_all());
    assert_eq!(a.granted_trees(), [] as [std::string::String; 0]);
}

#[quench_test::test]
fn an_admin_may_do_everything() {
    let a = access("admin");
    assert!(a.can_read("x") && a.can_write("x") && a.can_write_all() && a.can_read_all());
}

#[quench_test::test]
fn nobody_signed_in_may_do_nothing_unless_the_realm_is_open() {
    let nobody = Access::new(None, true);
    assert!(!nobody.can_read("x") && !nobody.can_read_all() && !nobody.can_write_all());
    assert_eq!(nobody.granted_trees(), [] as [std::string::String; 0]);
    let open = Access::new(None, false);
    assert!(open.can_read("x") && open.can_write("x") && open.can_write_all());
    assert_eq!(open.who(), "dev");
}
