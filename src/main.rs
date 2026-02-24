#![feature(try_blocks)]
#![feature(type_alias_impl_trait)]
#![feature(trait_alias)]
#![feature(stmt_expr_attributes)]

extern crate core;

mod authentication;
mod consts;
mod entity;
mod env_vars;
mod handlers;
mod http;

use actix_files::{Files, NamedFile};
use actix_web::{App, HttpServer, web};
use actix_web_httpauth::middleware::HttpAuthentication;
use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;

use crate::consts::{Conn, Pool};

#[derive(Clone)]
pub struct AppState {
    pool: Pool,
}

impl AppState {
    pub async fn cpool(&self) -> Conn {
        self.pool.clone().get().await.expect("X003: Pool should be initialized")
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_vars::init();
    HttpServer::new(move || app(pool())).bind(env_vars::bind_address())?.run().await
}

fn pool() -> Pool {
    env_vars::init();
    let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new(env_vars::database_url());
    Pool::builder(manager).build().expect("X005: Failed to create pool")
}

fn app(
    pool: Pool,
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
        .route("/login", web::post().to(handlers::login))
        .service(
            web::scope("/api")
                .wrap(HttpAuthentication::bearer(authentication::jwt_validator_generator))
                .service(
                    web::scope("/currency")
                        .route("", web::post().to(handlers::create_currency))
                        // Parameters: page
                        .route("", web::get().to(handlers::get_currencies))
                        .route("/{name}/stats", web::get().to(handlers::get_currency_stats))
                        .route("/{name}", web::get().to(handlers::get_currency_by_name))
                        .route("/{name}", web::post().to(handlers::update_currency))
                        .route("/{name}/archive", web::get().to(handlers::archive_currency))
                        // Parameters: page
                        .route("/{name}/entries", web::get().to(handlers::get_currency_entries))
                        // Parameters: page
                        .route("/{name}/sources", web::get().to(handlers::get_currency_sources)),
                )
                .service(
                    web::scope("/source")
                        .route("", web::post().to(handlers::create_source))
                        // Parameters: page
                        .route("", web::get().to(handlers::get_sources))
                        .route("/{name}", web::get().to(handlers::get_source_by_name))
                        .route("/{name}", web::post().to(handlers::update_source))
                        .route("/{name}/archive", web::get().to(handlers::archive_source))
                        // Parameters: page, primary_only
                        .route("/{name}/entries", web::get().to(handlers::get_source_entries)),
                )
                .service(
                    web::scope("/category")
                        .route("", web::post().to(handlers::create_category))
                        // Parameters: page
                        .route("", web::get().to(handlers::get_categories))
                        // Parameters: now
                        .route("/{name}/stats", web::get().to(handlers::get_category_stats))
                        .route("/{name}", web::get().to(handlers::get_category_by_name))
                        .route("/{name}", web::post().to(handlers::update_category))
                        .route("/{name}/archive", web::get().to(handlers::archive_category))
                        // Parameters: page
                        .route("/{name}/entries", web::get().to(handlers::get_category_entries)),
                )
                .service(
                    web::scope("/entry")
                        .route("", web::post().to(handlers::create_entry))
                        // Basic get-all handler, does not return any statistics
                        // Parameters: page
                        .route("/all", web::get().to(handlers::get_entries))
                        // Sort
                        // - amount_asc, amount_desc
                        // - date_asc, date_desc
                        // - create_asc, create_desc
                        // - defaults to date_asc
                        // The rest of the fields (source|currency|category|entry_type|etc.)
                        // have filtering but no sorting
                        // Sort by source|currency|category|entry_type should be client-side.
                        // Returns the entries, their sum, their average per month, and their
                        // sum-per-category-per-month
                        .route("", web::get().to(handlers::find_entries))
                        // Parameters: ids
                        .route("/update", web::post().to(handlers::update_entry))
                        // Parameters: ids
                        .route("", web::delete().to(handlers::delete_entries))
                        .route("/archive", web::get().to(handlers::archive_entries)),
                ),
        )
        .service(
            web::scope("/app")
                // Serve the built assets (WASM, JS, CSS). `""` mounts inside the scope prefix.
                .service(
                    web::scope("/static")
                        .service(Files::new("", env_vars::app_dist_dir()).index_file("index.html")),
                )
                // SPA fallback for deep links under /app/*
                .default_service(web::get().to(spa_index)),
        );

    #[cfg(any(test, feature = "create_user"))]
    let app = app.route("/user", web::post().to(handlers::create_user));

    #[cfg(any(test, feature = "create_user"))]
    let app = app.route("/user/{username}", web::delete().to(handlers::delete_user));

    app
}

async fn spa_index() -> Result<NamedFile, actix_web::Error> {
    NamedFile::open_async(dbg!(std::path::Path::join(env_vars::app_dist_dir(), "index.html")))
        .await
        .map_err(|e| actix_web::error::ErrorNotFound(format!("index.html not found: {e}")))
}

// Tests can run in parallel, do not duplicate source, currency, or category names.
// Always append some prefix (e.g. `T1`, `T2`, etc.) before their names in different tests.
#[cfg(test)]
mod tests {
    use std::fmt::Debug;
    use std::pin::Pin;

