//! The requests the service answers. All of them are `GET`s, none needs a login.
//!
//! ```text
//! /                       the list of books
//! /<book>/...             the newest release of a book
//! /api/v1/books           the same list as JSON
//! /health, /health/live, /health/ready
//! ```

use crate::book::{File, Found};
use crate::library::Library;
use crate::ui;
use quench_core::async_trait;
use quench_core::bytes::Bytes;
use quench_http::prelude::http::{Method, StatusCode};
use quench_http::prelude::{Endpoint, Request, Response};
use quench_serde::json;
use std::sync::Arc;

/// What a route does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Handler {
    Index,
    Books,
    Health,
    Book,
    Page,
    Style,
    ShellScript,
    Script,
    FrameScript,
    Icon,
}

/// One route of the table below.
#[derive(Debug, Clone)]
pub struct Route {
    pub method: Method,
    pub pattern: &'static str,
    handler: Handler,
}

/// Every route, most specific first. `{name}` is one non-empty segment; `{path}` takes the rest of the path, slashes
/// and all, and may be empty.
pub const ROUTES: [Route; 12] = [
    Route {
        method: Method::GET,
        pattern: "/",
        handler: Handler::Index,
    },
    Route {
        method: Method::GET,
        pattern: "/api/v1/books",
        handler: Handler::Books,
    },
    Route {
        method: Method::GET,
        pattern: "/health",
        handler: Handler::Health,
    },
    Route {
        method: Method::GET,
        pattern: "/health/live",
        handler: Handler::Health,
    },
    Route {
        method: Method::GET,
        pattern: "/health/ready",
        handler: Handler::Health,
    },
    Route {
        method: Method::GET,
        pattern: "/ui/assets/style.css",
        handler: Handler::Style,
    },
    Route {
        method: Method::GET,
        pattern: "/ui/assets/shell.js",
        handler: Handler::ShellScript,
    },
    Route {
        method: Method::GET,
        pattern: "/ui/assets/lectern.js",
        handler: Handler::Script,
    },
    Route {
        method: Method::GET,
        pattern: "/ui/assets/frame.js",
        handler: Handler::FrameScript,
    },
    Route {
        method: Method::GET,
        pattern: "/ui/assets/favicon.png",
        handler: Handler::Icon,
    },
    Route {
        method: Method::GET,
        pattern: "/{book}",
        handler: Handler::Book,
    },
    Route {
        method: Method::GET,
        pattern: "/{book}/{path}",
        handler: Handler::Page,
    },
];

/// Whether `path` (with its leading slash) is what `pattern` describes.
#[must_use]
pub fn matches(pattern: &str, path: &str) -> bool {
    let mut wanted = pattern.split('/');
    let mut got = path.splitn(pattern.matches('/').count() + 1, '/');
    loop {
        match (wanted.next(), got.next()) {
            (None, None) | (Some("{path}"), Some(_)) => return true,
            (Some(want), Some(have)) if want.starts_with('{') => {
                if have.is_empty() || have.contains('/') {
                    return false;
                }
            }
            (Some(want), Some(have)) => {
                if want != have {
                    return false;
                }
            }
            _ => return false,
        }
    }
}

/// What the front page may load: its own files, and the pictures its stylesheet draws inline.
const PAGE_POLICY: &str = "default-src 'none'; style-src 'self'; script-src 'self'; img-src 'self' data:; \
base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// What a framed page may load: its own files, and the book in the frame (the same origin).
const FRAME_POLICY: &str = "default-src 'none'; style-src 'self'; script-src 'self'; img-src 'self' data:; \
frame-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// The service's icon, the tab's and the top bar's.
const ICON: &[u8] = include_bytes!("../../icon.png");

#[derive(Debug)]
pub struct Site {
    library: Arc<Library>,
    /// `/` or a path with a leading slash and none trailing.
    base_path: String,
}

impl Site {
    #[must_use]
    pub fn new(library: Arc<Library>, base_path: &str) -> Self {
        Self {
            library,
            base_path: base_path.trim_end_matches('/').to_string(),
        }
    }

    fn redirect(&self, to: &str) -> Response {
        Response::new(StatusCode::MOVED_PERMANENTLY)
            .header("location", format!("{}{to}", self.base_path))
    }

    fn index(&self, request: &Request) -> Response {
        let page = ui::front_page(
            &self.base_path,
            &self.library.list(),
            request.header("cookie"),
            request.header("accept-language"),
        );
        Response::html(StatusCode::OK, page)
            .header("cache-control", "no-cache")
            .header("content-security-policy", PAGE_POLICY)
            .header("referrer-policy", "same-origin")
    }

    /// One of the page's own files.
    fn asset(content: impl Into<Vec<u8>>, content_type: &str) -> Response {
        Response::from_bytes(StatusCode::OK, Bytes::from(content.into()))
            .header("content-type", content_type)
            .header("cache-control", "no-cache")
    }

    fn books(&self) -> Response {
        let books: Vec<_> = self
            .library
            .list()
            .iter()
            .map(|book| {
                json!({
                    "name": book.name.as_str(),
                    "title": book.title.as_str(),
                    "version": book.version.as_str(),
                    "built": book.built.as_str(),
                    "files": book.file_count(),
                    "url": format!("{}/{}/", self.base_path, book.name),
                })
            })
            .collect();
        Response::json(StatusCode::OK, &books)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
            .header("cache-control", "no-cache")
    }

