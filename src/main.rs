#![feature(try_blocks)]
#![feature(let_chains)]
#![feature(type_alias_impl_trait)]
#![feature(trait_alias)]
#![feature(stmt_expr_attributes)]

mod authentication;
mod env_vars;
mod handlers;
mod model;
mod schema;

use actix_web::{web, App, HttpServer};
use actix_web_httpauth::middleware::HttpAuthentication;
use diesel::r2d2::ConnectionManager;
use diesel::PgConnection;
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
    HttpServer::new(move || app(&pool())).bind(env_vars::bind_address())?.run().await
}

fn pool() -> Pool {
    env_vars::init();
    let manager = ConnectionManager::<PgConnection>::new(env_vars::database_url());
    Pool::builder().build(manager).expect("Failed to create pool")
}

fn app(
    pool: &Pool,
) -> App<
    impl actix_web::dev::ServiceFactory<
        actix_web::dev::ServiceRequest,
        Response = actix_web::dev::ServiceResponse<impl actix_web::body::MessageBody>,
        Error = actix_web::Error,
        Config = (),
        InitError = (),
    >,
> {
    env_vars::init();
    let app = App::new()
        .wrap(actix_web::middleware::Logger::new("%T %a %s %r %b").log_target("actix_web"))
        .app_data(web::Data::new(AppState { pool: pool.clone() }))
        .route("/login", web::post().to(login))
        .service(
            web::scope("/api")
                .wrap(HttpAuthentication::bearer(authentication::jwt_validator_generator))
                .service(
                    web::scope("/currency")
                        .route("", web::post().to(handlers::create_currency))
                        .route("", web::get().to(handlers::get_currencies))
                        // TODO: Also provide the monthly sums for the last 12 months, as well as
                        // that sum but normalized by conversion rates to
                        // fixed at the time of spending
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
                        // TODO: Also provide the monthly sums for the last 12 months, as well as
                        // that sum but normalized by conversion rates to
                        // fixed at the time of spending
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
                        // - sort (comma separated list of values that fall in
                        //   amount|source|currency|category|date|created_at|entry_type)
                        // Returns the entries, their sum, their average per month, and their
                        // sum-per-category-per-month
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
    use std::fmt::Debug;
    use std::pin::Pin;

    use actix_http::body::MessageBody;
    use actix_http::error::PayloadError;
    use actix_http::Request;
    use actix_web::dev::ServiceResponse;
    use actix_web::http::{Method, StatusCode};
    use actix_web::test as at;
    use diesel::prelude::*;
    use serde::de::DeserializeOwned;
    use serde::Serialize;
    use serde_json::json;
    use tokio::sync::OnceCell;

    use super::*;
    use crate::handlers::{CreateResponse, EmptyResponse, LoginResponse};
    use crate::model::CurrencyResponse;

    static TEST_USERNAME: &'static str = "root";
    static TEST_PASSWORD: &'static str = "root";
    static TEST_CURRENCY: &'static str = "USD";

    struct TestResponse<T: DeserializeOwned + Serialize> {
        pub status_code: StatusCode,
        pub body_string: String,
        pub body: Option<T>,
    }

    fn assert_response_status<T: DeserializeOwned + Serialize>(
        res: &TestResponse<T>,
        status: StatusCode,
    ) {
        assert_eq!(
            res.status_code,
            status,
            "expected {}, found {}: {}",
            status.as_str(),
            res.status_code.as_str(),
            res.body_string
        );
    }

    fn assert_response_status_is_success<T: DeserializeOwned + Serialize>(res: &TestResponse<T>) {
        assert!(
            res.status_code.is_success(),
            "expected 200~300, found {}: {}",
            res.status_code.as_str(),
            res.body_string
        );
    }

    async fn run_req<T, B, S>(
        app: &S,
        method: Method,
        uri: &str,
        token: Option<&str>,
        request_body: Option<serde_json::Value>,
    ) -> TestResponse<T>
    where
        T: DeserializeOwned + Serialize + Debug,
        B: MessageBody,
        S: actix_web::dev::Service<
            Request<Pin<Box<dyn futures::Stream<Item = Result<web::Bytes, PayloadError>>>>>,
            Response = ServiceResponse<B>,
            Error = actix_web::Error,
        >,
    {
        let mut req = at::TestRequest::default().method(method).uri(uri);
        if let Some(t) = token {
            req = req.append_header(("Authorization", format!("Bearer {}", t)));
        }
        if let Some(body) = request_body {
            req = req.set_json(body);
        }

        let req = req.to_request();
        let res = at::call_service(&app, req).await;
        let status_code = res.status();
        unsafe {
            match res.status() {
                StatusCode::OK => {
                    let res_struct = at::read_body_json::<T, B>(res).await;
                    TestResponse {
                        status_code,
                        body_string: serde_json::to_string(&res_struct).expect(
                            "Serializing returned error code 200 json body should always
                succeed",
                        ),
                        body: Some(dbg!(res_struct)),
                    }
                }
                _ => {
                    let res_bytes = at::read_body(res).await;
                    let res_body_string = std::str::from_utf8_unchecked(&res_bytes).to_owned();
                    TestResponse { status_code, body_string: res_body_string, body: None }
                }
            }
        }
    }

    /// Once lazily:
    /// 1. Deletes the user (in case it already exists from a previous test run).
    /// 2. Creates a user.
    /// 3. Logs in and returns the token.
    /// Sends requests to three endpoints. Asserts that the delete endpoint returned success or
    /// not-found, and that the create and login endpoints returned success. This is set up this way
    /// to allow for setup without having to use something like
    /// `#![feature(custom_test_frameworks)]`. We have it such that teardown, which is only
    /// achievable using custom frameworks, is not necessary.
    async fn token() -> &'static str {
        async fn once() -> String {
            let app = at::init_service(app(&pool())).await;

            let res: TestResponse<EmptyResponse> = run_req(
                &app,
                Method::DELETE,
                format!("/user/{TEST_USERNAME}").as_str(),
                None,
                None,
            )
            .await;
            assert!(res.status_code.is_success() || res.status_code == StatusCode::NOT_FOUND);

            let res: TestResponse<CreateResponse> = run_req(
                &app,
                Method::POST,
                "/user",
                None,
                Some(json!({"username": TEST_USERNAME, "password": TEST_PASSWORD, "currency": TEST_CURRENCY })),
            )
            .await;
            assert_response_status_is_success(&res);

            let res: TestResponse<LoginResponse> = run_req(
                &app,
                Method::POST,
                "/login",
                None,
                Some(json!({"username": TEST_USERNAME, "password": TEST_PASSWORD })),
            )
            .await;
            assert_response_status_is_success(&res);

            res.body.expect("expected body to be set on 200").token
        }

        static TOKEN: OnceCell<String> = OnceCell::const_new();
        TOKEN.get_or_init(once).await.as_str()
    }

    fn delete_direct<T>(pool: &Pool, q: T)
    where
        T: diesel::query_builder::IntoUpdateTarget,
        <T as diesel::associations::HasTable>::Table: diesel::query_builder::QueryId + 'static,
        <T as diesel::query_builder::IntoUpdateTarget>::WhereClause:
            diesel::query_builder::QueryId + diesel::query_builder::QueryFragment<diesel::pg::Pg>,
        <<T as diesel::associations::HasTable>::Table as QuerySource>::FromClause:
            diesel::query_builder::QueryFragment<diesel::pg::Pg>,
    {
        let mut conn = pool.get().expect("Failed to get database connection");
        diesel::delete(q).execute(&mut conn).expect("Failed to run the delete query you specified");
    }

    #[actix_web::test]
    async fn register_login() { let _ = token().await; }

    #[actix_web::test]
    async fn test_currency_lifecycle() {
        let t = Some(token().await);
        let app = at::init_service(app(&pool())).await;

        // Create currency
        let res: TestResponse<CreateResponse> = run_req(
            &app,
            Method::POST,
            "/api/currency",
            t,
            Some(json!({"name": "EUR", "rate_to_fixed": 0.85, "archived": false})),
        )
        .await;
        assert_response_status_is_success(&res);
        assert!(
            res.body.expect("expected body to be set on 200").id > 0,
            "id should be greater than zero"
        );

        // Get currency
        let res: TestResponse<CurrencyResponse> =
            run_req(&app, Method::GET, "/api/currency/EUR", t, None).await;
        assert_response_status_is_success(&res);
        let name = res.body.expect("expected body to be set on 200").name;
        assert_eq!(name, "EUR", "currency name {name} should be EUR");

        // Update currency
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/currency/EUR",
            t,
            Some(json!({"name": "EUR", "rate_to_fixed": 0.9, "archived": false})),
        )
        .await;
        assert_response_status_is_success(&res);

        // Archive currency that has no sources or entries
        let res: TestResponse<EmptyResponse> =
            run_req(&app, Method::GET, "/api/currency/EUR/archive", t, None).await;
        assert_response_status_is_success(&res);

        // Confirm update and archive currency
        let res: TestResponse<CurrencyResponse> =
            run_req(&app, Method::GET, "/api/currency/EUR", t, None).await;
        assert_response_status_is_success(&res);
        let body = res.body.expect("expected body to be set on 200");
        assert!(
            (body.rate_to_fixed - 0.9).abs() < 1e-5f64,
            "currency rate {} should be 0.9",
            body.rate_to_fixed
        );
        assert!(body.archived, "currency should be archived");

        // Attempt to create the same currency again (expect failure)
        let res: TestResponse<CreateResponse> = run_req(
            &app,
            Method::POST,
            "/api/currency",
            t,
            Some(json!({"name": "EUR", "rate_to_fixed": 0.85, "archived": false})),
        )
        .await;
        assert_response_status(&res, StatusCode::BAD_REQUEST);

        // Cleanup
        {
            use crate::schema::currencies::dsl::*;
            delete_direct(&pool(), currencies.filter(name.eq("EUR")));
        }
    }
}