    use actix_http::Request;
    use actix_http::body::MessageBody;
    use actix_http::error::PayloadError;
    use actix_web::dev::ServiceResponse;
    use actix_web::http::{Method, StatusCode};
    use actix_web::test as at;
    use diesel::prelude::*;
    use fpdec::{Dec, Decimal};
    use model::entity::{
        CategoryResponse, CategoryStatsResponse, CurrencyResponse, EmptyResponse, EntryQuery,
        EntryResponse, FindEntriesResponse, LoginResponse, SourceResponse,
    };
    use serde::Serialize;
    use serde::de::{DeserializeOwned, StdError};
    use serde_json::json;
    use tokio::sync::OnceCell;

    use super::*;
    use crate::env_vars::page_size;

    static TEST_USERNAME: &str = "root";
    static TEST_PASSWORD: &str = "root";
    static TEST_CURRENCY: &str = "USD";

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
                    // This logic is the same as read_body_json, but we can place an intermediate
                    // debug statement when it's a string.
                    let body = at::read_body(res).await;
                    // let dbg_val = dbg!(String::from_utf8(body.to_vec()).unwrap()); // We can use
                    // this to debug.
                    let res_struct = serde_json::from_slice(&body)
                        .map_err(Into::<Box<dyn StdError>>::into)
                        .unwrap_or_else(|err| {
                            panic!(
                                "could not deserialize body into a {}\nerr: {}",
                                std::any::type_name::<T>(),
                                err,
                            )
                        });
                    TestResponse {
                        status_code,
                        body_string: serde_json::to_string(&res_struct).expect(
                            "XT: Serializing returned code 200 json body should always succeed",
                        ),
                        body: Some(res_struct),
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
    ///
    /// Sends requests to three endpoints. Asserts that the delete endpoint returned success or
    /// not-found, and that the create and login endpoints returned success. This is set up this way
    /// to allow for setup without having to use something like
    /// `#![feature(custom_test_frameworks)]`. We have it such that teardown, which is only
    /// achievable using custom frameworks, is not necessary.
    async fn token() -> &'static str {
        async fn once() -> String {
            let app = at::init_service(app(pool())).await;

            let res: TestResponse<EmptyResponse> = run_req(
                &app,
                Method::DELETE,
                format!("/user/{TEST_USERNAME}").as_str(),
                None,
                None,
            )
            .await;
            assert!(res.status_code.is_success() || res.status_code == StatusCode::NOT_FOUND);

            let res: TestResponse<EmptyResponse> = run_req(
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

            res.body.expect("XT: expected body to be set on 200").token
        }

        static TOKEN: OnceCell<String> = OnceCell::const_new();
        TOKEN.get_or_init(once).await.as_str()
    }

    async fn delete_direct<T>(pool: &Pool, q: T)
    where
        T: diesel::query_builder::IntoUpdateTarget + Send,
        diesel::query_builder::DeleteStatement<T::Table, T::WhereClause>:
            diesel::query_builder::QueryFragment<diesel::pg::Pg>
                + diesel::query_builder::QueryId
                + Send,
    {
        let mut conn = pool.get().await.expect("XT: Failed to get database connection");
        diesel_async::RunQueryDsl::execute(diesel::delete(q), &mut conn)
            .await
            .expect("XT: Failed to run the delete query you specified");
    }

    #[actix_web::test]
    async fn test_register_login() { let _ = token().await; }

    #[actix_web::test]
    async fn test_currency_lifecycle() {
        // Cleanup
        {
            use model::schema::currencies::dsl::*;
            delete_direct(&pool(), currencies.filter(name.eq("T1EUR"))).await;
        }

        // Get token
        let t = Some(token().await);
        let app = at::init_service(app(pool())).await;

        // Create currency
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/currency",
            t,
            Some(json!({"name": "T1EUR", "rate_to_fixed": "1.01"})),
        )
        .await;
        assert_response_status_is_success(&res);

        // Get currency
        let res: TestResponse<CurrencyResponse> =
            run_req(&app, Method::GET, "/api/currency/T1EUR", t, None).await;
        assert_response_status_is_success(&res);
        let name = res.body.expect("XT: expected body to be set on 200").name;
        assert_eq!(name, "T1EUR", "currency name {name} should be T1EUR");

        // Update currency
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/currency/T1EUR",
            t,
            Some(json!({"name": "T1EUR", "rate_to_fixed": "1.06"})),
        )
        .await;
        assert_response_status_is_success(&res);

