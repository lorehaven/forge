//! Serves a service's browser client: the page that starts a `quench-ui` module, and the files it loads.
//!
//! A service declares four small routes and hands each to a [`Client`]:
//!
//! ```ignore
//! static CLIENT: forge_web::Client = forge_web::Client::new("Gauge", gauge_web::stylesheet);
//!
//! #[get("/ui/home")]
//! async fn home(_who: ActorOrRedirect) -> Response { CLIENT.page() } // the service's own session check stays
//! #[get("/ui/assets/quench.js")]
//! async fn script() -> Response { CLIENT.script() }
//! #[get("/ui/assets/app.css")]
//! async fn style() -> Response { CLIENT.style() }
//! #[get("/ui/assets/app.wasm")]
//! async fn module() -> Response { CLIENT.module() }
//! ```
//!
//! The paths of the three files are fixed ([`SCRIPT_PATH`], [`STYLE_PATH`], [`MODULE_PATH`]), under the service's base
//! path. The module is built apart (`cargo build --target wasm32-unknown-unknown --profile wasm -p <name>-web`) and put in
//! `WEB_DIR` (`web` by default) as `app.wasm`; the image build does this for a service that asks for it (`WASM_PACKAGE`).
//! Without the module the page says the client is not installed.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use quench_fullstack::{Page, content_security_policy, new_nonce, render_document};
use quench_http::prelude::Response;
use quench_http::prelude::http::StatusCode;
use quench_starter::common::routes::with_base_path;
use quench_ui::View;

/// Where the loader script is, under the service's base path.
pub const SCRIPT_PATH: &str = "/ui/assets/quench.js";
/// Where the stylesheet is.
pub const STYLE_PATH: &str = "/ui/assets/app.css";
/// Where the wasm module is.
pub const MODULE_PATH: &str = "/ui/assets/app.wasm";

/// Where the shell script is.
pub const SHELL_PATH: &str = "/ui/assets/shell.js";
/// Where the service's icon is.
pub const ICON_PATH: &str = "/ui/assets/favicon.png";

/// The directory the module is read from: `WEB_DIR`, else `web` (relative to the service's working directory).
#[must_use]
pub fn web_dir() -> PathBuf {
    PathBuf::from(quench_core::env::get_or("WEB_DIR", "web"))
}

/// One browser client.
#[derive(Debug, Clone, Copy)]
pub struct Client {
    title: &'static str,
    stylesheet: fn() -> String,
    /// The service's own icon (a PNG), the tab's and the home screen's.
    icon: &'static [u8],
    /// Whether the page has forge's frame (its stylesheet and script), as the services' own clients do.
    framed: bool,
    /// Where the client's files are, under the service's base path: `/ui` unless the service has several clients.
    prefix: &'static str,
    /// The file in `WEB_DIR` the module is read from, and served as.
    module: &'static str,
    /// A script of the client's own, `(file name, text)`, loaded by the page after the module's loader.
    script: Option<(&'static str, &'static str)>,
    /// The client plays audio from `blob:` addresses (songs kept on the device).
    media: bool,
}

impl Client {
    /// A client called `title` (the page's title), whose stylesheet `stylesheet` makes (`<name>_web::stylesheet`).
    #[must_use]
    pub const fn new(title: &'static str, stylesheet: fn() -> String) -> Self {
        Self {
            title,
            stylesheet,
            icon: &[],
            framed: false,
            prefix: "/ui",
            module: "app.wasm",
            script: None,
            media: false,
        }
    }

    /// The module is the file `name` of `WEB_DIR` (default `app.wasm`), for a service with more than one client.
    #[must_use]
    pub const fn module_file(mut self, name: &'static str) -> Self {
        self.module = name;
        self
    }

    /// A script of the client's own, served at `{prefix}/assets/{name}` and loaded by the page.
    #[must_use]
    pub const fn with_script(mut self, name: &'static str, text: &'static str) -> Self {
        self.script = Some((name, text));
        self
    }

    /// The client plays audio, also from `blob:` addresses: the page's policy allows it.
    #[must_use]
    pub const fn media(mut self) -> Self {
        self.media = true;
        self
    }

    /// The client's own script, or `404` when it has none.
    #[must_use]
    pub fn own_script(&self) -> Response {
        self.script.map_or_else(Response::not_found, |(_, text)| {
            asset(text, "text/javascript; charset=utf-8")
        })
    }

    /// A service with more than one client gives each its own place: the files of this one are `{prefix}/assets/...`
    /// (the default is `/ui`).
    #[must_use]
    pub const fn at(mut self, prefix: &'static str) -> Self {
        self.prefix = prefix;
        self
    }

