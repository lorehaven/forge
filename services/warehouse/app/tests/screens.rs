//! The screens with no host: mounted into a test tree, driven by events, read back as text.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;

use quench_ui::Event;
use quench_ui::prelude::*;
use quench_ui::router::{History, MemoryHistory};
use quench_ui::test_renderer::NodeId;
use warehouse_app::{Msg, Request, Setup, Warehouse, warehouse};
use warehouse_core::{
    ArtifactEntry, Browse, BrowseFile, CrateDep, CrateVersion, DockerTag, DynamicStorage, Features,
    MetaPair, NewStorage, Overview, RivetEntry, StaticStorage, Storages,
};

type Asked = Rc<RefCell<Vec<Request>>>;

fn find(r: &TestRenderer, tag: &str, attr: (&str, &str)) -> NodeId {
    r.document_order()
        .into_iter()
        .find(|n| {
            r.tag(*n).as_deref() == Some(tag) && r.attr(*n, attr.0).as_deref() == Some(attr.1)
        })
        .unwrap_or_else(|| panic!("no <{tag} {}={}>: {}", attr.0, attr.1, r.html()))
}

fn present(r: &TestRenderer, id: &str) -> bool {
    r.document_order()
        .into_iter()
        .any(|n| r.attr(n, "id").as_deref() == Some(id))
}

struct Live {
    r: Rc<TestRenderer>,
    asked: Asked,
    deliver: Rc<dyn Fn(Msg)>,
    history: Rc<MemoryHistory>,
    _mount: quench_ui::Mount<TestRenderer>,
}

impl Live {
    fn click(&self, tag: &str, id: &str) {
        self.r.fire(find(&self.r, tag, ("id", id)), &Event::Click);
    }

    fn type_into(&self, tag: &str, id: &str, text: &str) {
        self.r
            .fire(find(&self.r, tag, ("id", id)), &Event::Input(text.into()));
    }

    fn submit(&self, id: &str) {
        self.r
            .fire(find(&self.r, "form", ("id", id)), &Event::Submit);
    }

    /// Follows the link `<a attr=value>`.
    fn follow(&self, attr: (&str, &str)) {
        self.r.fire(find(&self.r, "a", attr), &Event::Click);
    }

    fn asked(&self) -> Vec<Request> {
        self.asked.borrow().clone()
    }

    fn last(&self) -> Request {
        self.asked().last().unwrap().clone()
    }

    fn send(&self, m: Msg) {
        (self.deliver)(m);
    }

    fn text(&self) -> String {
        self.r.text()
    }

    fn url(&self) -> String {
        self.history.location().url()
    }
}

fn live(url: &str) -> Live {
    let asked: Asked = Rc::default();
    let log = asked.clone();
    let history = Rc::new(MemoryHistory::new(url));
    let Warehouse { view, deliver } = warehouse(
        Rc::new(move |q| log.borrow_mut().push(q)),
        Setup {
            base: "/warehouse".into(),
            language: "en-US".into(),
            login_url: "/warehouse/ui/login".into(),
            history: history.clone(),
        },
    );
    let r = Rc::new(TestRenderer::new());
    let mount = mount(&r, &r.root(), view);
    Live {
        r,
        asked,
        deliver,
        history,
        _mount: mount,
    }
}

fn overview(can_manage: bool) -> Overview {
    Overview {
        username: "dev".into(),
        can_manage,
        features: Features {
            docker: true,
            crates: true,
            files: true,
            artifacts: true,
            rivets: true,
        },
    }
}

/// A started app: the overview came.
fn started(url: &str, can_manage: bool) -> Live {
    let l = live(url);
    l.send(Msg::Overview(overview(can_manage)));
    l
}

// ── Shell and home ───────────────────────────────────────────────────────────

#[quench_test::test]
fn the_app_starts_by_asking_who_and_what_is_on() {
    let l = live("/warehouse/ui/home");
    assert_eq!(l.asked(), [Request::Startup]);
    assert!(present(&l.r, "waiting"));
}

#[quench_test::test]
fn home_has_a_card_per_registry_that_is_on() {
    let l = started("/warehouse/ui/home", true);
    for (id, href) in [
        ("docker", "/warehouse/ui/docker/catalog"),
        ("crates", "/warehouse/ui/crates/catalog"),
        ("files", "/warehouse/ui/files/storages"),
        ("artifacts", "/warehouse/ui/artifacts/catalog"),
        ("rivets", "/warehouse/ui/rivets/catalog"),
    ] {
        let card = find(&l.r, "a", ("data-service", id));
        assert_eq!(l.r.attr(card, "href").as_deref(), Some(href));
    }
    assert!(l.text().contains("Docker Registry"), "{}", l.text());

    let mut off = overview(false);
    off.features = Features {
        rivets: true,
        ..Features::default()
    };
    l.send(Msg::Overview(off));
    assert!(l.r.find_by_attr("data-service", "docker").is_empty());
    assert!(!l.r.find_by_attr("data-service", "rivets").is_empty());

    l.send(Msg::Overview(Overview::default()));
    assert!(present(&l.r, "no-services"));
}

