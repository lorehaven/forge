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

#[async_trait::async_trait]
impl Backend for SwitchboardBackend {
    fn initialize(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn initialized(&self) {
        ui::print_backend_banner("switchboard", &self.base_url);
    }

    fn is_running(&self) -> bool {
        let Ok(url) = reqwest::Url::parse(&self.base_url) else {
            return false;
        };
        let Some(host) = url.host_str() else {
            return false;
        };
        let port = url.port_or_known_default().unwrap_or(443);
        std::net::TcpStream::connect((host, port)).is_ok()
    }
}
