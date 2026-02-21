use axum::{Router, response::Html, routing::get};
use quench::prelude::*;
use std::net::SocketAddr;
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {
    let app_shell = AppShellBuilder::new()
        .default_theme(Theme::BootstrapDark)
        .supported_themes(vec![Theme::BootstrapDark, Theme::BootstrapLight])
        .build();

    let index = app_shell.page(
        content()
            .class("main-content")
            .child(div().attr("id", "root").text("Loading...")),
    );

    let about = app_shell.page(
        div()
            .class("page")
            .child(h2().text("About Us"))
            .child(p().text("We are a company that builds amazing SPAs"))
            .child(p().text("Our mission is to make the web better")),
    );

    let contact = app_shell.page(
        div()
            .class("page")
            .child(h2().text("Contact Us"))
            .child(p().text("Email: contact@myapp.com"))
            .child(p().text("Phone: +1 (123) 456-7890")),
    );

    let app = Router::new()
        .nest_service("/assets", ServeDir::new("dist/assets"))
        .route("/", get(Html(index)))
        .route("/about", get(Html(about)))
        .route("/contact", get(Html(contact)))
        .route(
            "/api/data",
            get(Html(r#"{"message": "Hello from API"}"#.to_string())),
        );

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    println!("Server running on http://{}", addr);

    axum::serve(listener, app).await.unwrap();
}
