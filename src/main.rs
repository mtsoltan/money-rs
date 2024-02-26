#![feature(try_blocks)]
#![feature(let_chains)]

mod authentication;
mod env_vars;
mod handlers;
mod model;
mod schema;

use actix_web::{web, App, HttpServer};
use actix_web_httpauth::middleware::HttpAuthentication;
use diesel::{r2d2::ConnectionManager, PgConnection};
use handlers::login;

pub type Pool = diesel::r2d2::Pool<ConnectionManager<PgConnection>>;

#[derive(Clone)]
pub struct AppState {
    pool: Pool,
}

impl AppState {
    pub fn cpool(&self) -> r2d2::PooledConnection<ConnectionManager<PgConnection>> {
        self.pool.clone().get().expect("Pool should be initialized")
    }
}
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_vars::init();
    HttpServer::new(move || app(&pool()))
        .bind(env_vars::bind_address())?
        .run()
        .await
}

fn pool() -> Pool {
    env_vars::init();
    let manager = ConnectionManager::<PgConnection>::new(env_vars::database_url());
    Pool::builder()
        .build(manager)
        .expect("Failmed to create pool")
}

fn app(
    pool: &Pool,
) -> App<
    impl actix_web::dev::ServiceFactory<
        actix_web::dev::ServiceRequest,
        Config = (),
        Response = actix_web::dev::ServiceResponse<impl actix_web::body::MessageBody>,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    env_vars::init();
    let app = App::new()
        .app_data(web::Data::new(AppState { pool: pool.clone() }))
        .route("/login", web::post().to(login))
        .service(
            web::scope("/api")
                .wrap(HttpAuthentication::bearer(
                    authentication::jwt_validator_generator,
                ))
                .service(
                    web::scope("/currency")
                        .route("", web::post().to(handlers::create_currency))
                        .route("", web::get().to(handlers::get_currencies))
                        // TODO: Also provide the monthly sums for the last 12 months, as well as that sum but normalized by conversion rates to fixed at the time of spending
                        .route("/{name}", web::get().to(handlers::get_currency_by_name))
                        .route("/{name}", web::post().to(handlers::update_currency))
                        .route("/{name}/archive", web::get().to(handlers::archive_currency))
                        // TODO
                        .route("/{name}/entries", web::get().to(handlers::unimplemented)),
                )
                .service(
                    web::scope("/source")
                        .route("", web::post().to(handlers::create_source))
                        .route("", web::get().to(handlers::get_sources))
                        .route("/{name}", web::get().to(handlers::get_source_by_name))
                        .route("/{name}", web::post().to(handlers::update_source))
                        .route("/{name}/archive", web::get().to(handlers::archive_source))
                        // TODO Entries that have this as source 1 or source 2
                        .route("/{name}/entries", web::get().to(handlers::unimplemented)),
                )
                .service(
                    web::scope("/category")
                        .route("", web::post().to(handlers::create_category))
                        .route("", web::get().to(handlers::get_categories))
                        // TODO: Also provide the monthly sums for the last 12 months, as well as that sum but normalized by conversion rates to fixed at the time of spending
                        .route("/{name}", web::get().to(handlers::get_category_by_name))
                        .route("/{name}", web::post().to(handlers::update_category))
                        .route("/{name}/archive", web::get().to(handlers::archive_category))
                        // TODO
                        .route("/{name}/entries", web::get().to(handlers::unimplemented)),
                )
                .service(
                    web::scope("/entry")
                        .route("", web::post().to(handlers::create_entry))
                        // Basic get-all handler, does not return any statistics
                        .route("/all", web::get().to(handlers::get_entries))
                        // - sort (comma separated list of values that fall in amount|source|currency|category|date|created_at|entry_type)
                        // Returns the entries, their sum, their average per month, and their sum-per-category-per-month
                        .route("", web::get().to(handlers::find_entries))
                        // Parameters: ids
                        .route("update", web::post().to(handlers::unimplemented))
                        // Parameters: ids
                        .route("", web::delete().to(handlers::delete_entries))
                        .route("/archive", web::get().to(handlers::archive_entries)),
                ),
        );

        #[cfg(any(test, feature = "create_user"))]
        let app = app.route("/user", web::post().to(handlers::create_user));

        #[cfg(any(test, feature = "create_user"))]
        let app = app.route("/user/{username}", web::delete().to(handlers::delete_user));

    app
}

#[cfg(test)]
mod tests {
    use super::*;

    use actix_web::test as at;
    use serde_json::json;
    use tokio::sync::OnceCell;
    use actix_web::http::StatusCode;

    static TEST_USERNAME: &'static str = "root";
    static TEST_PASSWORD: &'static str = "root";
    static TEST_CURRENCY: &'static str = "USD";

    /// Once lazily:
    /// 1. Deletes the user (in case it already exists from a previous test run).
    /// 2. Creates a user.
    /// 3. Logs in and returns the token.
    /// Sends requests to three endpoints. Asserts that the delete endpoint returned success or not-found,
    /// and that the create and login endpoints returned success.
    /// This is set up this way to allow for setup without having to use something like `#![feature(custom_test_frameworks)]`.
    /// We have it such that teardown, which is only achievable using custom frameworks, is not necessary.
    pub async fn token() -> &'static str {
        async fn once() -> String {
            let app = at::init_service(app(&pool())).await;

            let req = at::TestRequest::delete().uri(format!("/user/{TEST_USERNAME}").as_str())
                .to_request();
            let res = at::call_service(&app, req).await;
            assert!(res.status().is_success() || res.status() == StatusCode::NOT_FOUND);

            let req = at::TestRequest::post().uri("/user")
                .set_json(json!({"username": TEST_USERNAME, "password": TEST_PASSWORD, "currency": TEST_CURRENCY }))
                .to_request();
            let res = at::call_service(&app, req).await;
            assert!(res.status().is_success());

            let req = at::TestRequest::post().uri("/login")
                .set_json(json!({"username": TEST_USERNAME, "password": TEST_PASSWORD }))
                .to_request();
            let res = at::call_service(&app, req).await;
            assert!(res.status().is_success());

            let login_res: crate::handlers::LoginResponse = at::read_body_json(res).await;

            login_res.token
        }

        static TOKEN: OnceCell<String> = OnceCell::const_new();
        TOKEN.get_or_init(once).await.as_str()
    }

    #[actix_web::test]
    async fn register_login() {
        let _ = token().await;
    }

    #[allow(unused,unreachable_code)]
    #[ignore]
    #[actix_web::test]
    async fn unimplemented() {
        let _ = token().await;
        let app = at::init_service(app(&pool())).await;

        let req = at::TestRequest::get().uri(unimplemented!()).to_request();
        let res = at::call_service(&app, req).await;
        assert!(unimplemented!());
    }
}