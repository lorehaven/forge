use actix_web::dev::HttpServiceFactory;
use actix_web::web;

pub mod chat;
pub mod ui;

pub fn root_scope() -> impl HttpServiceFactory {
    web::scope("").service(ui::assets)
}

pub fn base_path_scope() -> impl HttpServiceFactory {
    web::scope("").service(ui::scope()).service(chat::scope())
}