#[quench_test::test]
fn the_sidebar_links_the_registries_and_marks_the_open_one() {
    let l = started("/warehouse/ui/crates/catalog", false);
    let nav = |id: &str| find(&l.r, "a", ("data-nav", id));
    assert_eq!(
        l.r.attr(nav("docker"), "href").as_deref(),
        Some("/warehouse/ui/docker/catalog")
    );
    assert!(
        l.r.attr(nav("crates"), "class")
            .unwrap()
            .contains("wh-current")
    );
    assert!(
        !l.r.attr(nav("docker"), "class")
            .unwrap()
            .contains("wh-current")
    );
    l.follow(("data-nav", "files"));
    assert_eq!(l.url(), "/warehouse/ui/files/storages");
    assert!(
        l.r.attr(nav("files"), "class")
            .unwrap()
            .contains("wh-current")
    );
    l.follow(("data-nav", "home"));
    assert!(l.text().contains("Registry Services"), "{}", l.text());
}

#[quench_test::test]
fn an_unknown_address_is_not_found() {
    let l = live("/warehouse/ui/nowhere");
    assert!(present(&l.r, "not-found"));
}

#[quench_test::test]
fn a_lost_session_offers_the_way_back_in_and_a_refusal_is_said_in_words() {
    let l = started("/warehouse/ui/home", false);
    l.send(Msg::SignedOut);
    let link = find(&l.r, "a", ("id", "sign-in"));
    assert_eq!(
        l.r.attr(link, "href").as_deref(),
        Some("/warehouse/ui/login")
    );
    l.send(Msg::Failed("cannot reach the service: down".into()));
    assert!(l.text().contains("cannot reach the service: down"));

    let l = started("/warehouse/ui/home", false);
    l.send(Msg::Rejected("api_error_forbidden".into()));
    assert!(
        l.text().contains("You do not have permission to do that."),
        "{}",
        l.text()
    );
    l.send(Msg::Rejected("the service answered 502".into()));
    assert!(l.text().contains("the service answered 502"));
}

#[quench_test::test]
fn changing_the_language_redraws_the_screen_without_asking_again() {
    let l = started("/warehouse/ui/home", false);
    let asked = l.asked().len();
    assert!(l.text().contains("Registry Services"));
    let picker = find(&l.r, "select", ("id", "language"));
    l.r.fire(picker, &Event::Input("de-DE".into()));
    assert!(!l.text().contains("Registry Services"), "{}", l.text());
    assert!(l.text().contains("Docker"), "{}", l.text());
    assert_eq!(l.last(), Request::Language("de-DE".into()));
    assert_eq!(l.asked().len(), asked + 1, "only the language was reported");
    // The screen was drawn again in the other language, with its data (the overview) still there.
    assert!(!present(&l.r, "waiting"));
}

// ── Docker ───────────────────────────────────────────────────────────────────

fn tag(name: &str, digest: &str) -> DockerTag {
    DockerTag {
        tag: name.into(),
        digest: digest.into(),
        media_type: Some("application/vnd.oci.image.manifest.v1+json".into()),
        size_bytes: Some(1234),
    }
}

