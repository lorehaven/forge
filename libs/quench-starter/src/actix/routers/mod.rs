pub mod health;
pub mod metrics;
pub mod swagger;
pub mod ui;

// Re-export JwtConfig for use in UI routers
pub use quench_auth::actix::domain::jwt::JwtConfig;
