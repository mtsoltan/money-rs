use ::pbkdf2::Pbkdf2;
use actix_web::{web, HttpRequest, HttpResponse};
use base64::Engine as _;
use diesel::{insert_into, prelude::*};
use password_hash::{PasswordHash, Salt};
use rand::RngCore as _;
use serde::{Deserialize, Serialize};

use crate::{
    env_vars::database_url,
    model::{NewUser, User},
    AppState,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    token: String,
}

pub async fn login(data: web::Json<LoginRequest>, app_state: web::Data<AppState>) -> HttpResponse {
    use crate::schema::users::dsl::*;
    let mut err = 0;

    let mut items = users
        .filter(username.eq(&data.username))
        .load::<User>(&mut app_state.cpool())
        .unwrap_or(vec![]); // TODO: Use this or establish connection? - &mut app_state.pool.get().unwrap()

    let user = items.pop().unwrap_or_default();
    let stored_hash = PasswordHash::new(&user.password).expect("invalid password hash");

    if items.is_empty() {
        err += 1;
    }
    if items.len() > 1 {
        err += 1;
    }

    match stored_hash.verify_password(&[&Pbkdf2], &data.password) {
        Ok(_) => {}
        Err(_) => {
            err += 1;
        }
    };
    if err > 0 {
        HttpResponse::Unauthorized().body("Unauthorized")
    } else {
        let token = crate::authentication::generate(user.id);
        HttpResponse::Ok().json(LoginResponse { token })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserRequest {
    username: String,
    password: String,
    currency: String,
}

#[derive(Serialize)]
pub struct CreateUserResponse {}

pub async fn create_user(
    data: web::Json<UserRequest>,
    app_state: web::Data<AppState>,
) -> HttpResponse {
    use crate::schema::users::dsl::*;
    let mut bytes: [u8; 12] = [0; 12];
    // Salt::RECOMMENDED_LENGTH would fail because of equal signs.
    // See https://docs.rs/password-hash/latest/src/password_hash/salt.rs.html#122
    rand::thread_rng().fill_bytes(&mut bytes);
    let base64_string = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let generated_salt =
        Salt::from_b64(base64_string.as_str()).expect("Salt construction should work");
    let maybe_hash = PasswordHash::generate(Pbkdf2, &data.password.as_bytes(), generated_salt);
    match maybe_hash {
        Err(_) => HttpResponse::InternalServerError().finish(),
        Ok(hash) => {
            match insert_into(users)
                .values(NewUser {
                    password: dbg!(hash.to_string()),
                    username: data.username.to_string(),
                })
                .execute(&mut app_state.cpool())
            {
                Ok(_) => HttpResponse::Ok().json(CreateUserResponse {}),
                Err(_) => HttpResponse::InternalServerError().finish(),
            }
        }
    }
}
