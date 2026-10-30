#![allow(clippy::unwrap_used, clippy::too_many_lines, clippy::type_complexity)]

use std::cell::RefCell;
use std::rc::Rc;

use quench_core::future::block_on;
use switchboard_app::{Msg, Request};
use switchboard_core::{FilterDraft, LaunchDraft};
use switchboard_web::client::{
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

fn stand_in(
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

const MODEL: &str = r#"{"source":"HF","name":"llama","path":"/m/llama","architecture":null,"vllm_supported":true,
    "quant":"FP16","context":"ALL","layers":32,"hidden_size":4096,"params_billion":7.0,
    "estimates":[{"quant":"FP16","context":4096,"weights_gb":14.0,"kv_gb":1.0,"total_gb":15.0}]}"#;

const INSTANCE: &str = r#"{"id":"vllm-1","namespace":"native","model":"llama","host":"0.0.0.0","port":8000,
    "quantization":null,"max_model_len":null,"gpu_memory_utilization":0.9,"enable_prefix_caching":false,
    "enable_tool_calling":false,"started_at":"2026-10-29T10:01:02Z","status":"running","log_path":null,"last_error":null}"#;

#[quench_test::test]
fn paths_and_bases() {
    assert_eq!(segment("vllm 1/x"), "vllm%201%2Fx");
    assert_eq!(base_of("/switchboard/ui/home"), "/switchboard");
    assert_eq!(base_of("/ui/vllm/manage"), "");
    assert_eq!(base_of("/switchboard/ui"), "/switchboard");
}

#[quench_test::test]
fn startup_reads_the_capabilities() {
    let (client, seen, _) = stand_in(|_| {
        (
            200,
            r#"{"models_dashboard":true,"vllm_management":true,"can_launch":false,"can_stop":false,"can_delete_model":true,"namespace":"native"}"#.into(),
        )
    });
    let msgs = block_on(client.execute(Request::Startup));
    assert_eq!(
        seen.borrow().as_slice(),
        [("GET".to_string(), "/api/v1/vllm/capabilities".to_string())]
    );
    let [Msg::Capabilities(caps)] = msgs.as_slice() else {
        panic!("{msgs:?}");
    };
    assert!(caps.models_dashboard && caps.can_delete_model && !caps.can_launch);
}

#[quench_test::test]
fn the_gpu_and_the_instances_are_plain_reads() {
    let (client, seen, _) = stand_in(|r| {
        if r.path.contains("gpu") {
            (
                200,
                r#"{"name":"card1","total_gb":32.0,"used_gb":8.0,"free_gb":24.0}"#.into(),
            )
        } else {
            (200, format!("[{INSTANCE}]"))
        }
    });
    let gpu = block_on(client.execute(Request::Gpu));
    assert!(matches!(gpu.as_slice(), [Msg::Gpu(g)] if g.free_gb > 23.0));
    let list = block_on(client.execute(Request::Instances));
    assert!(matches!(list.as_slice(), [Msg::Instances(l)] if l.len() == 1 && l[0].id == "vllm-1"));
    assert_eq!(
        seen.borrow().as_slice(),
        [
            ("GET".to_string(), "/api/v1/gpu/status".to_string()),
            ("GET".to_string(), "/api/v1/vllm/instances".to_string())
        ]
    );
}

#[quench_test::test]
fn the_models_are_asked_with_the_filters_and_the_answer_carries_them() {
    let (client, seen, bodies) = stand_in(|_| (200, format!("[{MODEL}]")));
    let filters = FilterDraft {
        search: "lla".into(),
        vllm_only: true,
        ..FilterDraft::default()
    };
    let body = filters.body();
    let msgs = block_on(client.execute(Request::Models(body.clone())));
    let [Msg::Models(back, list)] = msgs.as_slice() else {
        panic!("{msgs:?}");
    };
    assert_eq!(back, &body);
    assert_eq!(list[0].name, "llama");
    assert_eq!(
        seen.borrow()[0],
        ("POST".to_string(), "/api/v1/models/list".to_string())
    );
    let sent = bodies.borrow()[0].clone().unwrap();
    for part in [
        r#""source":"hf""#,
        r#""search":"lla""#,
        r#""vllm_only":true"#,
    ] {
        assert!(sent.contains(part), "{sent}");
    }
}

