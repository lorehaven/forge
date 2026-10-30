#![allow(clippy::unwrap_used, clippy::too_many_lines, clippy::type_complexity)]

use std::cell::RefCell;
use std::rc::Rc;

use quench_core::future::block_on;
use warehouse_app::{Msg, Request};
use warehouse_core::{NewStorage, StorageChanges};
use warehouse_web::client::{
    Boxed, Client, HttpRequest, HttpResponse, Transport, base_of, segment,
};

type Handler = Box<dyn Fn(&HttpRequest) -> (u16, String)>;
type Seen = Rc<RefCell<Vec<(String, String)>>>;
type Bodies = Rc<RefCell<Vec<Option<String>>>>;

/// A service that answers from a function and remembers what it was asked.
struct Standin {
    seen: Seen,
    bodies: Bodies,
    handler: Handler,
}

impl Transport for Standin {
    fn send(&self, request: HttpRequest) -> Boxed<'_, Result<HttpResponse, String>> {
        let (status, body) = (self.handler)(&request);
        self.seen
            .borrow_mut()
            .push((request.method.to_string(), request.path.clone()));
        self.bodies.borrow_mut().push(request.body);
        Box::pin(async move { Ok(HttpResponse { status, body }) })
    }
}

fn client(
    handler: impl Fn(&HttpRequest) -> (u16, String) + 'static,
) -> (Client<Standin>, Seen, Bodies) {
    let seen: Seen = Rc::default();
    let bodies: Bodies = Rc::default();
    (
        Client::new(Standin {
            seen: seen.clone(),
            bodies: bodies.clone(),
            handler: Box::new(handler),
        }),
        seen,
        bodies,
    )
}

const STORAGE: &str = r#"{"name":"photos","owner":"dev","max_file_bytes":null,"quota_bytes":100,"used_bytes":0,"sync_enabled":true,"created":"2026-10-29 10:00 UTC"}"#;
const STORAGES: &str = r#"{"fixed":[{"name":"shared","root":"/srv"}],"dynamic":[]}"#;

