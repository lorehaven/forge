use quench_srv::prelude::with_base_path;
use quench_web::prelude::{Script, js};

pub fn models_dashboard_ws_script() -> Script {
    let gpu_api_base = with_base_path("/api/v1/gpu");

    let js_code = format!(
        r#"
const socketUrl =
    `${{location.protocol === "https:" ? "wss" : "ws"}}://` +
    `${{location.host}}__GPU_API_BASE__/status/ws`;

let socket = null;
let reconnectTimer = null;

{connect_gpu_socket}
{before_unload}

window.addEventListener("DOMContentLoaded", () => {{
    connectGpuSocket();
}});
    "#,
        connect_gpu_socket = connect_gpu_socket(),
        before_unload = before_unload(),
    );

    js!(js_code.replace("__GPU_API_BASE__", &gpu_api_base))
}

fn connect_gpu_socket() -> String {
    r#"
function connectGpuSocket() {
    socket = new WebSocket(socketUrl);
    socket.onopen = () => {
        console.log("GPU websocket connected");
    };
    socket.onmessage = (event) => {
        try {
            const gpu = JSON.parse(event.data);
            window.dispatchEvent(new CustomEvent("gpu-update", { detail: gpu }));

            const root = document.getElementById("gpu-status");
            if (!root) return;

            const name = root.querySelector(".gpu-name");
            const total = root.querySelector(".gpu-total");
            const free = root.querySelector(".gpu-free");

            if (name) name.textContent = `GPU: ${gpu.name}`;
            if (total) total.textContent = `Total: ${gpu.total_gb} GB`;
            if (free) free.textContent = `Free: ${gpu.free_gb} GB`;
        } catch (err) {
            console.error("GPU websocket parse error", err);
        }
    };
    socket.onerror = (err) => {
        console.error("GPU websocket error", err);
    };
    socket.onclose = () => {
        console.warn("GPU websocket disconnected");
        if (document.visibilityState === "hidden") return;
        reconnectTimer = setTimeout(connectGpuSocket, 1000);
    };
}
"#
    .to_string()
}

fn before_unload() -> String {
    r#"
window.addEventListener("beforeunload", () => {
    if (reconnectTimer) {
        clearTimeout(reconnectTimer);
    }
    if (socket) {
        socket.close();
    }
});
"#
    .to_string()
}