#[quench_test::test]
fn the_catalog_asks_for_every_source() {
    let (client, _, bodies) = stand_in(|_| (200, format!("[{MODEL}]")));
    let msgs = block_on(client.execute(Request::Catalog));
    assert!(matches!(msgs.as_slice(), [Msg::Catalog(l)] if l.len() == 1));
    assert!(
        bodies.borrow()[0]
            .clone()
            .unwrap()
            .contains(r#""source":"all""#)
    );
}

#[quench_test::test]
fn deleting_a_model_posts_its_path() {
    let (client, seen, bodies) = stand_in(|_| (200, String::new()));
    let msgs = block_on(client.execute(Request::DeleteModel("/m/llama".into())));
    assert_eq!(msgs, vec![Msg::ModelDeleted("/m/llama".into())]);
    assert_eq!(
        seen.borrow()[0],
        ("POST".to_string(), "/api/v1/models/delete".to_string())
    );
    assert_eq!(
        bodies.borrow()[0].as_deref(),
        Some(r#"{"path":"/m/llama"}"#)
    );
}

#[quench_test::test]
fn a_refused_delete_keeps_the_services_words() {
    let (client, _, _) = stand_in(|_| (403, "api_error_invalid_model_path".into()));
    let msgs = block_on(client.execute(Request::DeleteModel("/etc".into())));
    assert_eq!(
        msgs,
        vec![Msg::Rejected("api_error_invalid_model_path".into())]
    );
    let (client, _, _) = stand_in(|_| (403, String::new()));
    let msgs = block_on(client.execute(Request::DeleteModel("/m".into())));
    assert!(matches!(msgs.as_slice(), [Msg::Rejected(t)] if t.contains("may not")));
}

#[quench_test::test]
fn launching_posts_the_body_and_stopping_deletes_the_instance() {
    let (client, seen, bodies) = stand_in(|_| (202, INSTANCE.into()));
    let mut draft = LaunchDraft::new("native");
    draft.model = "llama".into();
    let msgs = block_on(client.execute(Request::Launch(draft.to_body().unwrap())));
    assert_eq!(msgs, vec![Msg::Launched]);
    assert_eq!(
        seen.borrow()[0],
        ("POST".to_string(), "/api/v1/vllm/instances".to_string())
    );
    let sent = bodies.borrow()[0].clone().unwrap();
    for part in [
        r#""model":"llama""#,
        r#""host":"0.0.0.0""#,
        r#""port":8000"#,
        r#""namespace":"native""#,
        r#""gpu_memory_utilization":0.9"#,
        r#""enable_prefix_caching":false"#,
    ] {
        assert!(sent.contains(part), "{part}: {sent}");
    }

    let (client, seen, _) = stand_in(|_| (200, "<div></div>".into()));
    let msgs = block_on(client.execute(Request::Stop("vllm 1".into())));
    assert_eq!(msgs, vec![Msg::Stopped("vllm 1".into())]);
    assert_eq!(
        seen.borrow()[0],
        (
            "DELETE".to_string(),
            "/api/v1/vllm/instances/vllm%201".to_string()
        )
    );
}

#[quench_test::test]
fn the_failures_become_messages() {
    let (client, _, _) = stand_in(|_| (401, String::new()));
    assert_eq!(block_on(client.execute(Request::Gpu)), vec![Msg::SignedOut]);
    assert_eq!(
        block_on(client.execute(Request::Stop("x".into()))),
        vec![Msg::SignedOut]
    );

    let (client, _, _) = stand_in(|_| (403, String::new()));
    let msgs = block_on(client.execute(Request::Instances));
    assert!(matches!(msgs.as_slice(), [Msg::Failed(t)] if t.contains("may not see")));

    let (client, _, _) = stand_in(|_| (500, "boom".into()));
    let msgs = block_on(client.execute(Request::Instances));
    assert!(matches!(msgs.as_slice(), [Msg::Failed(t)] if t.contains("500")));

    // A refused change keeps the service's text; an error it did not explain is its status.
    let (client, _, _) = stand_in(|_| (500, "api_error_vllm_stop_failed".into()));
    assert_eq!(
        block_on(client.execute(Request::Stop("x".into()))),
        vec![Msg::Rejected("api_error_vllm_stop_failed".into())]
    );
    let (client, _, _) = stand_in(|_| (502, "<html>bad gateway</html>".into()));
    let msgs = block_on(client.execute(Request::Stop("x".into())));
    assert!(matches!(msgs.as_slice(), [Msg::Rejected(t)] if t.contains("502")));

    let (client, _, _) = stand_in(|_| (404, String::new()));
    let msgs = block_on(client.execute(Request::Stop("x".into())));
    assert!(matches!(msgs.as_slice(), [Msg::Rejected(t)] if t.contains("gone")));

    let (client, _, _) = stand_in(|_| (200, "not json".into()));
    let msgs = block_on(client.execute(Request::Gpu));
    assert!(matches!(msgs.as_slice(), [Msg::Failed(t)] if t.contains("unreadable")));
}

#[quench_test::test]
fn the_language_is_the_browsers_business() {
    let (client, seen, _) = stand_in(|_| (200, String::new()));
    assert!(block_on(client.execute(Request::Language("de-DE".into()))).is_empty());
    assert!(seen.borrow().is_empty());
}

quench_test::main!();
