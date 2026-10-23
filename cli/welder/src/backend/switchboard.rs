use crate::backend::Backend;
use crate::ui;

/// Backend that talks to an externally managed switchboard-service instead of
/// spawning a process itself.
///
/// switchboard-service owns the vLLM instance lifecycle; welder only ever
/// talks to its HTTP API. `initialize` is a no-op and `is_running` is a plain
/// reachability check.
#[derive(Debug)]
pub struct SwitchboardBackend {
    base_url: String,
}

impl SwitchboardBackend {
    #[must_use]
    pub const fn new(base_url: String) -> Self {
        Self { base_url }
    }
}

#[quench_core::async_trait]
impl Backend for SwitchboardBackend {
    fn initialize(&self) -> quench_core::Result<()> {
        Ok(())
    }

    fn initialized(&self) {
        ui::print_backend_banner("switchboard", &self.base_url);
    }

    fn is_running(&self) -> bool {
        let Ok(url) = quench_proto::Uri::parse(&self.base_url) else {
            return false;
        };
        let Some(host) = url.host() else {
            return false;
        };
        let default_port = if url.scheme() == Some("http") {
            80
        } else {
            443
        };
        let port = url.port_u16().unwrap_or(default_port);
        std::net::TcpStream::connect((host, port)).is_ok()
    }
}
