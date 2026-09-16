use async_trait::async_trait;
use quench_http::prelude::{FromRequest, HttpError, Request};

pub mod check_exists;
pub mod delete_image;
pub mod get_image;
pub mod put_image;

/// The `Accept` header, for manifest content-type negotiation.
pub struct AcceptHeader(pub Option<String>);

#[async_trait]
impl FromRequest for AcceptHeader {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(Self(req.header("accept").map(str::to_string)))
    }
}

pub fn register_routes() {
    check_exists::register_routes();
    delete_image::register_routes();
    get_image::register_routes();
    put_image::register_routes();
}
