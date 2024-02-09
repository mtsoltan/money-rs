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
    #[serde(skip_serializing)]
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

macro_rules! create_handler {
    ($fn_name:ident, $tb_name:ident, $req:ty, $new:ident, $ent:ty) => {
        pub async fn $fn_name(
            _req: HttpRequest,
            data: web::Json<$req>,
            app_state: web::Data<AppState>,
            user: web::ReqData<User>,
        ) -> HttpResponse {
            use crate::schema::$tb_name::dsl::*;
            use crate::model::{TryFromRequest, TryFromRequestError};
            let creatable = <$new as TryFromRequest<$req>>::try_from_request(
                    data.into_inner(),
                    user.into_inner(),
                    app_state.clone().into_inner(),
                );
            let creatable = match creatable {
                Err(e) => return match e {
                    TryFromRequestError::ReferencedDoesNotExist(_) => HttpResponse::BadRequest().body("One of the currencies / categories / sources you referenced does not exist"),
                    TryFromRequestError::DateTimeParseError(_) => HttpResponse::BadRequest().body("Malformed date provided"),
                },
                Ok(c) => c
            };
            let created = insert_into($tb_name)
                .values(creatable)
                .get_result::<$ent>(&mut app_state.cpool());
            match created {
                Ok(c) => HttpResponse::Ok().json(CreateResponse { id: c.id }),
                Err(_) => HttpResponse::BadRequest().body("Entity already exists"),
            }
        }
    };
}

create_handler!(
    create_currency,
    currencies,
    CurrencyRequest,
    NewCurrency,
    Currency
);
create_handler!(create_source, sources, SourceRequest, NewSource, Source);
create_handler!(
    create_category,
    categories,
    CategoryRequest,
    NewCategory,
    Category
);
create_handler!(create_entry, entries, EntryRequest, NewEntry, Entry);

macro_rules! get_all_handler {
    ($fn_name:ident, $ent:ident) => {
        pub async fn $fn_name(
            _req: HttpRequest,
            app_state: web::Data<AppState>,
            user: web::ReqData<User>,
        ) -> HttpResponse {
            let fetched = $ent::belonging_to(&user.into_inner())
                .select($ent::as_select())
                .load(&mut app_state.cpool());
            match fetched {
                Ok(f) => HttpResponse::Ok().json(f),
                Err(_) => HttpResponse::InternalServerError().body("E003"),
            }
        }
    };
}

get_all_handler!(get_currencies, Currency);
get_all_handler!(get_sources, Source);
get_all_handler!(get_categories, Category);
get_all_handler!(get_entries, Entry);

macro_rules! get_by_name_handler {
    ($fn_name:ident, $tb_name:ident, $ent:ident) => {
        pub async fn $fn_name(
            path_name: web::Path<String>,
            app_state: web::Data<AppState>,
            user: web::ReqData<User>,
        ) -> HttpResponse {
            use crate::schema::$tb_name::dsl::*;
            let fetched = $ent::belonging_to(&user.into_inner())
                .filter(name.eq(&path_name.into_inner()))
                .select($ent::as_select())
                .first(&mut app_state.cpool());
            match fetched {
                Ok(entity) => HttpResponse::Ok().json(entity),
                Err(_) => HttpResponse::NotFound().body("Entity not found"),
            }
        }
    };
}

get_by_name_handler!(get_currency_by_name, currencies, Currency);
get_by_name_handler!(get_source_by_name, sources, Source);
get_by_name_handler!(get_category_by_name, categories, Category);

// TODO: Add support for archival of currencies, sources, categories with balance of 0.
//  Error message on attempted archival of non-zero currency:
//  You cannot archive that currency while you still have balance within it. The balance exists in the following sources: <_>
//  Error message on attempted archival of non-zero source:
//  You cannot archive that source while it still has balance. You can transfer all balance to another source of the same currency
//  or do a currency conversion to a different source of a different currency.
//  Error message on attempted archival of non-zero category:
//  You cannot archive that category while it has entries. You can transfer all entries to another category and then proceed.

/*
Front end should allow:

# Entries functionality
- Listing of entries
- Filtering of entries based on source (including secondary source) / category / currency / entry type
- Filtering of entries based on amount (gte / lte / eq)
- Filtering of entries based on date (gte / lte / eq) (can quick select a month or a year)
- Search of entries based on description
- Multi-selecting entries, with select all that selects all entries in the search / filter.
- Sort based on any field
- Displays sum of selected entries (all entries if none selected)
- Displays average per month of selected entries
- Displays sum per category per month of selected entries
- Bulk editing of selected entries (can change category / description / currency / source / secondary source / entry type)
- Editing of individual entries (allows changing the above, and conversion rate, date and amount)
- Archival / deletion of entries
- Creation of new entries

# Categories functionality
- Monthly sum of entries for this category
- TODO

# Currencies functionality
- Change display currency (for all of the above) - defaults to the fixed currency of the user
- TODO

# Sources functionality
- TODO
*/