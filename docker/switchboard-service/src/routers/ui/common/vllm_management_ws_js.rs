use quench_srv::prelude::with_base_path;

pub fn ensure_vllm_ws_js() {
    let js = vllm_management_ws_js();

    let _ = std::fs::create_dir_all("dist/assets/js");
    let _ = std::fs::write("dist/assets/js/vllm_management_ws.js", js);
}

fn vllm_management_ws_js() -> String {
    let vllm_api_base = with_base_path("/api/v1/vllm");

    let js = r#"
const vllmSocketUrl =
    `${location.protocol === "https:" ? "wss" : "ws"}://` +
    `${location.host}__VLLM_API_BASE__/instances/ws`;

let vllmSocket = null;
let vllmReconnectTimer = null;

function connectVllmSocket() {
    vllmSocket = new WebSocket(vllmSocketUrl);
    vllmSocket.onopen = () => {
        console.log("vLLM websocket connected");
    };
    vllmSocket.onmessage = (event) => {
        try {
            const instances = JSON.parse(event.data);
            window.dispatchEvent(new CustomEvent("vllm-instances-update", { detail: instances }));
        } catch (err) {
            console.error("vLLM websocket parse error", err);
        }
    };
    vllmSocket.onerror = (err) => {
        console.error("vLLM websocket error", err);
    };
    vllmSocket.onclose = () => {
        console.warn("vLLM websocket disconnected");
        if (document.visibilityState === "hidden") return;
        vllmReconnectTimer = setTimeout(connectVllmSocket, 1000);
    };
}

window.addEventListener("beforeunload", () => {
    if (vllmReconnectTimer) {
        clearTimeout(vllmReconnectTimer);
    }
    if (vllmSocket) {
        vllmSocket.close();
    }
});

connectVllmSocket();
    "#
    .to_string();

    js.replace("__VLLM_API_BASE__", &vllm_api_base)
}
