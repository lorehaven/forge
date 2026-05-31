use quench_srv::prelude::with_base_path;
use quench_web::prelude::{Script, js};

pub fn vllm_management_ws_script() -> Script {
    let vllm_api_base = with_base_path("/api/v1/vllm");

    let js_code = r#"
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

window.addEventListener("DOMContentLoaded", () => {
    connectVllmSocket();
});
    "#
    .to_string();

    js!(js_code.replace("__VLLM_API_BASE__", &vllm_api_base))
}