fn repos() -> Vec<String> {
    ["app", "team/api", "team/web"]
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[quench_test::test]
fn docker_shows_the_repositories_as_a_tree() {
    let l = started("/warehouse/ui/docker/catalog", false);
    assert!(l.asked().contains(&Request::DockerRepos));
    l.send(Msg::DockerRepos(repos()));
    let app = find(&l.r, "a", ("data-repo", "app"));
    assert_eq!(
        l.r.attr(app, "href").as_deref(),
        Some("/warehouse/ui/docker/catalog?repo=app")
    );
    assert!(l.text().contains("team"), "{}", l.text());
    assert!(l.text().contains("Select a tag to inspect metadata."));
    l.send(Msg::DockerRepos(vec![]));
    assert!(present(&l.r, "no-repos"));
}

#[quench_test::test]
fn choosing_a_repository_asks_for_its_tags_and_a_tag_shows_its_metadata() {
    let l = started("/warehouse/ui/docker/catalog", false);
    l.send(Msg::DockerRepos(repos()));
    l.follow(("data-repo", "team/api"));
    assert_eq!(l.last(), Request::DockerTags("team/api".into()));
    l.send(Msg::DockerTags {
        repo: "team/api".into(),
        tags: vec![tag("2.0.0", "sha256:bb"), tag("1.0.0", "sha256:aa")],
    });
    assert!(l.r.find_by_attr("data-tag", "2.0.0").len() == 1);
    l.follow(("data-tag", "1.0.0"));
    assert_eq!(
        l.url(),
        "/warehouse/ui/docker/catalog?repo=team%2Fapi&tag=1.0.0"
    );
    let text = l.text();
    assert!(text.contains("Metadata for 1.0.0"), "{text}");
    assert!(
        text.contains("sha256:aa") && text.contains("1234 bytes"),
        "{text}"
    );
    assert!(text.contains("application/vnd.oci.image.manifest.v1+json"));
}

#[quench_test::test]
fn a_tag_with_no_manifest_says_unknown() {
    let l = started("/warehouse/ui/docker/catalog?repo=app&tag=t", false);
    l.send(Msg::DockerRepos(repos()));
    l.send(Msg::DockerTags {
        repo: "app".into(),
        tags: vec![DockerTag {
            tag: "t".into(),
            digest: String::new(),
            media_type: None,
            size_bytes: None,
        }],
    });
    assert!(l.text().matches("unknown").count() >= 2, "{}", l.text());
}

#[quench_test::test]
fn the_tags_of_another_repository_are_not_shown() {
    let l = started("/warehouse/ui/docker/catalog?repo=app&tag=1", false);
    l.send(Msg::DockerRepos(repos()));
    l.send(Msg::DockerTags {
        repo: "team/api".into(),
        tags: vec![tag("1", "sha256:aa")],
    });
    assert!(l.r.find_by_attr("data-tag", "1").is_empty());
}

#[quench_test::test]
fn deleting_an_image_is_confirmed_first_and_then_asked_for() {
    let l = started("/warehouse/ui/docker/catalog?repo=app&tag=1", false);
    l.send(Msg::DockerRepos(repos()));
    l.send(Msg::DockerTags {
        repo: "app".into(),
        tags: vec![tag("1", "sha256:aa")],
    });
    let before = l.asked().len();
    l.click("button", "delete-image");
    assert!(present(&l.r, "confirm"));
    assert!(l.text().contains("app:1"), "{}", l.text());
    assert_eq!(l.asked().len(), before, "nothing yet");

    l.click("button", "confirm-cancel");
    assert!(!present(&l.r, "confirm"));
    assert_eq!(l.asked().len(), before);

    l.click("button", "delete-image");
    l.click("button", "confirm-delete");
    assert!(!present(&l.r, "confirm"));
    assert_eq!(
        l.last(),
        Request::DeleteImage {
            repository: "app".into(),
            digest: "sha256:aa".into()
        }
    );

    l.send(Msg::ImageDeleted("app".into()));
    assert_eq!(l.url(), "/warehouse/ui/docker/catalog?repo=app");
}

// ── Crates ───────────────────────────────────────────────────────────────────

fn version(vers: &str, yanked: bool) -> CrateVersion {
    CrateVersion {
        vers: vers.into(),
        yanked,
        cksum: format!("sum-{vers}"),
        rust_version: Some("1.80".into()),
        links: None,
        features: vec!["a".into(), "b".into()],
        deps: vec![
            CrateDep {
                name: "serde".into(),
                req: "^1".into(),
                optional: true,
                kind: "normal".into(),
                target: None,
                package: None,
            },
            CrateDep {
                name: "tempfile".into(),
                req: "^3".into(),
                optional: false,
                kind: "dev".into(),
                target: None,
                package: None,
            },
        ],
    }
}

fn crate_names() -> Vec<String> {
    vec!["alpha".into(), "zeta".into()]
}

#[quench_test::test]
fn crates_lists_names_and_the_open_crates_versions_newest_first() {
    let l = started("/warehouse/ui/crates/catalog", false);
    assert!(l.asked().contains(&Request::Crates));
    l.send(Msg::Crates(crate_names()));
    l.follow(("data-crate", "alpha"));
    assert_eq!(l.last(), Request::CrateVersions("alpha".into()));
    l.send(Msg::CrateVersions {
        name: "alpha".into(),
        versions: vec![
            version("1.0.0", false),
            version("1.1.0", false),
            version("1.2.0", true),
        ],
    });
    let order: Vec<String> =
        l.r.document_order()
            .into_iter()
            .filter_map(|n| l.r.attr(n, "data-version"))
            .collect();
    assert_eq!(order, ["1.2.0", "1.1.0", "1.0.0"]);
    // Nothing chosen: the newest version that is not yanked is open.
    let text = l.text();
    assert!(text.contains("Metadata for 1.1.0"), "{text}");
    assert!(
        text.contains("sum-1.1.0") && text.contains("1.80"),
        "{text}"
    );
    assert!(text.contains("a, b"), "{text}");
}

#[quench_test::test]
fn dependencies_are_grouped() {
    let l = started("/warehouse/ui/crates/catalog?repo=alpha&tag=1.0.0", false);
    l.send(Msg::Crates(crate_names()));
    l.send(Msg::CrateVersions {
        name: "alpha".into(),
        versions: vec![version("1.0.0", false)],
    });
    let text = l.text();
    assert!(text.contains("serde ^1 [optional]"), "{text}");
    assert!(text.contains("tempfile ^3"), "{text}");
    assert!(
        text.contains("dev-dependencies") && text.contains("dependencies"),
        "{text}"
    );
}

#[quench_test::test]
fn a_crate_version_is_yanked_and_then_unyanked_by_one_click() {
    let l = started("/warehouse/ui/crates/catalog?repo=alpha&tag=1.0.0", false);
    l.send(Msg::Crates(crate_names()));
    l.send(Msg::CrateVersions {
        name: "alpha".into(),
        versions: vec![version("1.0.0", false), version("1.1.0", true)],
    });
    assert!(!present(&l.r, "unyank"));
    l.click("button", "yank");
    assert_eq!(
        l.last(),
        Request::SetCrateYank {
            name: "alpha".into(),
            version: "1.0.0".into(),
            yanked: true
        }
    );
    l.follow(("data-version", "1.1.0"));
    assert!(l.text().contains("Unyank Version"));
    l.click("button", "unyank");
    assert_eq!(
        l.last(),
        Request::SetCrateYank {
            name: "alpha".into(),
            version: "1.1.0".into(),
            yanked: false
        }
    );
}

#[quench_test::test]
fn a_crate_with_no_versions_says_so() {
    let l = started("/warehouse/ui/crates/catalog?repo=alpha", false);
    l.send(Msg::Crates(crate_names()));
    l.send(Msg::CrateVersions {
        name: "alpha".into(),
        versions: vec![],
    });
    assert!(l.text().contains("No versions found"), "{}", l.text());
    assert!(l.text().contains("Select a version"), "{}", l.text());
    l.send(Msg::Crates(vec![]));
    assert!(present(&l.r, "no-crates"));
}

// ── Artifacts ────────────────────────────────────────────────────────────────

fn artifact(program: &str, platform: &str, code: i64, yanked: bool) -> ArtifactEntry {
    ArtifactEntry {
        program: program.into(),
        platform: platform.into(),
        format: "apk".into(),
        version_code: code,
        version_name: format!("{code}.0"),
        filename: format!("{program}-{code}.apk"),
        size_bytes: 4096,
        sha256: "cafe".into(),
        uploaded_by: "dev".into(),
        yanked,
        arch: None,
        label: Some("Nice App".into()),
        min_sdk_version: Some(26),
        target_sdk_version: Some(35),
        permissions: vec!["INTERNET".into(), "CAMERA".into()],
    }
}

fn artifacts() -> Vec<ArtifactEntry> {
    vec![
        artifact("app", "android", 1, false),
        artifact("app", "android", 2, false),
        artifact("app", "linux", 5, true),
        artifact("tool", "windows", 3, false),
    ]
}

#[quench_test::test]
fn artifacts_open_a_program_on_its_first_platform_and_newest_build() {
    let l = started("/warehouse/ui/artifacts/catalog", true);
    assert!(l.asked().contains(&Request::Artifacts));
    l.send(Msg::Artifacts(artifacts()));
    assert!(l.r.find_by_attr("data-program", "tool").len() == 1);
    assert!(l.r.find_by_attr("data-platform", "android").is_empty());
    l.follow(("data-program", "app"));
    assert_eq!(l.url(), "/warehouse/ui/artifacts/catalog?repo=app");
    let text = l.text();
    assert!(text.contains("Metadata for 2.0 (2)"), "{text}");
    assert!(
        text.contains("Nice App")
            && text.contains("4096 bytes")
            && text.contains("INTERNET, CAMERA"),
        "{text}"
    );
    assert!(text.contains("26") && text.contains("35"), "{text}");
    l.follow(("data-platform", "linux"));
    assert!(l.text().contains("Metadata for 5.0 (5)"), "{}", l.text());
    assert!(l.text().contains("Unyank"), "{}", l.text());
}

#[quench_test::test]
fn an_artifact_build_is_yanked_by_someone_who_may_and_not_by_someone_who_may_not() {
    let l = started(
        "/warehouse/ui/artifacts/catalog?repo=app&platform=android&tag=1",
        true,
    );
    l.send(Msg::Artifacts(artifacts()));
    l.click("button", "yank");
    assert_eq!(
        l.last(),
        Request::SetArtifactYank {
            program: "app".into(),
            platform: "android".into(),
            version_code: 1,
            yanked: true
        }
    );

    let l = started(
        "/warehouse/ui/artifacts/catalog?repo=app&platform=android&tag=1",
        false,
    );
    l.send(Msg::Artifacts(artifacts()));
    assert!(l.text().contains("Metadata for 1.0 (1)"));
    assert!(!present(&l.r, "yank") && !present(&l.r, "unyank"));
}

#[quench_test::test]
fn no_artifacts_says_so() {
    let l = started("/warehouse/ui/artifacts/catalog", true);
    l.send(Msg::Artifacts(vec![]));
    assert!(present(&l.r, "no-artifacts"));
}

// ── Rivets ───────────────────────────────────────────────────────────────────

fn rivet(version: &str, yanked: bool) -> RivetEntry {
    RivetEntry {
        name: "base".into(),
        version: version.into(),
        size_bytes: 99,
        sha256: "abc".into(),
        uploaded_by: "dev".into(),
        yanked,
        published: "2026-10-29 10:00 UTC".into(),
        description: Some("The base overlay".into()),
        namespace: Some("infra".into()),
        requires_riveter: Some(">=1.0".into()),
        requires_packages: vec!["core".into(), "net".into()],
        meta: vec![MetaPair {
            key: "team".into(),
            value: "platform".into(),
        }],
    }
}

fn rivets() -> Vec<RivetEntry> {
    vec![rivet("2.0.0", true), rivet("1.0.0", false)]
}

#[quench_test::test]
fn rivets_open_what_latest_would_give() {
    let l = started("/warehouse/ui/rivets/catalog?repo=base", true);
    assert!(l.asked().contains(&Request::Rivets));
    l.send(Msg::Rivets(rivets()));
    let text = l.text();
    assert!(text.contains("Metadata for base 1.0.0"), "{text}");
    assert!(
        text.contains("The base overlay")
            && text.contains("core, net")
            && text.contains("platform"),
        "{text}"
    );
    assert!(text.contains("riveter install base@1.0.0"), "{text}");
    assert!(text.contains("2026-10-29 10:00 UTC"), "{text}");
    l.follow(("data-version", "2.0.0"));
    assert!(l.text().contains("Metadata for base 2.0.0"));
    l.click("button", "unyank");
    assert_eq!(
        l.last(),
        Request::SetRivetYank {
            name: "base".into(),
            version: "2.0.0".into(),
            yanked: false
        }
    );
}

#[quench_test::test]
fn a_reader_cannot_yank_a_rivet_and_an_empty_registry_says_so() {
    let l = started("/warehouse/ui/rivets/catalog?repo=base", false);
    l.send(Msg::Rivets(rivets()));
    assert!(!present(&l.r, "yank") && !present(&l.r, "unyank"));
    l.send(Msg::Rivets(vec![]));
    assert!(present(&l.r, "no-rivets"));
}

// ── Files: storages ──────────────────────────────────────────────────────────

fn storage(name: &str) -> DynamicStorage {
    DynamicStorage {
        name: name.into(),
        owner: "dev".into(),
        max_file_bytes: Some(5 * 1024 * 1024),
        quota_bytes: 2 * 1024 * 1024 * 1024,
        used_bytes: 1024 * 1024 * 1024,
        sync_enabled: true,
        created: "2026-10-29 10:00 UTC".into(),
    }
}

fn storages_list() -> Storages {
    Storages {
        fixed: vec![StaticStorage {
            name: "shared".into(),
            root: "/srv/shared".into(),
        }],
        dynamic: vec![storage("backups")],
    }
}

#[quench_test::test]
fn storages_lists_both_kinds_and_asks_to_pick_one() {
    let l = started("/warehouse/ui/files/storages", false);
    assert!(l.asked().contains(&Request::Storages));
    l.send(Msg::Storages(storages_list()));
    assert!(l.text().contains("shared") && l.text().contains("backups"));
    assert!(l.text().contains("Select a storage from the list."));
    assert!(!present(&l.r, "new-storage"), "a reader has no form");
    l.send(Msg::Storages(Storages::default()));
    assert!(present(&l.r, "no-storages"));
}

#[quench_test::test]
fn a_static_storage_shows_its_root() {
    let l = started("/warehouse/ui/files/storages?storage=shared", true);
    l.send(Msg::Storages(storages_list()));
    assert!(l.text().contains("/srv/shared"), "{}", l.text());
    assert!(!present(&l.r, "edit-storage") && !present(&l.r, "delete-storage"));
    assert!(present(&l.r, "browse-files"));
    let browse = find(&l.r, "a", ("id", "browse-files"));
    assert_eq!(
        l.r.attr(browse, "href").as_deref(),
        Some("/warehouse/ui/files/browse?storage=shared")
    );
}

#[quench_test::test]
fn a_database_storage_shows_its_use_and_a_manager_can_edit_it() {
    let l = started("/warehouse/ui/files/storages?storage=backups", true);
    l.send(Msg::Storages(storages_list()));
    let text = l.text();
    assert!(text.contains("1.00 GiB / 2.00 GiB (50.0%)"), "{text}");
    assert!(
        text.contains("5.00 MiB") && text.contains("enabled"),
        "{text}"
    );
    assert!(present(&l.r, "edit-storage"));

    l.type_into("input", "edit-quota", "3");
    l.click("input", "edit-sync");
    l.submit("edit-storage");
    let Request::EditStorage { name, changes } = l.last() else {
        panic!("{:?}", l.last());
    };
    assert_eq!(name, "backups");
    assert_eq!(changes.quota_bytes, Some(3 * 1024 * 1024 * 1024));
    assert_eq!(changes.max_file_bytes, Some(5 * 1024 * 1024));
    assert!(!changes.sync_enabled, "the checkbox was unticked");

    l.click("input", "edit-clear");
    l.submit("edit-storage");
    let Request::EditStorage { changes, .. } = l.last() else {
        panic!()
    };
    assert!(changes.clear_max_file);

    let asked = l.asked().len();
    l.type_into("input", "edit-quota", "lots");
    l.submit("edit-storage");
    assert_eq!(l.asked().len(), asked, "a bad quota is not sent");
    assert!(
        l.text().contains("Quota must be a non-negative number."),
        "{}",
        l.text()
    );
}

#[quench_test::test]
fn a_reader_sees_a_storage_but_cannot_change_it() {
    let l = started("/warehouse/ui/files/storages?storage=backups", false);
    l.send(Msg::Storages(storages_list()));
    assert!(l.text().contains("backups"));
    for id in ["edit-storage", "new-storage", "delete-storage"] {
        assert!(!present(&l.r, id), "{id}");
    }
}

#[quench_test::test]
fn a_storage_is_created_and_then_opened() {
    let l = started("/warehouse/ui/files/storages", true);
    l.send(Msg::Storages(storages_list()));
    l.submit("new-storage");
    assert!(
        l.text().contains("Storage names may use letters, digits"),
        "{}",
        l.text()
    );
    l.type_into("input", "new-name", "photos");
    l.submit("new-storage");
    assert!(l.text().contains("An owner is required."));
    l.type_into("input", "new-owner", "dev");
    l.type_into("input", "new-quota", "0.5");
    l.click("input", "new-sync");
    l.submit("new-storage");
    assert_eq!(
        l.last(),
        Request::CreateStorage(NewStorage {
            name: "photos".into(),
            owner: "dev".into(),
            quota_bytes: Some(536_870_912),
            max_file_bytes: None,
            sync_enabled: true,
        })
    );
    l.send(Msg::StorageSaved("photos".into()));
    assert_eq!(l.url(), "/warehouse/ui/files/storages?storage=photos");
}

#[quench_test::test]
fn removing_a_storage_is_confirmed() {
    let l = started("/warehouse/ui/files/storages?storage=backups", true);
    l.send(Msg::Storages(storages_list()));
    l.click("button", "delete-storage");
    assert!(
        l.text()
            .contains("Delete this storage and everything in it?")
    );
    l.click("button", "confirm-delete");
    assert_eq!(l.last(), Request::DeleteStorage("backups".into()));
    l.send(Msg::StorageDeleted);
    assert_eq!(l.url(), "/warehouse/ui/files/storages");
}

#[quench_test::test]
fn an_unknown_storage_says_so_and_a_manager_can_still_create_one() {
    let l = started("/warehouse/ui/files/storages?storage=ghost", true);
    l.send(Msg::Storages(storages_list()));
    assert!(present(&l.r, "storage-missing"));
    assert!(present(&l.r, "new-storage"));
}

// ── Files: the browser ───────────────────────────────────────────────────────

fn file(path: &str, size: i64) -> BrowseFile {
    BrowseFile {
        path: path.into(),
        size: Some(size),
    }
}

fn browse_of(access: &str) -> Browse {
    Browse {
        storage: "backups".into(),
        access: access.into(),
        files: vec![
            file("docs/readme.md", 120),
            file("docs/img/logo.png", 2048),
            file("docs/data.bin", 3),
            file("top.txt", 5),
        ],
        truncated: false,
    }
}

#[quench_test::test]
fn the_browser_needs_a_storage_and_asks_for_its_files() {
    let l = started("/warehouse/ui/files/browse", true);
    assert_eq!(l.url(), "/warehouse/ui/files/storages");

    let l = started("/warehouse/ui/files/browse?storage=backups", true);
    assert!(l.asked().contains(&Request::Browse("backups".into())));
    assert!(present(&l.r, "waiting"));
}

#[quench_test::test]
fn the_root_lists_directories_first_and_a_directory_lists_its_entries() {
    let l = started("/warehouse/ui/files/browse?storage=backups", true);
    l.send(Msg::Browse(Box::new(browse_of("readable"))));
    let entries: Vec<String> =
        l.r.document_order()
            .into_iter()
            .filter_map(|n| l.r.attr(n, "data-entry"))
            .collect();
    assert_eq!(entries, ["docs", "top.txt"]);
    assert!(!present(&l.r, "up"));

    l.follow(("data-entry", "docs"));
    assert_eq!(
        l.url(),
        "/warehouse/ui/files/browse?storage=backups&path=docs"
    );
    let entries: Vec<String> =
        l.r.document_order()
            .into_iter()
            .filter_map(|n| l.r.attr(n, "data-entry"))
            .collect();
    assert_eq!(entries, ["docs/img", "docs/data.bin", "docs/readme.md"]);
    assert!(l.text().contains("120 B"), "{}", l.text());
    assert!(present(&l.r, "up"));
    l.follow(("data-crumb", ""));
    assert_eq!(l.url(), "/warehouse/ui/files/browse?storage=backups");
}

#[quench_test::test]
fn a_file_shows_its_details_a_preview_and_a_download() {
    let l = started(
        "/warehouse/ui/files/browse?storage=backups&path=docs%2Fimg%2Flogo.png",
        false,
    );
    l.send(Msg::Browse(Box::new(browse_of("readable"))));
    let text = l.text();
    assert!(text.contains("Metadata for logo.png"), "{text}");
    assert!(
        text.contains("image/png") && text.contains("2.00 KiB"),
        "{text}"
    );
    let img = find(&l.r, "img", ("alt", "logo.png"));
    assert_eq!(
        l.r.attr(img, "src").as_deref(),
        Some("/warehouse/api/v1/files/backups/file?path=docs%2Fimg%2Flogo.png&disposition=inline")
    );
    let download = find(&l.r, "a", ("id", "download"));
    assert_eq!(
        l.r.attr(download, "href").as_deref(),
        Some("/warehouse/api/v1/files/backups/file?path=docs%2Fimg%2Flogo.png")
    );
    assert_eq!(l.r.attr(download, "download").as_deref(), Some("logo.png"));
    assert!(!present(&l.r, "delete-file"), "a reader cannot delete");
}

#[quench_test::test]
fn text_is_framed() {
    let l = started(
        "/warehouse/ui/files/browse?storage=backups&path=docs%2Freadme.md",
        false,
    );
    l.send(Msg::Browse(Box::new(browse_of("readable"))));
    assert_eq!(l.r.find_by_tag("iframe").len(), 1);
}

#[quench_test::test]
fn a_file_of_unknown_kind_has_no_preview() {
    let l = started(
        "/warehouse/ui/files/browse?storage=backups&path=docs%2Fdata.bin",
        false,
    );
    l.send(Msg::Browse(Box::new(browse_of("readable"))));
    assert!(present(&l.r, "no-preview"));
}

#[quench_test::test]
fn deleting_a_file_is_confirmed_and_returns_to_its_directory() {
    let l = started(
        "/warehouse/ui/files/browse?storage=backups&path=docs%2Freadme.md",
        true,
    );
    l.send(Msg::Browse(Box::new(browse_of("readable"))));
    l.click("button", "delete-file");
    assert!(l.text().contains("backups/docs/readme.md"), "{}", l.text());
    l.click("button", "confirm-delete");
    assert_eq!(
        l.last(),
        Request::DeleteFile {
            storage: "backups".into(),
            path: "docs/readme.md".into()
        }
    );
    l.send(Msg::FileDeleted {
        storage: "backups".into(),
        parent: "docs".into(),
    });
    assert_eq!(
        l.url(),
        "/warehouse/ui/files/browse?storage=backups&path=docs"
    );
}

#[quench_test::test]
fn a_path_that_is_not_there_a_storage_that_is_not_there_and_one_that_is_denied() {
    let l = started(
        "/warehouse/ui/files/browse?storage=backups&path=nope%2Fx",
        false,
    );
    l.send(Msg::Browse(Box::new(browse_of("readable"))));
    assert!(present(&l.r, "file-missing"));

    let mut denied = browse_of("denied");
    denied.files.clear();
    l.send(Msg::Browse(Box::new(denied)));
    assert!(
        l.text().contains("You do not have permission to do that."),
        "{}",
        l.text()
    );

    let mut missing = browse_of("missing");
    missing.files.clear();
    l.send(Msg::Browse(Box::new(missing)));
    assert!(l.text().contains("No such storage."), "{}", l.text());
}

#[quench_test::test]
fn an_empty_storage_and_a_cut_listing_say_so() {
    let l = started("/warehouse/ui/files/browse?storage=backups", false);
    let mut empty = browse_of("readable");
    empty.files.clear();
    l.send(Msg::Browse(Box::new(empty)));
    assert!(l.text().contains("Directory is empty."), "{}", l.text());
    let mut cut = browse_of("readable");
    cut.truncated = true;
    l.send(Msg::Browse(Box::new(cut)));
    assert!(present(&l.r, "truncated"));
}

// ── Translations ─────────────────────────────────────────────────────────────

const LOCALES: [(&str, &str); 5] = [
    ("en-US", include_str!("../../service/i18n/en-US.ftl")),
    ("de-DE", include_str!("../../service/i18n/de-DE.ftl")),
    ("es-ES", include_str!("../../service/i18n/es-ES.ftl")),
    ("fr-FR", include_str!("../../service/i18n/fr-FR.ftl")),
    ("pl-PL", include_str!("../../service/i18n/pl-PL.ftl")),
];

fn keys(source: &str) -> Vec<String> {
    let mut keys: Vec<String> = source
        .lines()
        .filter_map(|line| {
            line.split_once(" = ")
                .map(|(key, _)| key.trim().to_string())
        })
        .filter(|key| !key.starts_with('#') && !key.is_empty())
        .collect();
    keys.sort();
    keys
}

#[quench_test::test]
fn every_language_has_the_same_messages() {
    let english = keys(LOCALES[0].1);
    assert!(english.len() > 100);
    for (code, source) in &LOCALES[1..] {
        assert_eq!(keys(source), english, "{code} differs from en-US");
    }
}

#[quench_test::test]
fn the_shell_messages_are_there() {
    for key in [
        "ui_theme_light",
        "ui_theme_dark",
        "ui_theme_palette",
        "ui_logout",
        "locale_label",
        "ui_profile_edit",
        "ui_dashboard_button",
        "ui_home_button",
        "ui_user_menu",
        "ui_palette_green",
        "ui_loading",
        "ui_not_found",
        "ui_session_ended",
    ] {
        for (code, source) in LOCALES {
            assert!(keys(source).contains(&key.to_string()), "{code}: {key}");
        }
    }
}

#[quench_test::test]
fn yanking_the_version_that_is_open_by_default_keeps_it_open() {
    // Nothing chosen: the newest not yanked is open, and yanking it would move the screen to another version.
    let l = started("/warehouse/ui/rivets/catalog?repo=base", true);
    l.send(Msg::Rivets(rivets()));
    l.click("button", "yank");
    assert_eq!(l.url(), "/warehouse/ui/rivets/catalog?repo=base&tag=1.0.0");

    let l = started("/warehouse/ui/crates/catalog?repo=alpha", false);
    l.send(Msg::Crates(crate_names()));
    l.send(Msg::CrateVersions {
        name: "alpha".into(),
        versions: vec![version("1.0.0", false), version("1.1.0", false)],
    });
    l.click("button", "yank");
    assert_eq!(l.url(), "/warehouse/ui/crates/catalog?repo=alpha&tag=1.1.0");
}

quench_test::main!();