        // Archive currency that has no sources or entries
        let res: TestResponse<EmptyResponse> =
            run_req(&app, Method::GET, "/api/currency/T1EUR/archive", t, None).await;
        assert_response_status_is_success(&res);

        // Confirm update and archive currency
        let res: TestResponse<CurrencyResponse> =
            run_req(&app, Method::GET, "/api/currency/T1EUR", t, None).await;
        assert_response_status_is_success(&res);
        let body = res.body.expect("XT: expected body to be set on 200");
        assert!(
            (body.rate_to_fixed - Dec!(1.06)).abs() < consts::EPSILON,
            "currency rate {} should be 1.06",
            body.rate_to_fixed
        );
        assert!(body.archived, "currency should be archived");

        // Attempt to create the same currency again (expect failure)
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/currency",
            t,
            Some(json!({"name": "T1EUR", "rate_to_fixed": "0.9"})),
        )
        .await;
        assert_response_status(&res, StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn test_source_lifecycle() {
        // Cleanup
        {
            use model::schema::sources::dsl::*;
            delete_direct(&pool(), sources.filter(name.eq("T2SavingsAccount"))).await;
        }
        {
            use model::schema::currencies::dsl::*;
            delete_direct(&pool(), currencies.filter(name.eq("T2GBP"))).await;
        }

        // Get token
        let t = Some(token().await);
        let app = at::init_service(app(pool())).await;

        // Create currency for the source
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/currency",
            t,
            Some(json!({"name": "T2GBP", "rate_to_fixed": "1.28"})),
        )
        .await;
        assert_response_status_is_success(&res);

        // Create a source
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/source",
            t,
            Some(json!({"name": "T2SavingsAccount", "currency": "T2GBP"})),
        )
        .await;
        assert_response_status_is_success(&res);

        // Get a source
        let res: TestResponse<SourceResponse> =
            run_req(&app, Method::GET, "/api/source/T2SavingsAccount", t, None).await;
        assert_response_status_is_success(&res);
        let name = res.body.expect("XT: expected body to be set on 200").name;
        assert_eq!(name, "T2SavingsAccount", "source name should be 'T2SavingsAccount'");

