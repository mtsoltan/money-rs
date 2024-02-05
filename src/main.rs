mod authentication;
mod env_vars;
mod handlers;
mod model;
mod schema;

use actix_web::{web, App, HttpServer};
use actix_web_httpauth::middleware::HttpAuthentication;
use diesel::{r2d2::ConnectionManager, PgConnection};
use handlers::{create_user, login};

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
        .expect("Failed to create pool.");
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(AppState { pool: pool.clone() }))
            .route("/login", web::post().to(login))
            .route("/create", web::post().to(create_user))
            .service(web::scope("/api").wrap(HttpAuthentication::bearer(
                authentication::jwt_validator_generator,
            )))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