    /// Where one of this client's files is, under the service's base path: `name` is `app.css`, `quench.js`, ...
    #[must_use]
    pub fn asset_path(&self, name: &str) -> String {
        with_base_path(&format!("{}/assets/{name}", self.prefix))
    }

    /// The page wears forge's frame: `forge-shell`'s stylesheet and script come with it (the client draws the top bar).
    #[must_use]
    pub const fn framed(mut self) -> Self {
        self.framed = true;
        self
    }

    /// The service's icon: a PNG, `include_bytes!`d where the service keeps it.
    #[must_use]
    pub const fn icon(mut self, png: &'static [u8]) -> Self {
        self.icon = png;
        self
    }

    /// The page: an empty root, the stylesheet, and the script that loads the module.
    #[must_use]
    pub fn page(&self) -> Response {
        self.page_from(&web_dir())
    }

    /// The loader script (`quench.js`, the one of the `quench-ui-web` this was built with).
    #[must_use]
    pub fn script(&self) -> Response {
        asset(quench_ui_web::QUENCH_JS, "text/javascript; charset=utf-8")
    }

    /// The stylesheet: forge's (tokens, components, the top bar), then the client's own.
    #[must_use]
    pub fn style(&self) -> Response {
        let own = (self.stylesheet)();
        asset(
            if self.framed {
                format!("{}\n{own}", forge_shell::STYLE)
            } else {
                own
            },
            "text/css; charset=utf-8",
        )
    }

    /// The script that keeps theme, language and gatehouse links right (`forge-shell`'s).
    #[must_use]
    pub fn shell(&self) -> Response {
        asset(forge_shell::SCRIPT, "text/javascript; charset=utf-8")
    }

    /// The service's icon, or `404` when it has none.
    #[must_use]
    pub fn favicon(&self) -> Response {
        if self.icon.is_empty() {
            return Response::not_found();
        }
        asset(self.icon, "image/png")
    }

    /// The wasm module, or `404` when it is not installed.
    #[must_use]
    pub fn module(&self) -> Response {
        self.module_from(&web_dir())
    }

    /// [`page`](Self::page) with the module looked for in `dir`.
    #[must_use]
    pub fn page_from(&self, dir: &Path) -> Response {
        if !dir.join(self.module).is_file() {
            return Response::text(
                StatusCode::NOT_FOUND,
                "the browser client is not installed on this server",
            );
        }
        let nonce = new_nonce();
        let html = self.with_head(&render_document(
            Page::new(self.title, View::Empty).stylesheet(&self.asset_path("app.css")),
            &nonce,
            &self.asset_path("quench.js"),
            &self.asset_path(self.module),
        ));
        Response::html(StatusCode::OK, html)
            .header("cache-control", "no-store")
            .header("content-security-policy", self.policy(&nonce))
            .header("referrer-policy", "same-origin")
    }

    /// The page's content policy: the framework's, and `blob:` for audio when the client plays it.
    fn policy(&self, nonce: &str) -> String {
        let policy = content_security_policy(nonce);
        if self.media {
            policy.replacen(
                "default-src 'self';",
                "default-src 'self'; media-src 'self' blob:;",
                1,
            )
        } else {
            policy
        }
    }

    /// The icon and the shell script in the page's head. The script is a file of this origin, so the policy allows it.
    fn with_head(&self, html: &str) -> String {
        let mut tags = String::new();
        if !self.icon.is_empty() {
            let _ = write!(
                tags,
                "<link rel=\"icon\" type=\"image/png\" href=\"{}\">",
                self.asset_path("favicon.png")
            );
        }
        if self.framed {
            let gatehouse = quench_core::env::get_or("GATEHOUSE_URL", "");
            let _ = write!(
                tags,
                "<script src=\"{}\" data-gatehouse=\"{}\"></script>",
                self.asset_path("shell.js"),
                quench_ui::html::escape_attr(gatehouse.trim_end_matches('/')),
            );
        }
        if let Some((name, _)) = self.script {
            let _ = write!(tags, "<script src=\"{}\"></script>", self.asset_path(name));
        }
        html.replacen("</head>", &format!("{tags}</head>"), 1)
    }

    /// [`module`](Self::module) with the module looked for in `dir`.
    #[must_use]
    pub fn module_from(&self, dir: &Path) -> Response {
        std::fs::read(dir.join(self.module)).map_or_else(
            |_| Response::not_found(),
            |bytes| asset(bytes, "application/wasm"),
        )
    }
}

fn asset(body: impl Into<Vec<u8>>, content_type: &str) -> Response {
    Response::from_bytes(StatusCode::OK, body.into().into())
        .header("content-type", content_type)
        .header("cache-control", "no-cache")
}
