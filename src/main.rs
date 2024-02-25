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
    let manager = ConnectionManager::<PgConnection>::new(env_vars::database_url());
    let pool = Pool::builder()
        .build(manager)
        .expect("Failmed to create pool");
    HttpServer::new(move || app(&pool))
        .bind(env_vars::bind_address())?
        .run()
        .await
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
                        // TODO: Also provide the monthly sums for the last 12 months
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
                        // TODO has the following query parameters:
                        // - ids (IN) - for multi-select
                        // - sources (IN)
                        // - currencies (IN)
                        // - categories (IN)
                        // - amount (EQ - care float)
                        // - min_amount (GTE)
                        // - max_amount (LTE)
                        // - date (EQ)
                        // - after (GTE)
                        // - before (LTE)
                        // - created_after (GTE)
                        // - created_before (LTE)
                        // - description (LIKE)
                        // - entry_types (IN)
                        // - limit (default: 500)
                        // - sort (comma separated list of values that fall in amount|source|currency|category|date|created_at|entry_type)
                        // Returns the entries, their sum, their average per month, and their sum-per-category-per-month
                        .route("", web::get().to(handlers::unimplemented))
                        // Parameters: ids
                        .route("update", web::post().to(handlers::unimplemented))
                        // Parameters: ids
                        .route("", web::delete().to(handlers::delete_entries))
                        .route("/archive", web::get().to(handlers::archive_entries)),
                ),
        );

    #[cfg(feature = "create_user")]
    let app = app.route("/register", web::post().to(handlers::create_user));

    app
}
