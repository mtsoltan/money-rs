use dotenv::dotenv;
use std::sync::OnceLock;
use std::env::{var,set_var};

pub fn init() {
    dotenv().ok();
    set_var("RUST_LOG", "actix_web=debug");
    database_url();
    jwt_secret();
}

/// "postgresql://money:money@localhost:5432/money"
pub fn database_url() -> &'static str {
    static DATABASE_URL: OnceLock<String> = OnceLock::new();
    DATABASE_URL.get_or_init(|| {
        var("DATABASE_URL").expect("DATABASE_URL must be set")
    })
}

pub fn jwt_secret() -> &'static str {
    static JWT_SECRET: OnceLock<String> = OnceLock::new();
    JWT_SECRET.get_or_init(|| {
        var("JWT_SECRET").expect("JWT_SECRET must be set")
    })
}