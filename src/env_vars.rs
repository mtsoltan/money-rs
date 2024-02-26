use std::env::{set_var, var};
use std::sync::OnceLock;

#[cfg(not(test))]
pub fn init() {
    dotenv::from_filename(".env").ok();
    set_var("RUST_LOG", "actix_web=debug");
    database_url();
    jwt_secret();
}

#[cfg(test)]
pub fn init() {
    dotenv::from_filename("test.env").ok();
    set_var("RUST_LOG", "actix_web=debug");
    database_url();
    jwt_secret();
}

pub fn database_url() -> &'static str {
    static DATABASE_URL: OnceLock<String> = OnceLock::new();
    DATABASE_URL.get_or_init(|| var("DATABASE_URL").expect("DATABASE_URL must be set"))
}

pub fn jwt_secret() -> &'static str {
    static JWT_SECRET: OnceLock<String> = OnceLock::new();
    JWT_SECRET.get_or_init(|| var("JWT_SECRET").expect("JWT_SECRET must be set"))
}

pub fn bind_address() -> &'static str {
    static BIND_ADDRESS: OnceLock<String> = OnceLock::new();
    BIND_ADDRESS.get_or_init(|| {
        format!(
            "{}:{}",
            var("BIND_IP").expect("BIND_IP must be set"),
            var("BIND_PORT").expect("BIND_PORT must be set")
        )
    })
}
