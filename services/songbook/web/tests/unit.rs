#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use songbook_app::{Msg, Request};
use songbook_core::{Filter, JobStart, NewArtist};
use songbook_web::protocol::{API, Call, base_of, call_for, interpret, unreachable};

fn call(request: &Request) -> Call {
    call_for(request).unwrap()
}

#[quench_test::test]
fn the_prefix_is_what_comes_before_ui() {
    assert_eq!(base_of("/songbook/ui/library"), "/songbook");
    assert_eq!(base_of("/ui/library"), "");
    assert_eq!(base_of("/a/b/ui"), "/a/b");
    assert_eq!(base_of("/"), "");
}

#[quench_test::test]
fn reads_are_gets_under_the_admin_api() {
    assert_eq!(
        call(&Request::State),
        Call {
            method: "GET",
            path: format!("{API}/state"),
            body: None
        }
    );
    assert_eq!(call(&Request::Stats).path, format!("{API}/stats"));
    let genres = call(&Request::SetGenres {
        name: "WC".into(),
        genres: "Rap/Hip Hop".into(),
    });
    assert_eq!(genres.path, format!("{API}/artists/genres"));
    assert_eq!(
        genres.body.as_deref(),
        Some(r#"{"name":"WC","genres":"Rap/Hip Hop"}"#)
    );
    assert_eq!(
        interpret(
            &Request::SetGenres {
                name: "WC".into(),
                genres: String::new()
            },
            200,
            "{}"
        ),
        Msg::CategorySet
    );
    assert_eq!(call(&Request::Schedule).path, format!("{API}/schedule"));
    let save = call(&Request::SaveSchedule(songbook_core::Schedule::default()));
    assert_eq!(
        (save.method, save.path.as_str()),
        ("POST", format!("{API}/schedule").as_str())
    );
    assert!(save.body.unwrap().contains("\"fresh_ratings\":true"));
    let saved = songbook_core::Schedule::default();
    let body = quench_serde::json::to_string(&saved);
    assert!(matches!(interpret(&Request::Schedule, 200, &body), Msg::Schedule(s) if s == saved));
    assert_eq!(
        call(&Request::Job { from: 42 }).path,
        format!("{API}/job?from=42")
    );
    assert_eq!(
        call(&Request::Browse(Filter::default())).path,
        format!("{API}/browse")
    );
    let filter = Filter {
        artist: "Pięć Dwa".into(),
        q: "a&b".into(),
        links: "missing".into(),
        ..Filter::default()
    };
    assert_eq!(
        call(&Request::Browse(filter)).path,
        format!("{API}/browse?artist=Pi%C4%99%C4%87%20Dwa&q=a%26b&links=missing")
    );
}

#[quench_test::test]
fn changes_are_posts_with_a_json_body() {
    let c = call(&Request::SaveLink {
        track_id: 7,
        url: "https://youtu.be/x".into(),
    });
    assert_eq!(
        (c.method, c.path.as_str(), c.body.as_deref()),
        (
            "POST",
            "/api/v1/admin/link",
            Some(r#"{"track_id":7,"url":"https://youtu.be/x"}"#)
        )
    );
    let c = call(&Request::Blacklist {
        artist: "Nate Dogg".into(),
        album_id: 9,
    });
    assert_eq!(
        (c.path.as_str(), c.body.as_deref()),
        (
            "/api/v1/admin/albums/blacklist",
            Some(r#"{"artist":"Nate Dogg","album_id":9}"#)
        )
    );
    let c = call(&Request::AddArtists(vec![NewArtist {
        name: "Gibbs".into(),
        id: String::new(),
        category: "Polish Rap".into(),
    }]));
    assert_eq!(
        (c.path.as_str(), c.body.as_deref()),
        (
            "/api/v1/admin/artists/add",
            Some(r#"{"items":[{"name":"Gibbs","id":"","category":"Polish Rap"}]}"#)
        )
    );
    let c = call(&Request::RemoveArtist("Gibbs".into()));
    assert_eq!(
        (c.path.as_str(), c.body.as_deref()),
        ("/api/v1/admin/artists/remove", Some(r#"{"name":"Gibbs"}"#))
    );
    let c = call(&Request::SetCategory {
        name: "Gibbs".into(),
        category: String::new(),
    });
    assert_eq!(
        (c.path.as_str(), c.body.as_deref()),
        (
            "/api/v1/admin/artists/category",
            Some(r#"{"name":"Gibbs","category":""}"#)
        )
    );
    let c = call(&Request::RenameCategory {
        name: "Polish Rap".into(),
        new_name: "Rap PL".into(),
    });
    assert_eq!(
        (c.path.as_str(), c.body.as_deref()),
        (
            "/api/v1/admin/categories/rename",
            Some(r#"{"name":"Polish Rap","new_name":"Rap PL"}"#)
        )
    );
    let c = call(&Request::AddCategory("Latino".into()));
    assert_eq!(
        (c.path.as_str(), c.body.as_deref()),
        ("/api/v1/admin/categories/add", Some(r#"{"name":"Latino"}"#))
    );
    let c = call(&Request::StartJob(JobStart {
        name: "sync".into(),
        singles: true,
        limit: Some(5),
        ..JobStart::default()
    }));
    assert_eq!(c.path, "/api/v1/admin/jobs");
    let body = c.body.unwrap();
    assert!(
        body.contains(r#""name":"sync""#)
            && body.contains(r#""singles":true"#)
            && body.contains(r#""limit":5"#),
        "{body}"
    );
}

#[quench_test::test]
fn choosing_a_language_needs_no_call() {
    assert_eq!(call_for(&Request::Language("pl-PL".into())), None);
}

#[quench_test::test]
fn answers_become_messages() {
    let state = r#"{"artists":[],"categories":["A"],"uncategorized":"Uncategorized","genres":[],"suggestions":[]}"#;
    assert!(
        matches!(interpret(&Request::State, 200, state), Msg::State(s) if s.categories == ["A"])
    );
    let numbers = r#"{"artists":61,"credited":2122,"albums":679,"songs":8395,"downloaded":8141,"missing":254,"missing_linked":1}"#;
    assert!(
        matches!(interpret(&Request::Stats, 200, numbers), Msg::Stats(s) if (s.songs, s.downloaded, s.missing_linked) == (8395, 8141, 1))
    );
    let browse = r#"{"total":1,"linked":0,"shown":1,"albums":[]}"#;
    assert!(
        matches!(interpret(&Request::Browse(Filter::default()), 200, browse), Msg::Browse(b) if b.total == 1)
    );
    let job = r#"{"name":"sync","running":true,"error":null,"log":"x","offset":1}"#;
    assert!(
        matches!(interpret(&Request::Job { from: 0 }, 200, job), Msg::Job(j) if j.running && j.offset == 1)
    );
    let save = Request::SaveLink {
        track_id: 1,
        url: String::new(),
    };
    assert_eq!(interpret(&save, 200, r#"{"updated":1}"#), Msg::LinkSaved);
    let bl = Request::Blacklist {
        artist: String::new(),
        album_id: 1,
    };
    assert_eq!(
        interpret(&bl, 200, r#"{"removed":12}"#),
        Msg::Blacklisted(12)
    );
    let add = Request::AddArtists(vec![]);
    assert_eq!(
        interpret(&add, 200, r#"{"added":["Gibbs"]}"#),
        Msg::ArtistsAdded(vec!["Gibbs".into()])
    );
    assert_eq!(
        interpret(&Request::RemoveArtist(String::new()), 200, "{}"),
        Msg::ArtistRemoved
    );
    let cat = Request::SetCategory {
        name: String::new(),
        category: String::new(),
    };
    assert_eq!(interpret(&cat, 200, r#"{"retagged":3}"#), Msg::CategorySet);
    let rename = Request::RenameCategory {
        name: "A".into(),
        new_name: "B".into(),
    };
    assert_eq!(
        interpret(&rename, 200, r#"{"retagged":3}"#),
        Msg::CategoryRenamed
    );
    assert_eq!(
        interpret(&rename, 400, r#"{"error":"category exists: B"}"#),
        Msg::Rejected("category exists: B".into())
    );
    assert_eq!(
        interpret(
            &Request::AddCategory(String::new()),
            200,
            r#"{"added":["Latino"]}"#
        ),
        Msg::CategoryAdded(vec!["Latino".into()])
    );
    let start = Request::StartJob(JobStart::default());
    assert_eq!(
        interpret(&start, 200, r#"{"started":true}"#),
        Msg::Started(true)
    );
    assert_eq!(
        interpret(&start, 200, r#"{"started":false}"#),
        Msg::Started(false)
    );
}

#[quench_test::test]
fn an_unreadable_answer_is_a_failure_not_a_crash() {
    assert!(
        matches!(interpret(&Request::State, 200, "<html>"), Msg::Failed(why) if why.contains("unreadable"))
    );
}

#[quench_test::test]
fn statuses_become_the_right_message() {
    let state = Request::State;
    assert_eq!(interpret(&state, 401, ""), Msg::SignedOut);
    assert!(matches!(interpret(&state, 403, ""), Msg::Failed(why) if why.contains("gatehouse")));
    assert_eq!(
        interpret(&state, 500, ""),
        Msg::Failed("the service answered 500".into())
    );
    assert_eq!(
        interpret(&state, 502, "bad gateway"),
        Msg::Failed("the service answered 502".into())
    );
}

#[quench_test::test]
fn a_refusal_carries_the_services_reason() {
    let save = Request::SaveLink {
        track_id: 1,
        url: "x".into(),
    };
    assert_eq!(
        interpret(
            &save,
            400,
            r#"{"error":"URL must start with http:// or https://"}"#
        ),
        Msg::Rejected("URL must start with http:// or https://".into())
    );
    assert!(matches!(interpret(&save, 400, "not json"), Msg::Rejected(why) if !why.is_empty()));
    let remove = Request::RemoveArtist("x".into());
    assert_eq!(
        interpret(
            &remove,
            409,
            r#"{"error":"A job is running; try again when it finishes."}"#
        ),
        Msg::Rejected("A job is running; try again when it finishes.".into())
    );
}

#[quench_test::test]
fn a_busy_service_refusing_a_job_is_not_a_failure() {
    let start = Request::StartJob(JobStart::default());
    assert_eq!(
        interpret(&start, 409, r#"{"error":"busy"}"#),
        Msg::Started(false)
    );
}

#[quench_test::test]
fn an_unreachable_service_is_said() {
    assert_eq!(
        unreachable("offline"),
        Msg::Failed("cannot reach the service: offline".into())
    );
}

quench_test::main!();
