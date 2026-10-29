#![allow(clippy::unwrap_used, clippy::too_many_lines, clippy::type_complexity)]

use std::cell::RefCell;
use std::rc::Rc;

use daybook_core::{Msg, Request};
use daybook_host::{Controller, Op, Platform, Reply};
use daybook_net::{NetError, Store};
use daybook_web::client::{Boxed, Client, HttpRequest, HttpResponse, Keep, Transport};
use daybook_web::platform::{Later, Spawn, Web};
use quench_core::future::block_on;
use quench_core::time::Time;
use quench_tz::Zone;

type Handler = Box<dyn Fn(&HttpRequest) -> HttpResponse>;

/// A server that answers from a function and remembers what it was asked.
struct Standin {
    seen: Rc<RefCell<Vec<HttpRequest>>>,
    handler: Handler,
}

impl Transport for Standin {
    fn send(&self, request: HttpRequest) -> Boxed<'_, Result<HttpResponse, String>> {
        let answer = (self.handler)(&request);
        self.seen.borrow_mut().push(request);
        Box::pin(async move { Ok(answer) })
    }
}

#[derive(Clone, Default)]
struct Kept(Rc<RefCell<Option<String>>>);

impl Keep for Kept {
    fn load(&self) -> Option<String> {
        self.0.borrow().clone()
    }
    fn save(&self, token: Option<&str>) {
        *self.0.borrow_mut() = token.map(str::to_string);
    }
}

fn reply(status: u16, body: &str) -> HttpResponse {
    HttpResponse {
        status,
        body: body.to_string(),
    }
}

fn tokens(n: u32) -> HttpResponse {
    reply(
        200,
        &format!(
            r#"{{"access_token":"access-{n}","refresh_token":"refresh-{n}","token_type":"Bearer","expires_in":900}}"#
        ),
    )
}

const TASK: &str = r#"{"id":"t1","project_id":null,"content":"Milk","is_completed":false,"sort_order":0,"due_at":null,"due_all_day":false,"label_ids":[],"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-01T00:00:00Z"}"#;

type Rig = (Client<Standin, Kept>, Rc<RefCell<Vec<HttpRequest>>>, Kept);

/// A gatehouse that numbers its tokens and a daybook that answers with `api`.
fn rig(stored: Option<&str>, api: impl Fn(&HttpRequest) -> HttpResponse + 'static) -> Rig {
    let seen: Rc<RefCell<Vec<HttpRequest>>> = Rc::default();
    let issued = RefCell::new(0_u32);
    let keep = Kept::default();
    keep.save(stored);
    let handler: Handler = Box::new(move |r| {
        if r.path.ends_with("/auth/login") {
            return if r.body.as_deref().is_some_and(|b| b.contains("secret")) {
                tokens(1)
            } else {
                reply(401, "{}")
            };
        }
        if r.path.ends_with("/auth/refresh") {
            return if r.body.as_deref().is_some_and(|b| b.contains("spent")) {
                reply(401, "{}")
            } else {
                *issued.borrow_mut() += 1;
                tokens(*issued.borrow() + 1)
            };
        }
        api(r)
    });
    (
        Client::new(
            Standin {
                seen: Rc::clone(&seen),
                handler,
            },
            keep.clone(),
        ),
        seen,
        keep,
    )
}

fn login(c: &Client<Standin, Kept>, password: &str) -> Result<Reply, NetError> {
    block_on(c.execute(Op::Login {
        server: String::new(),
        username: "me".into(),
        password: password.into(),
    }))
}

#[quench_test::test]
fn signing_in_keeps_the_refresh_token_and_loads_with_the_access_token() {
    let (c, seen, keep) = rig(None, |r| {
        if r.path.ends_with("/tasks?all=true&completed=false") {
            reply(200, &format!("[{TASK}]"))
        } else {
            reply(200, "[]")
        }
    });
    assert!(!c.signed_in());
    login(&c, "secret").unwrap();
    assert_eq!(keep.load().as_deref(), Some("refresh-1"));
    let Reply::Loaded(data) = block_on(c.execute(Op::Load)).unwrap() else {
        panic!("not loaded")
    };
    assert_eq!(data.tasks.len(), 1);
    let seen = seen.borrow();
    let paths: Vec<&str> = seen.iter().skip(1).map(|r| r.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "/daybook/api/v1/projects",
            "/daybook/api/v1/labels",
            "/daybook/api/v1/tasks?all=true&completed=false"
        ]
    );
    assert!(
        seen.iter()
            .skip(1)
            .all(|r| r.bearer.as_deref() == Some("access-1"))
    );
}

#[quench_test::test]
fn a_wrong_password_is_unauthorized_and_keeps_nothing() {
    let (c, _seen, keep) = rig(None, |_| reply(200, "[]"));
    assert_eq!(login(&c, "nope").unwrap_err(), NetError::Unauthorized);
    assert_eq!(keep.load(), None);
}

#[quench_test::test]
fn a_kept_token_is_renewed_before_the_first_call_and_the_new_one_kept() {
    let (c, seen, keep) = rig(Some("refresh-0"), |_| reply(200, "[]"));
    assert!(c.signed_in());
    block_on(c.execute(Op::DeleteTask("t1".into()))).unwrap();
    assert!(seen.borrow()[0].path.ends_with("/auth/refresh"));
    assert_eq!(keep.load().as_deref(), Some("refresh-2"));
}