    fn health(&self, rest: &str) -> Response {
        match rest {
            "/health" | "/health/live" => Response::text(StatusCode::OK, "OK"),
            _ if self.library.is_ready() => {
                Response::json(StatusCode::OK, &json!({ "status": "ready" }))
                    .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
            }
            _ => Response::json(
                StatusCode::SERVICE_UNAVAILABLE,
                &json!({ "status": "not_ready" }),
            )
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        }
    }

    /// `/<book>` is `/<book>/`, if there is such a book.
    fn book(&self, rest: &str) -> Response {
        let name = rest.trim_start_matches('/');
        if self
            .library
            .get(&quench_serde::urlencoded::percent_decode(name))
            .is_none()
        {
            return Response::text(StatusCode::NOT_FOUND, "no such book");
        }
        self.redirect(&format!("/{name}/"))
    }

    fn page(&self, request: &Request, rest: &str) -> Response {
        let (name, path) = rest
            .trim_start_matches('/')
            .split_once('/')
            .unwrap_or_else(|| (rest.trim_start_matches('/'), ""));
        let Some(book) = self
            .library
            .get(&quench_serde::urlencoded::percent_decode(name))
        else {
            return Response::text(StatusCode::NOT_FOUND, "no such book");
        };
        let path = quench_serde::urlencoded::percent_decode(path);
        let found = match book.find(&path) {
            Found::File(file) => file,
            Found::Slash => return self.redirect(&format!("/{name}/{path}/")),
            Found::Missing => {
                return book.not_found_page().map_or_else(
                    || Response::text(StatusCode::NOT_FOUND, "not found"),
                    |page| respond(StatusCode::NOT_FOUND, page, None),
                );
            }
        };

        // A page opened in the browser's own window is wrapped in Lectern's frame; the same address asked for by the
        // frame, or by anything that is not a navigation, is the page itself. Only browsers that say what a request
        // is for (`Sec-Fetch-Dest`, sent to https and localhost) get the frame.
        let framed = found.content_type.starts_with("text/html")
            && request.header("sec-fetch-dest") == Some("document");

        // Within a release nothing changes, so its version is the validator of every file in it, once for each form.
        let etag = format!(
            "W/\"{}{}\"",
            book.version,
            if framed { "-framed" } else { "" }
        );
        if request
            .header("if-none-match")
            .is_some_and(|sent| sent.split(',').any(|tag| tag.trim() == etag))
        {
            return Response::new(StatusCode::NOT_MODIFIED)
                .header("etag", etag)
                .header("vary", "Sec-Fetch-Dest");
        }
        if framed {
            let mut src = request.uri().path().to_string();
            if let Some(query) = request.uri().query() {
                src.push('?');
                src.push_str(query);
            }
            let page = ui::framed_page(
                &self.base_path,
                &self.library.list(),
                &book,
                &src,
                request.header("cookie"),
                request.header("accept-language"),
            );
            return Response::html(StatusCode::OK, page)
                .header("cache-control", "no-cache")
                .header("etag", etag)
                .header("vary", "Sec-Fetch-Dest")
                .header("content-security-policy", FRAME_POLICY)
                .header("referrer-policy", "same-origin");
        }
        respond(StatusCode::OK, found, Some(&etag))
    }
}

fn respond(status: StatusCode, file: &File, etag: Option<&str>) -> Response {
    // Every file is revalidated against the release version (a `304` when nothing changed), so a new release never
    // meets a stylesheet or script cached from the last one under the same address.
    let mut response = Response::from_bytes(status, Bytes::clone(&file.body))
        .header("content-type", file.content_type)
        .header("cache-control", "no-cache");
    if let Some(etag) = etag {
        response = response.header("etag", etag);
    }
    if file.content_type.starts_with("text/html") {
        // The same address is the frame for a navigation and the page for the frame.
        response = response.header("vary", "Sec-Fetch-Dest");
    }
    response
}

#[async_trait]
impl Endpoint for Site {
    async fn call(&self, request: Request) -> Response {
        if *request.method() != Method::GET && *request.method() != Method::HEAD {
            return Response::new(StatusCode::METHOD_NOT_ALLOWED).header("allow", "GET, HEAD");
        }

        let path = request.uri().path().to_string();
        let rest = if self.base_path.is_empty() {
            path.as_str()
        } else if path == self.base_path {
            return self.redirect("/");
        } else {
            match path.strip_prefix(&self.base_path) {
                Some(rest) if rest.starts_with('/') => rest,
                _ => return Response::text(StatusCode::NOT_FOUND, "not found"),
            }
        };

        let Some(route) = ROUTES.iter().find(|route| matches(route.pattern, rest)) else {
            return Response::text(StatusCode::NOT_FOUND, "not found");
        };
        match route.handler {
            Handler::Index => self.index(&request),
            Handler::Books => self.books(),
            Handler::Health => self.health(rest),
            Handler::Book => self.book(rest),
            Handler::Page => self.page(&request, rest),
            Handler::Style => Self::asset(ui::style(), "text/css; charset=utf-8"),
            Handler::ShellScript => Self::asset(ui::SHELL_SCRIPT, "text/javascript; charset=utf-8"),
            Handler::Script => Self::asset(ui::SCRIPT, "text/javascript; charset=utf-8"),
            Handler::FrameScript => Self::asset(ui::FRAME_SCRIPT, "text/javascript; charset=utf-8"),
            Handler::Icon => Self::asset(ICON, "image/png"),
        }
    }
}
