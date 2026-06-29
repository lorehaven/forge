use crate::routers::ui::common::{UiPageKind, render_page};
use actix_web::{HttpResponse, Responder, get, post, web};
use quench_auth::actix::routers::ui::pages::auth::{
    LoginForm, LoginQuery, handle_login_submit, handle_logout, login_form_element,
};
use quench_auth::prelude::{JwtConfig, SessionDb, UserDb};
use quench_web::prelude::*;

#[get("/login")]
pub(super) async fn login(query: web::Query<LoginQuery>) -> impl Responder {
    render_login_page(query.err.as_deref() == Some("1"))
}

#[get("/login/")]
pub(super) async fn login_slash(query: web::Query<LoginQuery>) -> impl Responder {
    render_login_page(query.err.as_deref() == Some("1"))
}

#[post("/login")]
pub(super) async fn login_submit(
    form: web::Form<LoginForm>,
    config: web::Data<JwtConfig>,
    user_db: web::Data<std::sync::Arc<UserDb>>,
    session_db: web::Data<std::sync::Arc<SessionDb>>,
) -> impl Responder {
    handle_login_submit(form, config, user_db, session_db).await
}

#[get("/logout")]
pub(super) async fn logout(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    session_db: web::Data<std::sync::Arc<SessionDb>>,
) -> impl Responder {
    handle_logout(req, config, session_db).await
}

fn render_login_page(error: bool) -> HttpResponse {
    let login_form = login_form_element(error);

    render_page(
        HttpResponse::Ok(),
        content().class("container-fluid login-layout").child(
            div()
                .class("panel login-panel")
                .child(
                    div()
                        .class("panel-title")
                        .attr("data-i18n", "ui_login_sign_in"),
                )
                .child(div().class("meta-list").child(login_form)),
        ),
        UiPageKind::Auth,
    )
}