#[quench_test::test]
fn a_refused_token_is_renewed_once_and_the_call_sent_again() {
    let (c, seen, _keep) = rig(Some("refresh-0"), |r| {
        if r.bearer.as_deref() == Some("access-2") {
            reply(401, "{}")
        } else {
            reply(200, "[]")
        }
    });
    block_on(c.execute(Op::DeleteTask("t1".into()))).unwrap();
    let sent: Vec<String> = seen
        .borrow()
        .iter()
        .map(|r| {
            r.bearer
                .clone()
                .unwrap_or_else(|| r.path.rsplit('/').next().unwrap().to_string())
        })
        .collect();
    assert_eq!(sent, ["refresh", "access-2", "refresh", "access-3"]);
}

#[quench_test::test]
fn a_refresh_the_server_refuses_signs_out() {
    let (c, _seen, keep) = rig(Some("spent"), |_| reply(200, "[]"));
    assert_eq!(
        block_on(c.execute(Op::DeleteTask("t1".into()))).unwrap_err(),
        NetError::Unauthorized
    );
    assert_eq!(keep.load(), None);
    assert!(!c.signed_in());
}

#[quench_test::test]
fn the_services_answers_become_errors_a_screen_can_say() {
    for (status, error) in [
        (409, NetError::NameTaken),
        (404, NetError::NotFound),
        (400, NetError::Invalid),
        (500, NetError::Status(500)),
    ] {
        let (c, _s, _k) = rig(Some("refresh-0"), move |_| reply(status, "{}"));
        assert_eq!(
            block_on(c.execute(Op::CreateLabel {
                name: "x".into(),
                color: "#112233".into()
            }))
            .unwrap_err(),
            error
        );
    }
}

#[quench_test::test]
fn an_update_replaces_the_task_and_a_delete_has_no_body() {
    let (c, seen, _k) = rig(Some("refresh-0"), |r| {
        if r.method == "DELETE" {
            reply(204, "")
        } else {
            reply(200, TASK)
        }
    });
    let mut body = quench_serde::Map::new();
    body.insert("content", quench_serde::Value::String("Milk".into()));
    let Reply::Task(t) = block_on(c.execute(Op::UpdateTask(
        "t 1".into(),
        quench_serde::Value::Object(body),
    )))
    .unwrap() else {
        panic!()
    };
    assert_eq!(t.id, "t1");
    block_on(c.execute(Op::DeleteTask("t1".into()))).unwrap();
    let seen = seen.borrow();
    let put = seen.iter().find(|r| r.method == "PUT").unwrap();
    assert_eq!(put.path, "/daybook/api/v1/tasks/t%201");
    assert!(put.body.as_deref().unwrap().contains("\"content\""));
    assert!(
        seen.iter()
            .find(|r| r.method == "DELETE")
            .unwrap()
            .body
            .is_none()
    );
}

type Waiting = Rc<RefCell<Vec<Boxed<'static, ()>>>>;

fn web(
    handler: impl Fn(&HttpRequest) -> HttpResponse + 'static,
) -> (Web<Standin, Kept>, Waiting, Rc<RefCell<Vec<HttpRequest>>>) {
    let (client, seen, _keep) = rig(Some("refresh-0"), handler);
    let waiting: Waiting = Rc::default();
    let queue = Rc::clone(&waiting);
    let spawn: Spawn = Rc::new(move |f| queue.borrow_mut().push(f));
    let later: Later = Rc::new(|_, _| {});
    (
        Web::new(
            client,
            spawn,
            Rc::new(|| Time::from_unix(0)),
            Zone::utc(),
            later,
        ),
        waiting,
        seen,
    )
}

/// The next task to run; the list is not borrowed while it runs.
fn take(waiting: &Waiting) -> Option<Boxed<'static, ()>> {
    waiting.borrow_mut().pop()
}

#[quench_test::test]
fn calls_run_one_after_another() {
    let (web, waiting, seen) = web(|_| reply(200, "[]"));
    let done: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    for name in ["first", "second", "third"] {
        let done = Rc::clone(&done);
        web.call(
            Op::DeleteTask("t1".into()),
            Box::new(move |_| done.borrow_mut().push(name)),
        );
    }
    assert_eq!(waiting.borrow().len(), 1, "only the first has started");
    while let Some(next) = take(&waiting) {
        block_on(next);
    }
    assert_eq!(*done.borrow(), ["first", "second", "third"]);
    let gets = seen
        .borrow()
        .iter()
        .filter(|r| r.path.contains("/tasks/t1"))
        .count();
    assert_eq!(gets, 3);
    assert_eq!(
        seen.borrow()
            .iter()
            .filter(|r| r.path.ends_with("/auth/refresh"))
            .count(),
        1,
        "the token is renewed once, not once per call"
    );
}

#[quench_test::test]
fn the_controller_runs_on_the_page_s_platform() {
    let (web, waiting, _seen) = web(|r| {
        if r.path.ends_with("/projects") || r.path.ends_with("/labels") {
            reply(200, "[]")
        } else {
            reply(200, &format!("[{TASK}]"))
        }
    });
    let out: Rc<RefCell<Vec<Msg>>> = Rc::default();
    let sink = Rc::clone(&out);
    let c = Controller::new(
        Rc::new(web),
        Store::new("/nonexistent"),
        Rc::new(move |m| sink.borrow_mut().push(m)),
    );
    c.start();
    loop {
        let next = waiting.borrow_mut().pop();
        match next {
            Some(f) => block_on(f),
            None => break,
        }
    }
    let data = out
        .borrow()
        .iter()
        .rev()
        .find_map(|m| {
            if let Msg::Data(d) = m {
                Some((**d).clone())
            } else {
                None
            }
        })
        .expect("data");
    assert_eq!(data.tasks.len(), 1);
    assert!(out.borrow().contains(&Msg::Loading(false)));
    c.request(Request::Logout);
}

quench_test::main!();
