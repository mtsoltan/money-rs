use ::pbkdf2::Pbkdf2;
use actix_web::{web, HttpRequest, HttpResponse};
use diesel::{insert_into, prelude::*};
use password_hash::PasswordHash;
use serde::{Deserialize, Serialize};

use crate::{
    model::{
        Category, CategoryRequest, Currency, CurrencyRequest, Entry, EntryRequest, NewCategory,
        NewCurrency, NewEntry, NewSource, Source, SourceRequest, User,
    },
    AppState,
};

#[derive(Serialize)]
pub struct CreateResponse {
    id: i32,
}

pub enum ExternalServiceError {
    HashError(password_hash::Error),
    DieselError(diesel::result::Error),
}

impl From<password_hash::Error> for ExternalServiceError {
    fn from(value: password_hash::Error) -> Self {
        Self::HashError(value)
    }
}

impl From<diesel::result::Error> for ExternalServiceError {
    fn from(value: diesel::result::Error) -> Self {
        Self::DieselError(value)
    }
}

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
        .unwrap_or(vec![]);

    if items.is_empty() {
        err += 1;
    }

    let user = items.pop().unwrap_or(
        User {
            id: 0,
            username: format!(""),
            // Some random hash to ensure hash comparison runs even if user does not exist,
            // preventing timing attacks.
            password: format!("$pbkdf2-sha256$i=600000,l=32$XpabVnRzlUG8YOvL$/rdEfUzDwQOBJBCfmc6P3DrbJDo13IrrY+6/O087CSI"),
            fixed_currency_id: None,
        });
    let stored_hash = match PasswordHash::new(&user.password) {
        Ok(hash) => hash,
        Err(_) => return HttpResponse::InternalServerError().body("E002"),
    };

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

#[cfg(feature = "create_user")]
pub async fn create_user(
    mut data: web::Json<UserRequest>,
    app_state: web::Data<AppState>,
) -> HttpResponse {
    use crate::model::NewUser;
    use base64::Engine as _;
    use password_hash::Salt;
    use rand::RngCore as _;
    let created_user: Result<User, ExternalServiceError> = try {
        use crate::schema::currencies::dsl::*;
        use crate::schema::users::dsl::*;
        // Salt::RECOMMENDED_LENGTH would fail because of equal signs.
        // See https://docs.rs/password-hash/latest/src/password_hash/salt.rs.html#122
        let mut bytes: [u8; 12] = [0; 12];
        rand::thread_rng().fill_bytes(&mut bytes);
        let base64_string = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let generated_salt =
            Salt::from_b64(base64_string.as_str()).expect("Salt construction should work");
        let hash = PasswordHash::generate(Pbkdf2, &data.password.as_bytes(), generated_salt)?;

        let user = insert_into(users)
            .values(NewUser {
                password: hash.to_string(),
                username: data.username.to_string(),
            })
            .get_result::<User>(&mut app_state.cpool())?;
        insert_into(currencies)
            .values(NewCurrency {
                user_id: user.id,
                name: std::mem::take(&mut data.currency),
                rate_to_fixed: 1.0f64,
            })
            .execute(&mut app_state.cpool())?;

        user
    };

    match created_user {
        Ok(u) => HttpResponse::Ok().json(CreateResponse { id: u.id }),
        Err(ExternalServiceError::DieselError(_)) => {
            HttpResponse::BadRequest().body("User already exists")
        }
        Err(ExternalServiceError::HashError(_)) => HttpResponse::InternalServerError().body("E001"),
    }
}

macro_rules! create {
    ($fn_name:ident, $tb_name:ident, $req:ty, $new:ident, $ent:ty) => {
        pub async fn $fn_name(
            _req: HttpRequest,
            data: web::Json<$req>,
            app_state: web::Data<AppState>,
            user: web::ReqData<User>,
        ) -> HttpResponse {
            dbg!(&data);
            use crate::schema::$tb_name::dsl::*;
            let created = insert_into($tb_name)
                .values($new::from_request(data.into_inner(), user.into_inner()))
                .get_result::<$ent>(&mut app_state.cpool());
            match created {
                Ok(c) => HttpResponse::Ok().json(CreateResponse { id: c.id }),
                Err(_) => HttpResponse::BadRequest().body("Entity already exists"),
            }
        }
    };
}

create!(
    create_currency,
    currencies,
    CurrencyRequest,
    NewCurrency,
    Currency
);
create!(create_source, sources, SourceRequest, NewSource, Source);
create!(
    create_category,
    categories,
    CategoryRequest,
    NewCategory,
    Category
);
