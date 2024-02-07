#![feature(try_blocks)]

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
        .expect("Failed to create pool");
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
                    web::scope("/currency").route("", web::post().to(handlers::create_currency)),
                )
                .service(web::scope("/source").route("", web::post().to(handlers::create_source)))
                .service(
                    web::scope("/category").route("", web::post().to(handlers::create_category)),
                ),
        );

    #[cfg(feature = "create_user")]
    let app = app.route("/api/user", web::post().to(handlers::create_user));

    app
}
