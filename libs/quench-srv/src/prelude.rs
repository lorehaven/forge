pub use crate::actix::domain::auth::{Role, User, UserDb};
pub use crate::actix::domain::db::DbWrapper;
pub use crate::actix::domain::error;
pub use crate::actix::domain::jwt::{Claims, JwtConfig};
pub use crate::actix::domain::session::{Session, SessionDb};
pub mod jwt {
    pub use crate::actix::domain::jwt::*;
}
pub mod routers {
    pub use crate::actix::routers::*;
}
pub use crate::actix::routers::health::HealthState;
pub use crate::actix::serve;
pub use crate::common::routes::{normalize_base_path, with_base_path};
pub use crate::common::wait::wait_for_services;

pub use actix_web::dev::HttpServiceFactory;
