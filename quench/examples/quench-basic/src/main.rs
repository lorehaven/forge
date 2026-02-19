use axum::{Router, response::Html, routing::get};
use quench::html::*;
use quench::{
    AppBuilder, FooterBuilder, HeaderBuilder, NavPanelBuilder, Theme, create_asset_files,
};
use std::net::SocketAddr;
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {
    let nav = NavPanelBuilder::new().build();

    let header = HeaderBuilder::new()
        .label("header_label")
        .with_nav(nav)
        .build();
    let footer = FooterBuilder::new().label("footer_label").build();

    create_asset_files(Theme::Dark);
    let base_app = AppBuilder::new()
        .title("Quench")
        .header(header)
        .footer(footer);

    let index = base_app
        .clone()
        .page_content(
            content()
                .class("main-content")
                .child(div().attr("id", "root").text("Loading...")),
        )
        .build();

    let about = base_app
        .clone()
        .page_content(
            div()
                .class("page")
                .child(h2().text("About Us"))
                .child(p().text("We are a company that builds amazing SPAs"))
                .child(p().text("Our mission is to make the web better")),
        )
        .build();

    let contact = base_app
        .page_content(
            div()
                .class("page")
                .child(h2().text("Contact Us"))
                .child(p().text("Email: contact@myapp.com"))
                .child(p().text("Phone: +1 (123) 456-7890")),
        )
        .build();

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
