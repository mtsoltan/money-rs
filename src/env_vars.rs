use std::env::var;
use std::sync::OnceLock;

use env_logger::Env;

#[cfg(not(test))]
pub fn init() {
    dotenv::from_filename(".env").ok();
    init_logger();
    database_url();
    jwt_secret();
}

#[cfg(test)]
pub fn init() {
    dotenv::from_filename("test.env").ok();
    init_logger();
    database_url();
    jwt_secret();
}

pub fn database_url() -> &'static str {
    static DATABASE_URL: OnceLock<String> = OnceLock::new();
    DATABASE_URL.get_or_init(|| var("DATABASE_URL").expect("X004: DATABASE_URL must be set"))
}

pub fn jwt_secret() -> &'static str {
    static JWT_SECRET: OnceLock<String> = OnceLock::new();
    JWT_SECRET.get_or_init(|| var("JWT_SECRET").expect("X004: JWT_SECRET must be set"))
}

pub fn bind_address() -> &'static str {
    static BIND_ADDRESS: OnceLock<String> = OnceLock::new();
    BIND_ADDRESS.get_or_init(|| {
        format!(
            "{}:{}",
            var("BIND_IP").expect("X004: BIND_IP must be set"),
            var("BIND_PORT").expect("X004: BIND_PORT must be set")
        )
    })
}

pub fn init_logger() {
    static LOGGER_INIT: OnceLock<()> = OnceLock::new();
    LOGGER_INIT.get_or_init(|| {
        let env = Env::new().default_filter_or("info");
        env_logger::Builder::from_env(env)
            .target(env_logger::Target::Stdout)
            .format_timestamp_micros()
            .init();
    });
}

pub fn page_size() -> u32 {
    static PAGE_SIZE: OnceLock<String> = OnceLock::new();
    let page_size_str =
        PAGE_SIZE.get_or_init(|| var("PAGE_SIZE").expect("X004: PAGE_SIZE must be set"));
    page_size_str.parse::<u32>().expect("X004: PAGE_SIZE must be parsable into u32")
}

pub fn app_dist_dir() -> &'static std::path::PathBuf {
    static APP_DIST: OnceLock<std::path::PathBuf> = OnceLock::new();
    APP_DIST.get_or_init(|| {
        var("APP_DIST")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("frontend/dist"))
    })
}
