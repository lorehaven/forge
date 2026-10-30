//! The screens with no host: mounted into a test tree, driven by events, read back as text.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;

use quench_ui::Event;
use quench_ui::prelude::*;
use quench_ui::router::{History, MemoryHistory};
use quench_ui::test_renderer::NodeId;
use switchboard_app::{Msg, Request, Setup, Switchboard, switchboard};
use switchboard_core::{
    Capabilities, Context, Estimate, Gpu, Instance, LaunchBody, Model, ModelFilters,
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

fn count(r: &TestRenderer, class: &str) -> usize {
    r.document_order()
        .into_iter()
        .filter(|n| {
            r.attr(*n, "class")
                .is_some_and(|c| c.split(' ').any(|c| c == class))
        })
        .count()
}

thread_local! {
    static CLOCK: RefCell<Option<Rc<quench_ui::platform::ManualPlatform>>> = const { RefCell::new(None) };
}

/// Moves this test's clock on. Every test gets a clock of its own: the platform is per thread and the runner reuses
/// threads.
fn tick(ms: f64) {
    CLOCK
        .with(|c| c.borrow().clone())
        .expect("a test that ticks starts with `live`")
        .advance(ms);
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

    fn click_attr(&self, tag: &str, attr: &str, value: &str) {
        self.r
            .fire(find(&self.r, tag, (attr, value)), &Event::Click);
    }

    fn type_into(&self, tag: &str, id: &str, text: &str) {
        self.r
            .fire(find(&self.r, tag, ("id", id)), &Event::Input(text.into()));
    }

    fn value(&self, id: &str) -> String {
        self.r
            .attr(
                self.r
                    .document_order()
                    .into_iter()
                    .find(|n| self.r.attr(*n, "id").as_deref() == Some(id))
                    .unwrap_or_else(|| panic!("no #{id}: {}", self.r.html())),
                "value",
            )
            .unwrap_or_default()
    }

    fn disabled(&self, tag: &str, id: &str) -> bool {
        self.r
            .attr(find(&self.r, tag, ("id", id)), "disabled")
            .is_some()
    }

    fn asked(&self) -> Vec<Request> {
        self.asked.borrow().clone()
    }

    fn send(&self, m: Msg) {
        (self.deliver)(m);
    }

    fn text(&self) -> String {
        self.r.text()
    }
}

fn live(path: &str) -> Live {
    let fresh = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(fresh.clone());
    CLOCK.with(|c| *c.borrow_mut() = Some(fresh));
    let asked: Asked = Rc::default();
    let log = asked.clone();
    let history = Rc::new(MemoryHistory::new(path));
    let Switchboard { view, deliver } = switchboard(
        Rc::new(move |q| log.borrow_mut().push(q)),
        Setup {
            base: "/switchboard".into(),
            language: "en-US".into(),
            login_url: "/switchboard/ui/login".into(),
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

fn caps(models: bool, vllm: bool, all: bool) -> Capabilities {
    Capabilities {
        models_dashboard: models,
        vllm_management: vllm,
        can_launch: all,
        can_stop: all,
        can_delete_model: all,
        namespace: "native".into(),
    }
}

fn gpu(total: f64, free: f64) -> Gpu {
    Gpu {
        name: "card1".into(),
        total_gb: total,
        used_gb: total - free,
        free_gb: free,
    }
}

fn estimate(quant: &str, context: u32, weights: f64, kv: f64) -> Estimate {
    Estimate {
        quant: quant.into(),
        context: Context(context),
        weights_gb: weights,
        kv_gb: kv,
        total_gb: weights + kv,
    }
}

fn model(name: &str, vllm: bool, estimates: Vec<Estimate>) -> Model {
    Model {
        source: "HF".into(),
        name: name.into(),
        path: format!("/models/{name}"),
        architecture: None,
        vllm_supported: vllm,
        quant: "FP16".into(),
        context: Context(0),
        layers: 32,
        hidden_size: 4096,
        params_billion: 7.0,
        estimates,
    }
}

fn instance(id: &str, model: &str, status: &str) -> Instance {
    Instance {
        id: id.into(),
        namespace: "native".into(),
        model: model.into(),
        host: "0.0.0.0".into(),
        port: 8000,
        quantization: None,
        dtype: Some("float16".into()),
        limit_mm_per_prompt: None,
        max_model_len: None,
        gpu_memory_utilization: Some(0.9),
        enable_prefix_caching: false,
        enable_tool_calling: false,
        task: None,
        device: None,
        started_at: "2026-10-29T10:01:02Z".into(),
        status: status.into(),
        log_path: None,
        last_error: None,
    }
}

fn default_filters() -> ModelFilters {
    switchboard_core::FilterDraft::default().body()
}

#[quench_test::test]
fn the_app_asks_for_what_is_switched_on_and_home_follows_it() {
    let app = live("/switchboard/ui/home");
    assert_eq!(app.asked(), vec![Request::Startup]);
    assert!(present(&app.r, "waiting"));
    app.send(Msg::Capabilities(caps(true, true, true)));
    assert!(present(&app.r, "tile-models") && present(&app.r, "tile-vllm"));
    assert!(app.text().contains("AI Models Registry"));
    assert!(app.text().contains("Manage running vLLM instances"));
    // The side panel has the same two.
    assert!(present(&app.r, "nav-models") && present(&app.r, "nav-vllm"));
    let tile = find(&app.r, "a", ("id", "tile-vllm"));
    assert_eq!(
        app.r.attr(tile, "href").as_deref(),
        Some("/switchboard/ui/vllm/manage")
    );
}

#[quench_test::test]
fn home_shows_only_what_is_enabled_and_says_so_when_nothing_is() {
    let app = live("/switchboard/ui/home");
    app.send(Msg::Capabilities(caps(true, false, false)));
    assert!(present(&app.r, "tile-models") && !present(&app.r, "tile-vllm"));
    assert!(!present(&app.r, "nav-vllm"));

    let none = live("/switchboard/ui/home");
    none.send(Msg::Capabilities(caps(false, false, false)));
    assert!(present(&none.r, "no-services"));
    assert!(none.text().contains("No services are currently enabled."));
}

#[quench_test::test]
fn an_unknown_path_says_so() {
    let app = live("/switchboard/ui/nope");
    assert!(present(&app.r, "not-found"));
}

#[quench_test::test]
fn the_models_screen_asks_for_the_gpu_and_the_models_and_keeps_asking_for_the_gpu() {
    let app = live("/switchboard/ui/models/dashboard");
    assert_eq!(
        app.asked(),
        vec![
            Request::Startup,
            Request::Gpu,
            Request::Models(default_filters())
        ]
    );
    // Before the GPU answers: "n/a".
    assert!(app.text().contains("GPU: n/a"));
    assert!(app.text().contains("Total: n/a GB"));
    app.send(Msg::Gpu(gpu(32.0, 23.5)));
    assert!(app.text().contains("GPU: card1"));
    assert!(app.text().contains("Total: 32 GB"));
    assert!(app.text().contains("Free: 23.5 GB"));
    tick(1000.0);
    tick(1000.0);
    let gpus = app.asked().iter().filter(|r| **r == Request::Gpu).count();
    assert_eq!(gpus, 3, "{:?}", app.asked());
}

#[quench_test::test]
fn a_model_card_shows_what_the_old_one_did() {
    let app = live("/switchboard/ui/models/dashboard");
    app.send(Msg::Gpu(gpu(32.0, 30.0)));
    let m = model("llama-7b", true, vec![estimate("FP16", 4096, 14.0, 1.0)]);
    app.send(Msg::Models(default_filters(), vec![m]));
    let text = app.text();
    for part in [
        "vLLM",
        "llama-7b",
        "Params: 7.0B",
        "Quant: FP16",
        "Context: ALL",
        "Layers: 32",
        "Hidden: 4096",
        "Fits: YES",
        "Best: 4096 / FP16",
        "VRAM: 15.0 GB",
        "Margin: 15.00 GB",
        "/models/llama-7b",
    ] {
        assert!(text.contains(part), "{part}: {text}");
    }
    assert_eq!(count(&app.r, "fit-ok"), 1);
    // The GPU shrinks: the same card follows (tight, then too big).
    app.send(Msg::Gpu(gpu(32.0, 16.0)));
    assert_eq!(count(&app.r, "fit-warn"), 1);
    app.send(Msg::Gpu(gpu(32.0, 4.0)));
    assert!(app.text().contains("Fits: NO"));
    assert!(app.text().contains("Minimum: 15.0 GB"));
}

#[quench_test::test]
fn a_model_with_no_estimates_says_so_and_a_list_with_none_says_nothing_matches() {
    let app = live("/switchboard/ui/models/dashboard");
    app.send(Msg::Models(
        default_filters(),
        vec![model("bare", false, vec![])],
    ));
    assert!(app.text().contains("No estimates"));
    assert_eq!(count(&app.r, "vllm-badge"), 0);
    app.send(Msg::Models(default_filters(), vec![]));
    assert!(present(&app.r, "no-models"));
}

#[quench_test::test]
fn the_filters_ask_again_and_a_late_answer_for_old_filters_is_ignored() {
    let app = live("/switchboard/ui/models/dashboard");
    app.send(Msg::Models(
        default_filters(),
        vec![model("a", true, vec![])],
    ));
    app.type_into("input", "search", "lla");
    let mut f = default_filters();
    f.search = "lla".into();
    assert_eq!(app.asked().last(), Some(&Request::Models(f.clone())));
    // The answer for the filters before is stale.
    app.send(Msg::Models(
        default_filters(),
        vec![model("stale", true, vec![])],
    ));
    assert!(!app.text().contains("stale"));
    app.send(Msg::Models(f.clone(), vec![model("llama", true, vec![])]));
    assert!(app.text().contains("llama"));

    app.type_into("select", "sort", "vram_desc");
    f.sort = "vram_desc".into();
    assert_eq!(app.asked().last(), Some(&Request::Models(f.clone())));
    app.type_into("select", "context", "8192");
    f.context = "8192".into();
    assert_eq!(app.asked().last(), Some(&Request::Models(f.clone())));
    app.type_into("select", "quant", "AWQ");
    f.quant = "AWQ".into();
    assert_eq!(app.asked().last(), Some(&Request::Models(f.clone())));
    app.click("input", "vllm-only");
    f.vllm_only = true;
    assert_eq!(app.asked().last(), Some(&Request::Models(f.clone())));
    // GGUF has its own quantisations and clears the one chosen.
    assert!(!app.text().contains("q8_0"));
    app.type_into("select", "source", "gguf");
    f.source = "gguf".into();
    f.quant = "ALL".into();
    assert_eq!(app.asked().last(), Some(&Request::Models(f)));
    assert!(app.text().contains("q8_0") && !app.text().contains("awq"));
}

#[quench_test::test]
fn the_estimates_window_filters_its_rows() {
    let app = live("/switchboard/ui/models/dashboard");
    app.send(Msg::Gpu(gpu(32.0, 10.0)));
    let m = model(
        "m",
        true,
        vec![
            estimate("FP16", 4096, 14.0, 1.0),
            estimate("AWQ", 4096, 4.0, 1.0),
            estimate("AWQ", 8192, 4.0, 2.0),
        ],
    );
    app.send(Msg::Models(default_filters(), vec![m]));
    assert!(!present(&app.r, "estimates-modal"));
    app.click_attr("button", "data-estimates", "/models/m");
    assert!(present(&app.r, "estimates-modal"));
    assert!(app.text().contains("Estimates - m"));
    assert_eq!(count(&app.r, "estimate-grid"), 1);
    let rows = |app: &Live| {
        let grid = find(&app.r, "div", ("id", "estimate-grid"));
        app.r.children(grid).len()
    };
    assert_eq!(rows(&app), 3);
    app.type_into("select", "estimates-fit", "fit");
    assert_eq!(rows(&app), 2);
    app.type_into("select", "estimates-fit", "nofit");
    assert_eq!(rows(&app), 1);
    app.type_into("select", "estimates-fit", "all");
    app.type_into("select", "estimates-context", "8192");
    assert_eq!(rows(&app), 1);
    app.type_into("select", "estimates-context", "all");
    app.type_into("select", "estimates-quant", "FP16");
    assert_eq!(rows(&app), 1);
    // Closing.
    app.click("button", "estimates-modal-close");
    assert!(!present(&app.r, "estimates-modal"));
}

#[quench_test::test]
fn deleting_a_model_is_confirmed_and_refreshes_the_list() {
    let app = live("/switchboard/ui/models/dashboard");
    app.send(Msg::Models(
        default_filters(),
        vec![model("m", true, vec![])],
    ));
    // Without the permission there is no button.
    app.send(Msg::Capabilities(caps(true, true, false)));
    assert_eq!(count(&app.r, "card-delete"), 0);
    app.send(Msg::Capabilities(caps(true, true, true)));
    app.send(Msg::Models(
        default_filters(),
        vec![model("m", true, vec![])],
    ));
    assert_eq!(count(&app.r, "card-delete"), 1);
    app.click_attr("button", "data-delete", "/models/m");
    assert!(present(&app.r, "confirm-delete-modal"));
    assert!(app.text().contains("Confirm Delete"));
    assert!(app.text().contains("physically delete"));
    // Cancel leaves it be.
    app.click("button", "delete-cancel");
    assert!(!present(&app.r, "confirm-delete-modal"));
    let before = app.asked().len();
    app.click_attr("button", "data-delete", "/models/m");
    app.click("button", "delete-confirm");
    assert_eq!(
        app.asked().last(),
        Some(&Request::DeleteModel("/models/m".into()))
    );
    assert_eq!(app.asked().len(), before + 1);
    app.send(Msg::ModelDeleted("/models/m".into()));
    assert!(!present(&app.r, "confirm-delete-modal"));
    let asked = app.asked();
    assert!(asked.contains(&Request::Models(default_filters())));
    assert_eq!(asked.last(), Some(&Request::Catalog));
}

#[quench_test::test]
fn a_refused_delete_closes_the_window_and_says_why() {
    let app = live("/switchboard/ui/models/dashboard");
    app.send(Msg::Capabilities(caps(true, true, true)));
    app.send(Msg::Models(
        default_filters(),
        vec![model("m", true, vec![])],
    ));
    app.click_attr("button", "data-delete", "/models/m");
    app.click("button", "delete-confirm");
    app.send(Msg::Rejected("api_error_model_not_found".into()));
    assert!(!present(&app.r, "confirm-delete-modal"));
    assert!(present(&app.r, "notice"));
    assert!(app.text().contains("Model not found on disk"));
}

#[quench_test::test]
fn the_vllm_screen_lists_instances_with_their_details_and_polls() {
    let app = live("/switchboard/ui/vllm/manage");
    assert_eq!(
        app.asked(),
        vec![
            Request::Startup,
            Request::Gpu,
            Request::Instances,
            Request::Catalog
        ]
    );
    app.send(Msg::Capabilities(caps(true, true, true)));
    assert!(present(&app.r, "waiting"));
    let mut failed = instance("vllm-2", "qwen", "failed");
    failed.last_error = Some("out of memory".into());
    failed.log_path = Some("/logs/2.log".into());
    let mut cpu = instance("vllm-3", "tiny", "starting");
    cpu.device = Some("cpu".into());
    app.send(Msg::Instances(vec![
        instance("vllm-1", "llama", "running"),
        failed,
        cpu,
    ]));
    let text = app.text();
    for part in [
        "llama",
        "ID: vllm-1",
        "Namespace: native",
        "Endpoint: 0.0.0.0:8000",
        "Status: running",
        "Started: 2026-10-29 10:01:02",
        "Quant: auto",
        "Dtype: float16",
        "GPU Util: 0.90",
        "out of memory",
        "log: /logs/2.log",
        "Status: failed",
        "Status: starting",
        "Device: CPU",
    ] {
        assert!(text.contains(part), "{part}: {text}");
    }
    assert_eq!(count(&app.r, "badge-cpu"), 1);
    assert_eq!(count(&app.r, "badge-gpu"), 2);
    // A failed instance has nothing to stop.
    assert_eq!(count(&app.r, "card-delete"), 2);
    // Polling: every two seconds for the instances, every second for the GPU.
    tick(2000.0);
    let n = |app: &Live, r: &Request| app.asked().iter().filter(|x| *x == r).count();
    assert_eq!(n(&app, &Request::Instances), 2);
    assert_eq!(n(&app, &Request::Gpu), 3);
}

#[quench_test::test]
fn no_instances_says_so() {
    let app = live("/switchboard/ui/vllm/manage");
    app.send(Msg::Instances(vec![]));
    assert!(present(&app.r, "no-instances"));
    assert!(app.text().contains("No running instances"));
}

#[quench_test::test]
fn stopping_an_instance_is_confirmed() {
    let app = live("/switchboard/ui/vllm/manage");
    app.send(Msg::Capabilities(caps(true, true, true)));
    app.send(Msg::Instances(vec![instance("vllm-1", "llama", "running")]));
    app.click_attr("button", "data-stop", "vllm-1");
    assert!(present(&app.r, "confirm-stop-instance-modal"));
    assert!(
        app.text()
            .contains("Are you sure you want to stop this instance?")
    );
    app.click("button", "stop-cancel");
    assert!(!present(&app.r, "confirm-stop-instance-modal"));
    app.click_attr("button", "data-stop", "vllm-1");
    app.click("button", "stop-confirm");
    assert_eq!(app.asked().last(), Some(&Request::Stop("vllm-1".into())));
    app.send(Msg::Stopped("vllm-1".into()));
    assert!(!present(&app.r, "confirm-stop-instance-modal"));
    assert_eq!(app.asked().last(), Some(&Request::Instances));
}

#[quench_test::test]
fn without_the_permissions_there_is_no_stop_and_no_launch() {
    let app = live("/switchboard/ui/vllm/manage");
    app.send(Msg::Capabilities(caps(true, true, false)));
    app.send(Msg::Instances(vec![instance("vllm-1", "llama", "running")]));
    assert_eq!(count(&app.r, "card-delete"), 0);
    assert!(!present(&app.r, "launch-instance-action"));
}

fn launch_ready() -> Live {
    let app = live("/switchboard/ui/vllm/manage");
    app.send(Msg::Capabilities(caps(true, true, true)));
    app.send(Msg::Gpu(gpu(32.0, 32.0)));
    app.send(Msg::Catalog(vec![
        model("llama", true, vec![estimate("FP16", 4096, 10.0, 2.0)]),
        model("gguf-only", false, vec![]),
        model("big", true, vec![estimate("FP16", 4096, 60.0, 2.0)]),
    ]));
    app.click("button", "launch-instance-action");
    app
}

#[quench_test::test]
fn the_launch_form_opens_with_the_old_defaults_and_waits_for_a_model() {
    let app = launch_ready();
    assert!(present(&app.r, "launch-modal"));
    assert!(app.text().contains("Launch vLLM Instance"));
    assert_eq!(app.value("launch-host"), "0.0.0.0");
    assert_eq!(app.value("launch-port"), "8000");
    assert_eq!(app.value("launch-gpu-util"), "0.90");
    // The native engine has no namespace to choose.
    assert!(!present(&app.r, "launch-namespace"));
    assert!(
        app.text()
            .contains("Select a model to estimate required VRAM.")
    );
    assert!(app.disabled("button", "confirm-launch-btn"));
    // Only the models vLLM can run are offered.
    assert!(app.text().contains("llama") && !app.text().contains("gguf-only"));
    // Launching with no model never reaches the host.
    let before = app.asked().len();
    app.click("button", "confirm-launch-btn");
    assert_eq!(app.asked().len(), before);
}

#[quench_test::test]
fn choosing_a_model_sizes_the_memory_share_and_writes_the_note() {
    let app = launch_ready();
    app.type_into("select", "launch-model", "llama");
    // (10 + 1.5) / (32 - 2) = 0.383 -> 0.40.
    assert_eq!(app.value("launch-gpu-util"), "0.40");
    assert!(!app.disabled("button", "confirm-launch-btn"));
    // 10 + 2 * 0.4 = 10.8 GB for the model; 32 * 0.4 = 12.8 GB reserved; 19.2 left.
    assert!(
        app.text().contains("Should fit: model needs ~10.80 GB"),
        "{}",
        app.text()
    );
    // Typing a share keeps it; changing the length sizes it again.
    app.type_into("input", "launch-gpu-util", "0.20");
    assert_eq!(app.value("launch-gpu-util"), "0.20");
    assert!(app.text().contains("Won't fit"));
    assert!(app.disabled("button", "confirm-launch-btn"));
    app.type_into("input", "launch-max-len", "4096");
    assert_eq!(app.value("launch-gpu-util"), "0.40");
    // A model that is too big for the card cannot be launched.
    app.type_into("select", "launch-model", "big");
    assert!(app.text().contains("Won't fit"));
    assert!(app.disabled("button", "confirm-launch-btn"));
}

#[quench_test::test]
fn cpu_launches_skip_the_gpu_arithmetic() {
    let app = launch_ready();
    app.type_into("select", "launch-model", "big");
    assert!(app.disabled("button", "confirm-launch-btn"));
    app.type_into("select", "launch-device", "cpu");
    assert!(app.disabled("input", "launch-gpu-util"));
    assert!(app.text().contains("Running on CPU"));
    assert!(!app.disabled("button", "confirm-launch-btn"));
    app.click("button", "confirm-launch-btn");
    let Some(Request::Launch(body)) = app.asked().last().cloned() else {
        panic!("{:?}", app.asked());
    };
    assert_eq!(body.device.as_deref(), Some("cpu"));
    assert_eq!(body.gpu_memory_utilization, None);
}

#[quench_test::test]
fn the_form_sends_every_field_and_closes_when_the_service_accepts() {
    let app = launch_ready();
    app.type_into("select", "launch-model", "llama");
    app.type_into("input", "launch-host", "127.0.0.1");
    app.type_into("input", "launch-port", "9001");
    app.type_into("select", "launch-quant", "awq");
    app.type_into("select", "launch-dtype", "float16");
    app.type_into("input", "launch-limit-mm", r#"{"image": 4}"#);
    app.type_into("select", "launch-task", "embed");
    app.click("input", "launch-prefix-caching");
    app.click("input", "launch-enable-tool-calling");
    app.click("button", "confirm-launch-btn");
    let Some(Request::Launch(body)) = app.asked().last().cloned() else {
        panic!("{:?}", app.asked());
    };
    let LaunchBody {
        model,
        host,
        port,
        namespace,
        quantization,
        dtype,
        limit_mm_per_prompt,
        task,
        enable_prefix_caching,
        enable_tool_calling,
        ..
    } = body;
    assert_eq!(
        (model.as_str(), host.as_str(), port),
        ("llama", "127.0.0.1", 9001)
    );
    assert_eq!(namespace.as_deref(), Some("native"));
    assert_eq!(quantization.as_deref(), Some("awq"));
    assert_eq!(dtype.as_deref(), Some("float16"));
    assert_eq!(limit_mm_per_prompt.as_deref(), Some(r#"{"image": 4}"#));
    assert_eq!(task.as_deref(), Some("embed"));
    assert!(enable_prefix_caching && enable_tool_calling);
    // Waiting for the answer: no second click.
    assert!(app.disabled("button", "confirm-launch-btn"));
    app.send(Msg::Launched);
    assert!(!present(&app.r, "launch-modal"));
    assert_eq!(app.asked().last(), Some(&Request::Instances));
}

#[quench_test::test]
fn a_refused_launch_stays_open_and_says_why() {
    let app = launch_ready();
    app.type_into("select", "launch-model", "llama");
    app.click("button", "confirm-launch-btn");
    app.send(Msg::Rejected("api_error_vllm_launch_failed".into()));
    assert!(present(&app.r, "launch-modal"));
    assert!(present(&app.r, "launch-error"));
    assert!(app.text().contains("Failed to launch vLLM instance"));
    assert!(!app.disabled("button", "confirm-launch-btn"));
    app.click("button", "launch-cancel");
    assert!(!present(&app.r, "launch-modal"));
}

#[quench_test::test]
fn the_kubernetes_namespace_can_be_chosen() {
    let app = live("/switchboard/ui/vllm/manage");
    let mut c = caps(true, true, true);
    c.namespace = "ml".into();
    app.send(Msg::Capabilities(c));
    app.click("button", "launch-instance-action");
    assert_eq!(app.value("launch-namespace"), "ml");
    app.type_into("input", "launch-namespace", "other");
    app.type_into("select", "launch-model", "x");
    assert_eq!(app.value("launch-namespace"), "other");
}

#[quench_test::test]
fn a_session_that_ended_offers_to_sign_in_and_a_failure_is_shown() {
    let app = live("/switchboard/ui/models/dashboard");
    app.send(Msg::SignedOut);
    let link = find(&app.r, "a", ("id", "sign-in"));
    assert_eq!(
        app.r.attr(link, "href").as_deref(),
        Some("/switchboard/ui/login")
    );
    assert!(app.text().contains("Your session ended."));
    app.send(Msg::Failed("cannot reach the service".into()));
    assert!(app.text().contains("cannot reach the service"));
    // An answer clears it.
    app.send(Msg::Gpu(gpu(32.0, 30.0)));
    assert!(!present(&app.r, "problem"));
}

#[quench_test::test]
fn the_sidebar_marks_the_open_screen_and_the_links_navigate() {
    let app = live("/switchboard/ui/models/dashboard");
    app.send(Msg::Capabilities(caps(true, true, true)));
    let models = find(&app.r, "a", ("id", "nav-models"));
    assert_eq!(app.r.attr(models, "aria-current").as_deref(), Some("page"));
    let vllm = find(&app.r, "a", ("id", "nav-vllm"));
    assert!(app.r.attr(vllm, "aria-current").is_none());
    app.r.fire(vllm, &Event::Click);
    assert_eq!(app.history.location().path, "/switchboard/ui/vllm/manage");
    assert!(present(&app.r, "vllm-instances-grid") || present(&app.r, "waiting"));
}

#[quench_test::test]
fn the_language_switch_translates_the_screen() {
    let app = live("/switchboard/ui/home");
    app.send(Msg::Capabilities(caps(true, true, true)));
    assert!(app.text().contains("Available Services"));
    app.type_into("select", "language", "de-DE");
    assert!(!app.text().contains("Available Services"), "{}", app.text());
    assert!(app.asked().contains(&Request::Language("de-DE".into())));
}

quench_test::main!();