        // Update a source
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/source/T2SavingsAccount",
            t,
            Some(json!({"name": "T2SavingsAccount", "amount": "5000"})),
        )
        .await;
        assert_response_status_is_success(&res);

        // Archive source fails because there is amount
        let res: TestResponse<EmptyResponse> =
            run_req(&app, Method::GET, "/api/source/T2SavingsAccount/archive", t, None).await;
        assert_response_status(&res, StatusCode::BAD_REQUEST);

        // Update source to have no amount
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/source/T2SavingsAccount",
            t,
            Some(json!({"name": "T2SavingsAccount", "amount": "0"})),
        )
        .await;
        assert_response_status_is_success(&res);

        // Archive source
        let res: TestResponse<EmptyResponse> =
            run_req(&app, Method::GET, "/api/source/T2SavingsAccount/archive", t, None).await;
        assert_response_status_is_success(&res);

        // Confirm update and archive of a source
        let res: TestResponse<SourceResponse> =
            run_req(&app, Method::GET, "/api/source/T2SavingsAccount", t, None).await;
        assert_response_status_is_success(&res);
        let body = res.body.expect("XT: expected body to be set on 200");
        assert!(body.amount.abs() < consts::EPSILON, "source amount {} should be 0.0", body.amount);
        assert!(body.archived, "source should be archived");
    }

    #[actix_web::test]
    async fn test_category_lifecycle() {
        // Cleanup
        {
            use model::schema::categories::dsl::*;
            delete_direct(&pool(), categories.filter(name.eq("T3RentAndBills"))).await;
            delete_direct(&pool(), categories.filter(name.eq("T3RecurringExpenses"))).await;
        }

        // Get token
        let t = Some(token().await);
        let app = at::init_service(app(pool())).await;

        // Create category
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/category",
            t,
            Some(json!({"name": "T3RentAndBills"})),
        )
        .await;
        assert_response_status_is_success(&res);

        // Get category
        let res: TestResponse<CategoryResponse> =
            run_req(&app, Method::GET, "/api/category/T3RentAndBills", t, None).await;
        assert_response_status_is_success(&res);
        let name = res.body.expect("XT: expected body to be set on 200").name;
        assert_eq!(name, "T3RentAndBills", "category name should be 'T3RentAndBills'");

        // Update category name
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/category/T3RentAndBills",
            t,
            Some(json!({"name": "T3RecurringExpenses"})),
        )
        .await;
        assert_response_status_is_success(&res);

        // Archive category
        let res: TestResponse<EmptyResponse> =
            run_req(&app, Method::GET, "/api/category/T3RecurringExpenses/archive", t, None).await;
        assert_response_status_is_success(&res);

        // Confirm update and archive of category by fetching with new name
        let res: TestResponse<CategoryResponse> =
            run_req(&app, Method::GET, "/api/category/T3RecurringExpenses", t, None).await;
        assert_response_status_is_success(&res);
        let body = res.body.expect("XT: expected body to be set on 200");
        assert_eq!(
            body.name, "T3RecurringExpenses",
            "category name should be 'T3RecurringExpenses'"
        );
        assert!(body.archived, "category should be archived");
    }

    #[actix_web::test]
    async fn test_entries_lifecycle() {
        // 0. Cleanup: Delete currencies, categories, sources, and entries if they already exist
        {
            use model::schema::categories::dsl::*;
            use model::schema::currencies::dsl::*;
            use model::schema::entries::dsl::*;
            use model::schema::sources::dsl::*;
            delete_direct(&pool(), entries).await;
            delete_direct(&pool(), categories).await;
            delete_direct(&pool(), sources).await;
            delete_direct(&pool(), currencies).await;
        }

        // Get token and app service
        let t = Some(token().await);
        let app = at::init_service(app(pool())).await;

        // 1. Create Currencies: EGP and JPY
        let currencies = vec![("EGP", "0.02"), ("JPY", "0.00667")];
        for &(currency, rtf) in &currencies {
            let res: TestResponse<EmptyResponse> = run_req(
                &app,
                Method::POST,
                "/api/currency",
                t,
                Some(json!({ "name": currency, "rate_to_fixed": rtf })),
            )
            .await;
            assert_response_status_is_success(&res);
        }

        // 2. Create Categories: RecurringExpenses, LivingExpenses, Purchases, Entertainment
        let categories = vec!["RecurringExpenses", "LivingExpenses", "Purchases", "Entertainment"];
        for &category in &categories {
            let res: TestResponse<EmptyResponse> =
                run_req(&app, Method::POST, "/api/category", t, Some(json!({ "name": category })))
                    .await;
            assert_response_status_is_success(&res);
        }

        // 3. Create Sources for each currency (USD, EGP, JPY)
        // Final amounts before archival and deletion are included.
        let source_data = vec![
            ("USDBankAccount", "USD", Dec!(915.0)),
            ("USDWallet", "USD", Dec!(1080.0)),
            ("EGPBankAccount", "EGP", Dec!(40250.0)),
            ("EGPWallet", "EGP", Dec!(60750.0)),
            ("JPYBankAccount", "JPY", Dec!(142079.160419)),
            ("JPYWallet", "JPY", Dec!(179410.044977)),
        ];
        for &(source, currency, _) in &source_data {
            let res: TestResponse<EmptyResponse> = run_req(
                &app,
                Method::POST,
                "/api/source",
                t,
                Some(json!({ "name": source, "currency": currency })),
            )
            .await;
            assert_response_status_is_success(&res);
        }

        // Converts without a second source should throw bad request
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/entry",
            t,
            Some(json!({
                "entry_type": "Convert",
                "amount": "500.0",
                "source": "JPYWallet",
                "target": Some("Relative"),
                "category": "LivingExpenses",
                "description": "Sample Entry",
                "date": "2023-05-01",
                "currency": "USD",
            })),
        )
        .await;
        assert_response_status(&res, StatusCode::BAD_REQUEST);
        assert!(&res.body_string.contains("Malformed CreateEntryRequest"));

        // Requests with unknown fields should throw bad request
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/entry",
            t,
            Some(json!({
                "entry_type": "Borrow",
                "amount": 500.0,
                "source": "JPYWallet",
                "target": Some("Relative"),
                "category": "LivingExpenses",
                "description": "Sample Entry",
                "date": "2023-05-01",
                "currency": "USD",
                "conversion_rate_to_fixed": 1.00,
                "conversion_rate": 1.00
            })),
        )
        .await;
        assert_response_status(&res, StatusCode::BAD_REQUEST);
        assert!(&res.body_string.contains("Json deserialize error"));

        // 4. Create 20 Entries with varying types and attributes
        enum Expected {
            Success,
            BadRequest,
        }
        #[rustfmt::skip]
        let entries_data = vec![
            // Start with 1k usd worth of currency in each source, jpy is 149925.037481, egp is 50000
            ("Income", "1000.0", "USD", "JPYBankAccount", None,                   None,                   None,            "RecurringExpenses", "2023-05-01", Expected::Success),
            ("Income", "1000.0", "USD", "EGPBankAccount", None,                   None,                   None,            "RecurringExpenses", "2023-05-02", Expected::Success),
            ("Income", "1000.0", "USD", "USDBankAccount", None,                   None,                   None,            "RecurringExpenses", "2023-05-03", Expected::Success),
            ("Income", "1000.0", "USD", "JPYWallet",      None,                   None,                   None,            "RecurringExpenses", "2023-05-04", Expected::Success),
            ("Income", "1000.0", "USD", "EGPWallet",      None,                   None,                   None,            "RecurringExpenses", "2023-05-05", Expected::Success),
            ("Income", "1000.0", "USD", "USDWallet",      None,                   None,                   None,            "RecurringExpenses", "2023-05-06", Expected::Success),
            ("Borrow",   "65.0", "USD", "USDBankAccount", Some("Relative"),       None,                   None,            "Entertainment",     "2023-02-07", Expected::Success), // USDBankAccount 1065 - Gets archived
            ("Convert", "400.0", "EGP", "USDWallet",      None,                   Some("JPYWallet"),      Some("61877.0"), "RecurringExpenses", "2023-03-08", Expected::BadRequest), // The EGP currency in this entry is ambiguous
            ("Spend",    "90.0", "USD", "JPYBankAccount", None,                   None,                   None,            "Entertainment",     "2023-04-09", Expected::Success), // JPYBankAccount 136431.784108
            ("Income",  "200.0", "USD", "JPYWallet",      None,                   None,                   None,            "RecurringExpenses", "2023-05-10", Expected::Success), // JPYWallet 179910.044977
            ("Lend",     "75.0", "USD", "EGPBankAccount", Some("Associate"),      None,                   None,            "LivingExpenses",    "2023-06-11", Expected::Success), // EGPBankAccount 46250
            ("Borrow",   "85.0", "USD", "EGPWallet",      Some("Partner"),        None,                   None,            "Purchases",         "2023-07-12", Expected::Success), // EGPWallet 54250
            ("Convert", "500.0", "JPY", "JPYWallet",      None,                   Some("JPYBankAccount"), Some("400.0"),   "Entertainment",     "2023-08-13", Expected::Success), // JPYWallet 179410.044977 JPYBankAccount 136831.784108 - This conversion should just lose me money, but it should be valid
            ("Spend",   "100.0", "USD", "USDBankAccount", None,                   None,                   None,            "LivingExpenses",    "2023-01-14", Expected::Success), // USDBankAccount 965 - Gets archived
            ("Income",  "200.0", "USD", "USDWallet",      None,                   None,                   None,            "RecurringExpenses", "2023-02-15", Expected::Success), // USDWallet 1200 - Gets deleted
            ("Lend",     "50.0", "USD", "USDBankAccount", Some("John Doe"),       None,                   None,            "Purchases",         "2023-03-16", Expected::Success), // USDBankAccount 915 - Gets deleted
            ("Borrow",   "75.0", "USD", "USDWallet",      Some("Jane Doe"),       None,                   None,            "Entertainment",     "2023-04-17", Expected::Success), // USDWallet 1275
            ("Convert", "500.0", "USD", "USDBankAccount", None,                   Some("EGPWallet"),      None,            "LivingExpenses",    "2023-05-18", Expected::BadRequest), // Convert without secondary source amount
            ("Spend",   "120.0", "USD", "EGPBankAccount", None,                   None,                   None,            "Purchases",         "2023-06-19", Expected::Success), // EGPBankAccount 40250
            ("Income",  "130.0", "USD", "EGPWallet",      None,                   None,                   None,            "LivingExpenses",    "2023-07-20", Expected::Success), // EGPWallet 60750
            ("Lend",     "80.0", "USD", "JPYBankAccount", Some("Friend"),         None,                   None,            "Entertainment",     "2023-08-21", Expected::Success), // JPYBankAccount 124837.781109
            ("Borrow",   "90.0", "USD", "JPYWallet",      None,                   None,                   None,            "RecurringExpenses", "2023-09-22", Expected::BadRequest), // Borrow / lend needs target
            ("Convert", "200.0", "JPY", "JPYWallet",      None,                   Some("EGPBankAccount"), None,            "Purchases",         "2023-10-23", Expected::BadRequest), // Secondary source amount required on convert
            ("Spend",   "110.0", "USD", "USDWallet",      None,                   None,                   None,            "LivingExpenses",    "2023-11-24", Expected::Success), // USDWallet 1165
            ("Income",  "115.0", "USD", "JPYBankAccount", None,                   None,                   None,            "Entertainment",     "2023-12-25", Expected::Success), // JPYBankAccount 142079.160419
            ("Lend",     "85.0", "USD", "USDWallet",      Some("Neighbor"),       None,                   None,            "Purchases",         "2024-01-26", Expected::Success), // USDWallet 1080
        ];

        for (
            i,
            (
                entry_type,
                amount,
                currency,
                source,
                target,
                secondary_source,
                secondary_source_amount,
                category,
                date,
                expected,
            ),
        ) in entries_data.into_iter().enumerate()
        {
            let res: TestResponse<EmptyResponse> = run_req(
                &app,
                Method::POST,
                "/api/entry",
                t,
                Some(json!({
                    "entry_type": entry_type,
                    "amount": amount,
                    "source": source,
                    "secondary_source": secondary_source,
                    "secondary_source_amount": secondary_source_amount,
                    "target": target,
                    "category": category,
                    "description": format!("Sample Entry {i}"),
                    "date": date,
                    "currency": currency,
                })),
            )
            .await;
            match expected {
                Expected::Success => assert_response_status_is_success(&res),
                Expected::BadRequest => assert_response_status(&res, StatusCode::BAD_REQUEST),
            }
        }

        // 5. Assert sources after entry creation but before archival and deletion.
        for &(source, _, final_amount) in &source_data {
            let res: TestResponse<SourceResponse> =
                run_req(&app, Method::GET, format!("/api/source/{source}").as_str(), t, None).await;
            assert_response_status_is_success(&res);
            let amount = res.body.expect("XT: expected body to be set on 200").amount;
            assert!(
                (amount - final_amount).abs() < consts::EPSILON,
                "{source}: expected final_amount to be {final_amount} found {amount}"
            );
        }

        // 6. Get all entries and ensure the count is 22 - the other 4 are bad requests
        let res: TestResponse<Vec<EntryResponse>> =
            run_req(&app, Method::GET, "/api/entry/all", t, None).await;
        assert_response_status_is_success(&res);
        let all_entries = res.body.expect("XT: Expected entries in response");
        assert_eq!(all_entries.len(), 22, "Expected 22 entries initially");

        // 7. Use find entries with different filters and verify results
        #[rustfmt::skip]
        let filters: Vec<(EntryQuery, i32)> = vec![
            (EntryQuery { amount: Some(Dec!(90.0)), ..Default::default() }, -1),
            // The other 90 is a bad request
            (EntryQuery { amount: Some(Dec!(90.0)), currency: Some("USD".to_string()), ..Default::default() }, 1),
            (EntryQuery { min_amount: Some(Dec!(80.0)), currency: Some("USD".to_string()), ..Default::default() }, 17),
            // The following two ensure converts work
            (EntryQuery { max_amount: Some(Dec!(120.0)), currency: Some("USD".to_string()), ..Default::default() }, 12),
            (EntryQuery { max_amount_in_fixed: Some(Dec!(120.0)), ..Default::default() }, 13),
            (EntryQuery { currencies: Some(vec!["EGP".to_string()]), ..Default::default() }, 0),
            (EntryQuery { currencies: Some(vec!["JPY".to_string()]), ..Default::default() }, 1),
            (EntryQuery { sources: Some(vec!["JPYBankAccount".to_string()]), ..Default::default()}, 4),
        ];
        let filters_qs =
            filters.into_iter().map(|o| (serde_qs::to_string::<EntryQuery>(&o.0), o.1));

        for (filter, expected_count) in filters_qs {
            let filter = filter.expect("XT: Filter should always succeed in serializing");
            let res: TestResponse<FindEntriesResponse> =
                run_req(&app, Method::GET, format!("/api/entry?{filter}").as_str(), t, None).await;
            if expected_count < 0 {
                assert_response_status(&res, StatusCode::BAD_REQUEST);
                continue;
            }

            assert_response_status_is_success(&res);
            let filtered_entries = res.body.expect("XT: Expected filtered entries");
            // dbg!(json!(filtered_entries));
            assert_eq!(
                filtered_entries.entries.len(),
                expected_count as usize,
                "Unexpected entry count {} for filter {} - expected {}",
                filtered_entries.entries.len(),
                filter,
                expected_count
            );
        }

        // 8. Archive 2 entries and delete 2 entries
        // This will always archive 6 and 13 due to their date
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::GET,
            &format!("/api/entry/archive?ids[]={}&ids[]={}", all_entries[0].id, all_entries[1].id),
            t,
            None,
        )
        .await;
        assert_response_status_is_success(&res);

        // This will always delete 7 and 14 due to their date
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::DELETE,
            &format!("/api/entry?ids[]={}&ids[]={}", all_entries[2].id, all_entries[3].id),
            t,
            None,
        )
        .await;
        assert_response_status_is_success(&res);

        // 9. Ensure count is now 20 and 2 archived entries
        let res: TestResponse<Vec<EntryResponse>> =
            run_req(&app, Method::GET, "/api/entry/all", t, None).await;
        assert_response_status_is_success(&res);
        let body = res.body.expect("XT: Expected entries in response");
        assert_eq!(body.len(), 20, "XT: Expected 20 entries after deletion");
        assert_eq!(body.iter().filter(|o| o.archived).count(), 2, "Expected 2 archived entries");

        // 10. Assert pagination works
        let res: TestResponse<Vec<EntryResponse>> =
            run_req(&app, Method::GET, "/api/entry/all?page=1", t, None).await;
        assert_response_status_is_success(&res);
        let body = res.body.expect("XT: Expected entries in response");
        assert_eq!(
            body.len() as u32,
            page_size(),
            "Expected {} entries in paginated request",
            page_size()
        );

        // 11. Assert that entries-by-parent endpoints work
        let res: TestResponse<Vec<EntryResponse>> =
            run_req(&app, Method::GET, "/api/currency/JPY/entries", t, None).await;
        assert_response_status_is_success(&res);
        assert!(
            res.body.expect("XT: Expected entries in response").iter().all(|e| e.currency == "JPY")
        );

        let res: TestResponse<Vec<EntryResponse>> =
            run_req(&app, Method::GET, "/api/source/JPYWallet/entries", t, None).await;
        assert_response_status_is_success(&res);
        assert!(
            res.body
                .expect("XT: Expected entries in response")
                .iter()
                .all(|e| e.source == "JPYWallet")
        );

        let res: TestResponse<Vec<EntryResponse>> =
            run_req(&app, Method::GET, "/api/category/Entertainment/entries", t, None).await;
        assert_response_status_is_success(&res);
        assert!(
            res.body
                .expect("XT: Expected entries in response")
                .iter()
                .all(|e| e.category == "Entertainment")
        );

        // 12. Ensure that sources with deleted entries get their amounts returned
        for (source, final_amount) in
            vec![("USDWallet", Dec!(880.0)), ("USDBankAccount", Dec!(965.0))]
        {
            let res: TestResponse<SourceResponse> =
                run_req(&app, Method::GET, format!("/api/source/{source}").as_str(), t, None).await;
            assert_response_status_is_success(&res);
            let amount = res.body.expect("XT: expected body to be set on 200").amount;
            assert!(
                (amount - final_amount).abs() < consts::EPSILON,
                "{source}: expected final_amount to be {final_amount} found {amount}"
            );
        }

        // 13. category stats – for “Entertainment” there are no entries in 2024-08-01…2025-08-01
        let res: TestResponse<CategoryStatsResponse> =
            run_req(&app, Method::GET, "/api/category/Entertainment/stats?now=2023-12-05", t, None)
                .await;
        assert_response_status_is_success(&res);
        let stats = res.body.expect("XT: Expected CategoryStatsResponse");

        // sum over past 12 months (2024-08 through 2025-07) is 0
        assert!(
            (stats.year_sum_in_fixed - Dec!(313.335)).abs() < consts::EPSILON,
            "expected year_sum_in_fixed 313.335, got {}",
            stats.year_sum_in_fixed
        );
        // average = sum/12 = 0
        assert!(
            (stats.monthly_average_in_fixed - Dec!(26.11125)).abs() < consts::EPSILON,
            "expected monthly_average_in_fixed 26.11125, got {}",
            stats.monthly_average_in_fixed
        );
        // breakdown must be twelve zeros
        assert_eq!(
            stats.month_breakdown_in_fixed,
            vec![
                Dec!(0.0),
                Dec!(0.0),
                Dec!(65.0),
                Dec!(0.0),
                Dec!(165.0),
                Dec!(0.0),
                Dec!(0.0),
                Dec!(0.0),
                Dec!(83.335),
                Dec!(0.0),
                Dec!(0.0),
                Dec!(0.0)
            ]
        );
        // current month (2025-08) sum = 0
        assert!(
            (stats.current_month_in_fixed - Dec!(115.0)).abs() < consts::EPSILON,
            "expected current_month_in_fixed 115.0, got {}",
            stats.current_month_in_fixed
        );
        // no “largest spends” in the empty window
        assert!(stats.year_largest_spends.len() == 6);

        // TODO(40): TEST: Test newly implemented endpoints:
        //  - Get a currency's sources
        //  - Update entries by ids (bulk update entries)
        //  Then run tests with coverage to check coverage on handlers, model, auth and numeric
    }
}
