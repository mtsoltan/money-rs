#![feature(try_blocks)]
#![feature(let_chains)]
#![feature(type_alias_impl_trait)]
#![feature(trait_alias)]
#![feature(stmt_expr_attributes)]

mod authentication;
mod consts;
mod env_vars;
mod handlers;
mod http;
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
                        // TODO(15): LOGIC: Also provide the monthly sums for the last 12 months, as
                        //  well as that sum but normalized by conversion rates to fixed at the time
                        //  of spending
                        .route("/{name}", web::get().to(handlers::get_currency_by_name))
                        .route("/{name}", web::post().to(handlers::update_currency))
                        .route("/{name}/archive", web::get().to(handlers::archive_currency))
                        // TODO(15): ENDPOINT: unimplemented
                        .route("/{name}/entries", web::get().to(handlers::unimplemented)),
                )
                .service(
                    web::scope("/source")
                        .route("", web::post().to(handlers::create_source))
                        .route("", web::get().to(handlers::get_sources))
                        .route("/{name}", web::get().to(handlers::get_source_by_name))
                        .route("/{name}", web::post().to(handlers::update_source))
                        .route("/{name}/archive", web::get().to(handlers::archive_source))
                        // TODO(15): ENDPOINT: Entries that have this as source 1 or source 2
                        .route("/{name}/entries", web::get().to(handlers::unimplemented)),
                )
                .service(
                    web::scope("/category")
                        .route("", web::post().to(handlers::create_category))
                        .route("", web::get().to(handlers::get_categories))
                        // TODO(15): LOGIC: Also provide the monthly sums for the last 12 months, as
                        //  well as that sum but normalized by conversion rates to fixed at the time
                        //  of spending
                        .route("/{name}", web::get().to(handlers::get_category_by_name))
                        .route("/{name}", web::post().to(handlers::update_category))
                        .route("/{name}/archive", web::get().to(handlers::archive_category))
                        // TODO(15): ENDPOINT: unimplemented
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
                        .route("/update", web::post().to(handlers::unimplemented))
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
    use crate::handlers::{EmptyResponse, LoginResponse};
    use crate::model::{CategoryResponse, CurrencyResponse, EntryResponse, SourceResponse};

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
                            "Serializing returned code 200 json body should always succeed",
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

            res.body.expect("expected body to be set on 200").token
        }

        static TOKEN: OnceCell<String> = OnceCell::const_new();
        TOKEN.get_or_init(once).await.as_str()
    }

    fn delete_direct<T>(pool: &Pool, q: T)
    where
        T: diesel::query_builder::IntoUpdateTarget,
        diesel::query_builder::DeleteStatement<T::Table, T::WhereClause>:
            diesel::query_builder::QueryFragment<diesel::pg::Pg> + diesel::query_builder::QueryId,
    {
        let mut conn = pool.get().expect("Failed to get database connection");
        diesel::delete(q).execute(&mut conn).expect("Failed to run the delete query you specified");
    }

    #[actix_web::test]
    async fn register_login() { let _ = token().await; }

    #[actix_web::test]
    async fn test_currency_lifecycle() {
        // Cleanup
        {
            use crate::schema::currencies::dsl::*;
            delete_direct(&pool(), currencies.filter(name.eq("EUR")));
        }

        // Get token
        let t = Some(token().await);
        let app = at::init_service(app(&pool())).await;

        // Create currency
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/currency",
            t,
            Some(json!({"name": "EUR", "rate_to_fixed": 1.01})),
        )
        .await;
        assert_response_status_is_success(&res);

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
            Some(json!({"name": "EUR", "rate_to_fixed": 1.06})),
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
            (body.rate_to_fixed - 1.06).abs() < consts::EPSILON,
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
            Some(json!({"name": "EUR", "rate_to_fixed": 0.9})),
        )
        .await;
        assert_response_status(&res, StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn test_source_lifecycle() {
        // Cleanup
        {
            use crate::schema::sources::dsl::*;
            delete_direct(&pool(), sources.filter(name.eq("SavingsAccount")));
        }
        {
            use crate::schema::currencies::dsl::*;
            delete_direct(&pool(), currencies.filter(name.eq("GBP")));
        }

        // Get token
        let t = Some(token().await);
        let app = at::init_service(app(&pool())).await;

        // Create currency for the source
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/currency",
            t,
            Some(json!({"name": "GBP", "rate_to_fixed": 1.28})),
        )
        .await;
        assert_response_status_is_success(&res);

        // Create source
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/source",
            t,
            Some(json!({"name": "SavingsAccount", "currency": "GBP"})),
        )
        .await;
        assert_response_status_is_success(&res);

        // Get source
        let res: TestResponse<SourceResponse> =
            run_req(&app, Method::GET, "/api/source/SavingsAccount", t, None).await;
        assert_response_status_is_success(&res);
        let name = res.body.expect("expected body to be set on 200").name;
        assert_eq!(name, "SavingsAccount", "source name should be 'SavingsAccount'");

        // Update source
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/source/SavingsAccount",
            t,
            Some(json!({"name": "SavingsAccount", "amount": 5000})),
        )
        .await;
        assert_response_status_is_success(&res);

        // Archive source fails because there is amount
        let res: TestResponse<EmptyResponse> =
            run_req(&app, Method::GET, "/api/source/SavingsAccount/archive", t, None).await;
        assert_response_status(&res, StatusCode::BAD_REQUEST);

        // Update source to have no amount
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/source/SavingsAccount",
            t,
            Some(json!({"name": "SavingsAccount", "amount": 0})),
        )
        .await;
        assert_response_status_is_success(&res);

        // Archive source
        let res: TestResponse<EmptyResponse> =
            run_req(&app, Method::GET, "/api/source/SavingsAccount/archive", t, None).await;
        assert_response_status_is_success(&res);

        // Confirm update and archive of source
        let res: TestResponse<SourceResponse> =
            run_req(&app, Method::GET, "/api/source/SavingsAccount", t, None).await;
        assert_response_status_is_success(&res);
        let body = res.body.expect("expected body to be set on 200");
        assert!(
            (body.amount - 0.0).abs() < consts::EPSILON,
            "source amount {} should be 0.0",
            body.amount
        );
        assert!(body.archived, "source should be archived");
    }

    #[actix_web::test]
    async fn test_category_lifecycle() {
        // Cleanup
        {
            use crate::schema::categories::dsl::*;
            delete_direct(&pool(), categories.filter(name.eq("RentAndBillsT")));
            delete_direct(&pool(), categories.filter(name.eq("RecurringExpensesT")));
        }

        // Get token
        let t = Some(token().await);
        let app = at::init_service(app(&pool())).await;

        // Create category
        let res: TestResponse<EmptyResponse> =
            run_req(&app, Method::POST, "/api/category", t, Some(json!({"name": "RentAndBillsT"})))
                .await;
        assert_response_status_is_success(&res);

        // Get category
        let res: TestResponse<CategoryResponse> =
            run_req(&app, Method::GET, "/api/category/RentAndBillsT", t, None).await;
        assert_response_status_is_success(&res);
        let name = res.body.expect("expected body to be set on 200").name;
        assert_eq!(name, "RentAndBillsT", "category name should be 'RentAndBillsT'");

        // Update category name
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::POST,
            "/api/category/RentAndBillsT",
            t,
            Some(json!({"name": "RecurringExpensesT"})),
        )
        .await;
        assert_response_status_is_success(&res);

        // Archive category
        let res: TestResponse<EmptyResponse> =
            run_req(&app, Method::GET, "/api/category/RecurringExpensesT/archive", t, None).await;
        assert_response_status_is_success(&res);

        // Confirm update and archive of category by fetching with new name
        let res: TestResponse<CategoryResponse> =
            run_req(&app, Method::GET, "/api/category/RecurringExpensesT", t, None).await;
        assert_response_status_is_success(&res);
        let body = res.body.expect("expected body to be set on 200");
        assert_eq!(body.name, "RecurringExpensesT", "category name should be 'RecurringExpensesT'");
        assert!(body.archived, "category should be archived");
    }

    #[actix_web::test]
    async fn test_entries_lifecycle() {
        // Cleanup: Delete currencies, categories, sources, and entries if they already exist
        {
            use crate::schema::entries::dsl::*;

            delete_direct(&pool(), entries);
        }
        {
            use crate::schema::categories::dsl::*;

            delete_direct(&pool(), categories.filter(name.eq("RecurringExpenses")));
            delete_direct(&pool(), categories.filter(name.eq("LivingExpenses")));
            delete_direct(&pool(), categories.filter(name.eq("Purchases")));
            delete_direct(&pool(), categories.filter(name.eq("Entertainment")));
        }
        {
            use crate::schema::sources::dsl::*;
            delete_direct(&pool(), sources.filter(name.eq("USDBankAccount")));
            delete_direct(&pool(), sources.filter(name.eq("USDWallet")));
            delete_direct(&pool(), sources.filter(name.eq("EGPBankAccount")));
            delete_direct(&pool(), sources.filter(name.eq("EGPWallet")));
            delete_direct(&pool(), sources.filter(name.eq("JPYBankAccount")));
            delete_direct(&pool(), sources.filter(name.eq("JPYWallet")));
        }
        {
            use crate::schema::currencies::dsl::*;
            delete_direct(&pool(), currencies.filter(name.eq("EGP")));
            delete_direct(&pool(), currencies.filter(name.eq("JPY")));
        }

        // Get token and app service
        let t = Some(token().await);
        let app = at::init_service(app(&pool())).await;

        // 1. Create Currencies: EGP and JPY
        let currencies = vec!["EGP", "JPY"];
        for &currency in &currencies {
            let res: TestResponse<EmptyResponse> = run_req(
                &app,
                Method::POST,
                "/api/currency",
                t,
                Some(json!({ "name": currency, "rate_to_fixed": 1.0 })),
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
        let source_data = vec![
            ("USDBankAccount", "USD"),
            ("USDWallet", "USD"),
            ("EGPBankAccount", "EGP"),
            ("EGPWallet", "EGP"),
            ("JPYBankAccount", "JPY"),
            ("JPYWallet", "JPY"),
        ];
        for &(source, currency) in &source_data {
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

        // 4. Create 20 Entries with varying types and attributes
        let entries_data = vec![
            ("Spend", 100.0, "USDBankAccount", None, "LivingExpenses", "2023-01-01"),
            ("Income", 200.0, "USDWallet", None, "RecurringExpenses", "2023-02-01"),
            ("Lend", 50.0, "USDBankAccount", Some("John Doe"), "Purchases", "2023-03-01"),
            ("Borrow", 75.0, "USDWallet", Some("Jane Doe"), "Entertainment", "2023-04-01"),
            ("Convert", 500.0, "USDBankAccount", Some("USDWallet"), "LivingExpenses", "2023-05-01"),
            ("Spend", 120.0, "EGPBankAccount", None, "Purchases", "2023-06-01"),
            ("Income", 130.0, "EGPWallet", None, "LivingExpenses", "2023-07-01"),
            ("Lend", 80.0, "JPYBankAccount", Some("Friend"), "Entertainment", "2023-08-01"),
            ("Borrow", 90.0, "JPYWallet", Some("Colleague"), "RecurringExpenses", "2023-09-01"),
            ("Convert", 200.0, "EGPWallet", Some("EGPBankAccount"), "Purchases", "2023-10-01"),
            ("Spend", 110.0, "USDWallet", None, "LivingExpenses", "2023-11-01"),
            ("Income", 115.0, "JPYBankAccount", None, "Entertainment", "2023-12-01"),
            ("Lend", 85.0, "USDWallet", Some("Neighbor"), "Purchases", "2024-01-01"),
            ("Borrow", 65.0, "USDBankAccount", Some("Relative"), "Entertainment", "2024-02-01"),
            ("Convert", 400.0, "USDWallet", Some("USDWallet"), "RecurringExpenses", "2024-03-01"),
            ("Spend", 90.0, "JPYBankAccount", None, "Entertainment", "2024-04-01"),
            ("Income", 200.0, "JPYWallet", None, "RecurringExpenses", "2024-05-01"),
            ("Lend", 75.0, "EGPBankAccount", Some("Associate"), "LivingExpenses", "2024-06-01"),
            ("Borrow", 85.0, "EGPWallet", Some("Partner"), "Purchases", "2024-07-01"),
            ("Convert", 500.0, "JPYWallet", Some("JPYBankAccount"), "Entertainment", "2024-08-01"),
        ];

        for (entry_type, amount, source, target, category, date) in entries_data {
            let res: TestResponse<EmptyResponse> = run_req(
                &app,
                Method::POST,
                "/api/entry",
                t,
                Some(json!({
                    "entry_type": entry_type,
                    "amount": amount,
                    "source": source,
                    "target": target,
                    "category": category,
                    "description": "Sample Entry",
                    "date": date,
                    "currency": "USD", // TODO(10): BUG: Should auto-convert if creating a USD entry from an EGP source
                    "conversion_rate_to_fixed": 1.00,
                    "conversion_rate": 1.00 // TODO(10): STRUCTURE: this and conversion_rate_to_fixed should not be provided in requests
                })),
            )
            .await;
            assert_response_status_is_success(&res);
        }

        // 5. Get all entries and ensure the count is 20
        let res: TestResponse<Vec<EntryResponse>> =
            run_req(&app, Method::GET, "/api/entry/all", t, None).await;
        assert_response_status_is_success(&res);
        let all_entries = res.body.expect("Expected entries in response");
        assert_eq!(all_entries.len(), 20, "Expected 20 entries initially");

        // 6. Archive 2 entries and delete 2 entries
        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::GET,
            &format!("/api/entry/archive?ids[]={}&ids[]={}", all_entries[0].id, all_entries[1].id),
            t,
            None,
        )
        .await;
        assert_response_status_is_success(&res);

        let res: TestResponse<EmptyResponse> = run_req(
            &app,
            Method::DELETE,
            &format!("/api/entry?ids[]={}&ids[]={}", all_entries[2].id, all_entries[3].id),
            t,
            None,
        )
        .await;
        assert_response_status_is_success(&res);

        // 7. Ensure count is now 18 and 2 archived entries
        let res: TestResponse<Vec<EntryResponse>> =
            run_req(&app, Method::GET, "/api/entry/all", t, None).await;
        assert_response_status_is_success(&res);
        let body = res.body.expect("Expected entries in response");
        assert_eq!(body.len(), 18, "Expected 18 entries after deletion");
        assert_eq!(body.iter().filter(|o| o.archived).count(), 2, "Expected 2 archived entries");

        // 8. Use find entries with different filters and verify results
        let filters = vec![
            (Some(100.0), None, None, 3), // Find entries with amount 100.0
            (None, Some(80.0), None, 5),  // Find entries with min amount 80.0
            (None, None, Some(120.0), 7), // Find entries with max amount 120.0
            (None, None, None, 20),       // Find all entries
        ];

        for (amount, min_amount, max_amount, expected_count) in filters {
            let mut filter = json!({});
            if let Some(amount) = amount {
                filter["amount"] = json!(amount);
            }
            if let Some(min) = min_amount {
                filter["min_amount"] = json!(min);
            }
            if let Some(max) = max_amount {
                filter["max_amount"] = json!(max);
            }

            let res: TestResponse<Vec<EntryResponse>> =
                run_req(&app, Method::GET, "/api/entry", t, Some(filter)).await;
            assert_response_status_is_success(&res);
            let filtered_entries = res.body.expect("Expected filtered entries");
            assert_eq!(filtered_entries.len(), expected_count, "Unexpected entry count for filter");
        }
    }
}
