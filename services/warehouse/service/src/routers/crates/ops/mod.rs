pub mod download;
pub mod publish;
pub mod unyank;
pub mod yank;

pub fn register_routes() {
    download::register_routes();
    publish::register_routes();
    unyank::register_routes();
    yank::register_routes();
}