fn routes(request: &HttpRequest) -> (u16, String) {
    let body = match (request.method, request.path.as_str()) {
        ("GET", "/ui/api/overview") => r#"{"username":"dev","can_manage":true,"features":{"docker":true,"crates":true,"files":true,"artifacts":true,"rivets":true}}"#.to_string(),
        ("GET", "/ui/api/docker/repositories") => r#"["app","team/api"]"#.to_string(),
        ("GET", "/ui/api/docker/tags?repo=app") => {
            r#"[{"tag":"1","digest":"sha256:aa","media_type":null,"size_bytes":12}]"#.to_string()
        }
        ("POST", "/ui/api/docker/delete-image") => String::new(),
        ("GET", "/ui/api/crates") => r#"["alpha"]"#.to_string(),
        ("GET", "/ui/api/crates/alpha/versions") => r#"[{"vers":"1.0.0","yanked":false,"cksum":"c"}]"#.to_string(),
        ("POST", "/ui/api/crates/yank" | "/ui/api/crates/unyank") => String::new(),
        ("GET", "/ui/api/artifacts") => r#"[{"program":"a","platform":"linux","format":"tar.gz","version_code":1,"version_name":"1.0","filename":"f","size_bytes":3,"sha256":"s","uploaded_by":"dev","yanked":false}]"#.to_string(),
        ("POST", "/ui/api/artifacts/yank" | "/ui/api/artifacts/unyank") => String::new(),
        ("GET", "/ui/api/rivets") => "[]".to_string(),
        ("POST", "/ui/api/rivets/yank" | "/ui/api/rivets/unyank") => String::new(),
        ("GET", "/ui/api/files/storages") => STORAGES.to_string(),
        ("POST", "/ui/api/files/storages" | "/ui/api/files/storages/photos/edit") => {
            STORAGE.to_string()
        }
        ("POST", "/ui/api/files/storages/photos/delete" | "/ui/api/files/delete-file") => {
            String::new()
        }
        ("GET", "/ui/api/files/browse?storage=photos") => {
            r#"{"storage":"photos","access":"readable","files":[{"path":"a/b.txt","size":3}],"truncated":false}"#.to_string()
        }
        _ => return (404, r#"{"error":"nothing here"}"#.to_string()),
    };
    // The status of a change with no answer.
    if request.method == "POST" && body.is_empty() {
        return (204, body);
    }
    (200, body)
}

fn paths(seen: &Seen) -> Vec<String> {
    seen.borrow()
        .iter()
        .map(|(m, p)| format!("{m} {p}"))
        .collect()
}

#[quench_test::test]
fn the_service_prefix_is_what_comes_before_ui() {
    assert_eq!(base_of("/warehouse/ui/home"), "/warehouse");
    assert_eq!(base_of("/warehouse/ui/files/browse"), "/warehouse");
    assert_eq!(base_of("/warehouse/ui"), "/warehouse");
    assert_eq!(base_of("/ui/home"), "");
    assert_eq!(segment("team/app v1"), "team%2Fapp%20v1");
}

#[quench_test::test]
fn startup_reads_the_overview() {
    let (client, seen, _) = client(routes);
    let out = block_on(client.execute(Request::Startup));
    let [Msg::Overview(overview)] = &out[..] else {
        panic!("{out:?}")
    };
    assert!(overview.can_manage && overview.features.files);
    assert_eq!(paths(&seen), ["GET /ui/api/overview"]);
}

#[quench_test::test]
fn docker_is_read_and_a_missing_repository_has_no_tags() {
    let (client, seen, _) = client(routes);
    assert!(matches!(
        &block_on(client.execute(Request::DockerRepos))[..],
        [Msg::DockerRepos(list)] if list.len() == 2
    ));
    let out = block_on(client.execute(Request::DockerTags("app".into())));
    let [Msg::DockerTags { repo, tags }] = &out[..] else {
        panic!("{out:?}")
    };
    assert_eq!((repo.as_str(), tags.len()), ("app", 1));
    let out = block_on(client.execute(Request::DockerTags("team/api".into())));
    assert!(
        matches!(&out[..], [Msg::DockerTags { tags, .. }] if tags.is_empty()),
        "{out:?}"
    );
    assert_eq!(
        paths(&seen)[2],
        "GET /ui/api/docker/tags?repo=team%2Fapi",
        "the name is one encoded value"
    );
}

#[quench_test::test]
fn deleting_an_image_posts_it_and_reads_the_tags_again() {
    let (client, seen, bodies) = client(routes);
    let out = block_on(client.execute(Request::DeleteImage {
        repository: "app".into(),
        digest: "sha256:aa".into(),
    }));
    assert!(
        matches!(&out[..], [Msg::DockerTags { .. }, Msg::ImageDeleted(r)] if r == "app"),
        "{out:?}"
    );
    assert_eq!(
        paths(&seen),
        [
            "POST /ui/api/docker/delete-image",
            "GET /ui/api/docker/tags?repo=app"
        ]
    );
    assert_eq!(
        bodies.borrow()[0].as_deref(),
        Some(r#"{"repository":"app","digest":"sha256:aa"}"#)
    );
}

#[quench_test::test]
fn crates_are_read_and_yanking_reads_the_versions_again() {
    let (client, seen, bodies) = client(routes);
    assert!(matches!(
        &block_on(client.execute(Request::Crates))[..],
        [Msg::Crates(list)] if list == &["alpha".to_string()]
    ));
    let out = block_on(client.execute(Request::CrateVersions("alpha".into())));
    assert!(
        matches!(&out[..], [Msg::CrateVersions { versions, .. }] if versions[0].vers == "1.0.0")
    );
    // A crate the index does not know has no versions.
    let out = block_on(client.execute(Request::CrateVersions("ghost".into())));
    assert!(matches!(&out[..], [Msg::CrateVersions { versions, .. }] if versions.is_empty()));

    seen.borrow_mut().clear();
    bodies.borrow_mut().clear();
    let out = block_on(client.execute(Request::SetCrateYank {
        name: "alpha".into(),
        version: "1.0.0".into(),
        yanked: true,
    }));
    assert!(matches!(&out[..], [Msg::CrateVersions { .. }]));
    block_on(client.execute(Request::SetCrateYank {
        name: "alpha".into(),
        version: "1.0.0".into(),
        yanked: false,
    }));
    assert_eq!(
        paths(&seen),
        [
            "POST /ui/api/crates/yank",
            "GET /ui/api/crates/alpha/versions",
            "POST /ui/api/crates/unyank",
            "GET /ui/api/crates/alpha/versions"
        ]
    );
    assert_eq!(
        bodies.borrow()[0].as_deref(),
        Some(r#"{"name":"alpha","version":"1.0.0"}"#)
    );
}

#[quench_test::test]
fn artifacts_and_rivets_yank_and_read_again() {
    let (client, seen, bodies) = client(routes);
    let out = block_on(client.execute(Request::SetArtifactYank {
        program: "a".into(),
        platform: "linux".into(),
        version_code: 1,
        yanked: true,
    }));
    assert!(
        matches!(&out[..], [Msg::Artifacts(list)] if list.len() == 1),
        "{out:?}"
    );
    assert_eq!(
        bodies.borrow()[0].as_deref(),
        Some(r#"{"program":"a","platform":"linux","version_code":1}"#)
    );
    let out = block_on(client.execute(Request::SetRivetYank {
        name: "base".into(),
        version: "1.0.0".into(),
        yanked: false,
    }));
    assert!(
        matches!(&out[..], [Msg::Rivets(list)] if list.is_empty()),
        "{out:?}"
    );
    assert_eq!(
        paths(&seen),
        [
            "POST /ui/api/artifacts/yank",
            "GET /ui/api/artifacts",
            "POST /ui/api/rivets/unyank",
            "GET /ui/api/rivets"
        ]
    );
}

#[quench_test::test]
fn storages_are_created_edited_and_removed() {
    let (client, seen, bodies) = client(routes);
    let out = block_on(client.execute(Request::Storages));
    assert!(matches!(&out[..], [Msg::Storages(s)] if s.fixed.len() == 1));

    let out = block_on(client.execute(Request::CreateStorage(NewStorage {
        name: "photos".into(),
        owner: "dev".into(),
        quota_bytes: Some(100),
        max_file_bytes: None,
        sync_enabled: true,
    })));
    assert!(
        matches!(&out[..], [Msg::Storages(_), Msg::StorageSaved(n)] if n == "photos"),
        "{out:?}"
    );
    let out = block_on(client.execute(Request::EditStorage {
        name: "photos".into(),
        changes: StorageChanges {
            quota_bytes: None,
            max_file_bytes: Some(5),
            clear_max_file: false,
            sync_enabled: false,
        },
    }));
    assert!(matches!(&out[..], [Msg::Storages(_), Msg::StorageSaved(_)]));
    let out = block_on(client.execute(Request::DeleteStorage("photos".into())));
    assert!(
        matches!(&out[..], [Msg::Storages(_), Msg::StorageDeleted]),
        "{out:?}"
    );

    assert_eq!(
        paths(&seen),
        [
            "GET /ui/api/files/storages",
            "POST /ui/api/files/storages",
            "GET /ui/api/files/storages",
            "POST /ui/api/files/storages/photos/edit",
            "GET /ui/api/files/storages",
            "POST /ui/api/files/storages/photos/delete",
            "GET /ui/api/files/storages",
        ]
    );
    assert!(
        bodies.borrow()[1]
            .as_deref()
            .unwrap()
            .contains(r#""quota_bytes":100"#)
    );
}

#[quench_test::test]
fn files_are_browsed_and_deleting_one_reads_the_directory_again() {
    let (client, seen, bodies) = client(routes);
    let out = block_on(client.execute(Request::Browse("photos".into())));
    assert!(matches!(&out[..], [Msg::Browse(b)] if b.files.len() == 1 && b.access == "readable"));
    let out = block_on(client.execute(Request::DeleteFile {
        storage: "photos".into(),
        path: "a/b.txt".into(),
    }));
    assert!(
        matches!(&out[..], [Msg::Browse(_), Msg::FileDeleted { parent, .. }] if parent == "a"),
        "{out:?}"
    );
    assert_eq!(
        paths(&seen),
        [
            "GET /ui/api/files/browse?storage=photos",
            "POST /ui/api/files/delete-file",
            "GET /ui/api/files/browse?storage=photos"
        ]
    );
    assert_eq!(
        bodies.borrow()[1].as_deref(),
        Some(r#"{"storage":"photos","path":"a/b.txt"}"#)
    );
}

#[quench_test::test]
fn the_language_is_the_browsers_own_business() {
    let (client, seen, _) = client(routes);
    assert!(block_on(client.execute(Request::Language("pl-PL".into()))).is_empty());
    assert!(seen.borrow().is_empty());
}

#[quench_test::test]
fn a_lost_session_a_refusal_and_a_dead_service_are_told_apart() {
    let (answering, _, _) = client(|_| (401, r#"{"error":"sign in"}"#.into()));
    assert_eq!(
        block_on(answering.execute(Request::Startup)),
        [Msg::SignedOut]
    );
    assert_eq!(
        block_on(answering.execute(Request::SetCrateYank {
            name: "a".into(),
            version: "1".into(),
            yanked: true
        })),
        [Msg::SignedOut]
    );

    let (answering, _, _) = client(|_| (403, r#"{"error":"api_error_forbidden"}"#.into()));
    assert!(matches!(
        &block_on(answering.execute(Request::Artifacts))[..],
        [Msg::Failed(why)] if why.contains("may not see Warehouse")
    ));
    assert_eq!(
        block_on(answering.execute(Request::SetArtifactYank {
            program: "a".into(),
            platform: "linux".into(),
            version_code: 1,
            yanked: true
        })),
        [Msg::Rejected("api_error_forbidden".into())],
        "the service's key goes to the app, which translates it"
    );

    let (answering, _, _) = client(|_| (409, "plain words".into()));
    assert_eq!(
        block_on(answering.execute(Request::DeleteStorage("x".into()))),
        [Msg::Rejected("the service answered 409".into())]
    );

    let (answering, _, _) = client(|_| (500, "oops".into()));
    assert_eq!(
        block_on(answering.execute(Request::Rivets)),
        [Msg::Failed("the service answered 500".into())]
    );

    let (answering, _, _) = client(|_| (200, "not json".into()));
    assert!(matches!(
        &block_on(answering.execute(Request::Crates))[..],
        [Msg::Failed(why)] if why.contains("unreadable")
    ));
}

quench_test::main!();
