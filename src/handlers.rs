use std::collections::HashMap;

use ::pbkdf2::Pbkdf2;
use actix_web::{web, HttpRequest, HttpResponse};
use diesel::insert_into;
use diesel::prelude::*;
use password_hash::PasswordHash;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    model::{
        Category, CategoryResponse, CreateCategoryRequest, CreateCurrencyRequest, CreateEntryRequest, CreateSourceRequest, Currency, CurrencyResponse, Entry, EntryQuery, EntryResponse, GetNetAmount, NewCategory, NewCurrency, NewEntry, NewSource, Source, SourceResponse, StatefulTryFrom, StatefulTryFromError, UpdateCategory, UpdateCategoryRequest, UpdateCurrency, UpdateCurrencyRequest, UpdateEntry, UpdateEntryRequest, UpdateSource, UpdateSourceRequest, User
    },
    AppState,
};

#[allow(unused)]
#[derive(Serialize)]
pub struct CreateResponse {
    #[serde(skip_serializing)]
    id: i32,
}

#[derive(Serialize)]
pub struct EmptyResponse {}

#[derive(Serialize)]
pub struct CountResponse {
    count: usize,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
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
            enabled: true,
        });
    let stored_hash = match PasswordHash::new(&user.password) {
        Ok(hash) => hash,
        Err(_) => return HttpResponse::InternalServerError().body("E002: Failed to log in"),
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
pub struct CreateUserRequest {
    username: String,
    password: String,
    currency: String,
}

#[cfg(any(test, feature = "create_user"))]
pub async fn create_user(
    mut data: web::Json<CreateUserRequest>,
    app_state: web::Data<AppState>,
) -> HttpResponse {
    use base64::Engine as _;
    use password_hash::Salt;
    use rand::RngCore as _;

    use crate::model::NewUser;
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
            .values(NewUser { password: hash.to_string(), username: data.username.to_string() })
            .get_result::<User>(&mut app_state.cpool())?;
        insert_into(currencies)
            .values(NewCurrency {
                user_id: user.id,
                name: std::mem::take(&mut data.currency),
                rate_to_fixed: 1.0f64,
                archived: None,
            })
            .execute(&mut app_state.cpool())?;

        user
    };

    match created_user {
        Ok(u) => HttpResponse::Ok().json(CreateResponse { id: u.id }),
        Err(ExternalServiceError::DieselError(_)) => {
            HttpResponse::BadRequest().body("User already exists")
        }
        Err(ExternalServiceError::HashError(_)) => HttpResponse::InternalServerError().body("E001: Failed to create entities"),
    }
}

#[cfg(any(test, feature = "create_user"))]
pub async fn delete_user(
    path_username: web::Path<String>,
    app_state: web::Data<AppState>,
) -> HttpResponse {
    use crate::schema::users::dsl::*;
    let path_username = path_username.into_inner();
    let deleted_count =
        diesel::delete(users.filter(username.eq(path_username)))
            .execute(&mut app_state.cpool());

    match deleted_count {
        Ok(1) => HttpResponse::Ok().json(json!({})),
        Ok(0) => HttpResponse::NotFound().finish(),
        Ok(2..) => HttpResponse::InternalServerError().body("E009: Deleted more than one user"),
        Err(_) => HttpResponse::InternalServerError().body("E008: Failed to delete user"),
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
            let creatable = <$new as StatefulTryFrom<$req>>::stateful_try_from(
                    data.into_inner(),
                    &user.into_inner(),
                    app_state.clone().into_inner(),
                );
            let creatable = match creatable {
                Err(e) => return match e {
                    StatefulTryFromError::ReferencedDoesNotExist(_) =>
                        HttpResponse::BadRequest().body("One of the currencies / categories / sources you referenced does not exist"),
                    StatefulTryFromError::DateTimeParseError(_) =>
                        HttpResponse::BadRequest().body("Malformed date provided - please use YYYY-MM-DD"),
                },
                Ok(c) => c,
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
    CreateCurrencyRequest,
    NewCurrency,
    Currency
);
create_handler!(
    create_source,
    sources,
    CreateSourceRequest,
    NewSource,
    Source
);
create_handler!(
    create_category,
    categories,
    CreateCategoryRequest,
    NewCategory,
    Category
);
create_handler!(create_entry, entries, CreateEntryRequest, NewEntry, Entry);

macro_rules! get_all_handler {
    ($fn_name:ident, $ent:ident, $resp:ident) => {
        pub async fn $fn_name(
            _req: HttpRequest,
            app_state: web::Data<AppState>,
            user: web::ReqData<User>,
        ) -> HttpResponse {
            let user = user.into_inner();
            let app_state = app_state.into_inner();
            let fetched = match $ent::belonging_to(&user)
                .select($ent::as_select())
                .load(&mut app_state.cpool()) {
                    Err(_) => return HttpResponse::InternalServerError().body("E003: Failed to get entities"),
                    Ok(f) => f,
                };
            let responses = fetched.into_iter()
                .map(|f| $resp::stateful_try_from(f, &user, app_state.clone()))
                .collect::<Result<Vec<$resp>, _>>();

            match responses {
                Err(e) => return match e {
                    StatefulTryFromError::ReferencedDoesNotExist(_) =>
                        HttpResponse::BadRequest().body("One of the currencies / categories / sources you referenced does not exist"),
                    StatefulTryFromError::DateTimeParseError(_) =>
                        HttpResponse::BadRequest().body("Malformed date provided - please use YYYY-MM-DD"),
                },
                Ok(c) => HttpResponse::Ok().json(c),
            }
        }
    };
}

get_all_handler!(get_currencies, Currency, CurrencyResponse);
get_all_handler!(get_sources, Source, SourceResponse);
get_all_handler!(get_categories, Category, CategoryResponse);
get_all_handler!(get_entries, Entry, EntryResponse);

pub async fn unimplemented(
    _app_state: web::Data<AppState>,
    _user: web::ReqData<User>,
) -> HttpResponse {
    HttpResponse::NotImplemented().body(format!("Unimplemented."))
}

macro_rules! get_by_name_handler {
    ($fn_name:ident, $tb_name:ident, $ent:ident, $resp:ident) => {
        pub async fn $fn_name(
            path_name: web::Path<String>,
            app_state: web::Data<AppState>,
            user: web::ReqData<User>,
        ) -> HttpResponse {
            use crate::schema::$tb_name::dsl::*;
            let user = user.into_inner();
            let app_state = app_state.into_inner();
            let path_name = path_name.into_inner();
            let fetched = match $ent::belonging_to(&user)
                .filter(name.eq(&path_name))
                .select($ent::as_select())
                .first(&mut app_state.cpool()) {
                    Ok(f) => f,
                    Err(_) => return HttpResponse::NotFound().body("Entity not found"),
                };
            let response = $resp::stateful_try_from(fetched, &user, app_state.clone());
            match response {
                Err(e) => return match e {
                    StatefulTryFromError::ReferencedDoesNotExist(_) => HttpResponse::BadRequest().body("One of the currencies / categories / sources you referenced does not exist"),
                    StatefulTryFromError::DateTimeParseError(_) => HttpResponse::BadRequest().body("Malformed date provided - please use YYYY-MM-DD"),
                },
                Ok(entity) => HttpResponse::Ok().json(entity),
            }
        }
    };
}

get_by_name_handler!(get_currency_by_name, currencies, Currency, CurrencyResponse);
get_by_name_handler!(get_source_by_name, sources, Source, SourceResponse);
get_by_name_handler!(get_category_by_name, categories, Category, CategoryResponse);

#[derive(Debug, Deserialize)]
pub struct BulkRequest {
    ids: Vec<i32>,
}

pub async fn delete_entries(
    web::Query(req): web::Query<BulkRequest>,
    app_state: web::Data<AppState>,
    user: web::ReqData<User>,
) -> HttpResponse {
    use crate::schema::entries::dsl::*;
    let deleted_count =
        diesel::delete(Entry::belonging_to(&user.into_inner()).filter(id.eq_any(&req.ids)))
            .execute(&mut app_state.cpool());

    match deleted_count {
        Ok(count) => HttpResponse::Ok().json(CountResponse { count }),
        Err(_) => HttpResponse::InternalServerError().body("E004: Failed to delete entities"),
    }
}

pub async fn archive_entries(
    web::Query(ids): web::Query<Vec<i32>>,
    app_state: web::Data<AppState>,
    user: web::ReqData<User>,
) -> HttpResponse {
    use crate::schema::entries::dsl::*;
    let updated_count = diesel::update(Entry::belonging_to(&user.into_inner()).filter(id.eq_any(&ids)))
        .set(archived.eq(true)).execute(&mut app_state.cpool());
    match updated_count {
        Ok(count) => HttpResponse::Ok().json(CountResponse { count }),
        Err(_) => HttpResponse::InternalServerError().body("E005: Failed to archive entities"),
    }
}


macro_rules! update_handler {
    ($fn_name:ident, $tb_name:ident, $ent:ident, $changeset:ident, $req:ident) => {
        pub async fn $fn_name(
            path_name: web::Path<String>,
            app_state: web::Data<AppState>,
            data: web::Json<$req>,
            user: web::ReqData<User>,
        ) -> HttpResponse {
            use crate::schema::$tb_name::dsl::*;
            let user = user.into_inner();
            let app_state = app_state.into_inner();
            let path_name = path_name.into_inner();
            let data = data.into_inner();
            let change_set = match $changeset::stateful_try_from(data, &user, app_state.clone()) {
                Err(e) => return match e {
                    StatefulTryFromError::ReferencedDoesNotExist(_) =>
                        HttpResponse::BadRequest().body("One of the currencies / categories / sources you referenced does not exist"),
                    StatefulTryFromError::DateTimeParseError(_) =>
                        HttpResponse::BadRequest().body("Malformed date provided - please use YYYY-MM-DD"),
                },
                Ok(c) => c,
            };
            let count: usize = match diesel::update($ent::belonging_to(&user)
                .filter(name.eq(&path_name)))
                .set(change_set)
                .execute(&mut app_state.cpool()) {
                    Ok(f) => f,
                    Err(_) => return HttpResponse::NotFound().body("Entity not found"),
                };
            HttpResponse::Ok().json(CountResponse { count })
        }
    };
}

update_handler!(update_currency, currencies, Currency, UpdateCurrency, UpdateCurrencyRequest);
update_handler!(update_source, sources, Source, UpdateSource, UpdateSourceRequest);
update_handler!(update_category, categories, Category, UpdateCategory, UpdateCategoryRequest);

macro_rules! archive_handler {
    ($fn_name:ident, $tb_name:ident, $ent:ident, $err:expr) => {
        pub async fn $fn_name(
            path_name: web::Path<String>,
            app_state: web::Data<AppState>,
            user: web::ReqData<User>,
        ) -> HttpResponse {
            use crate::schema::$tb_name::dsl::*;
            let user = user.into_inner();
            let app_state = app_state.into_inner();
            let path_name = path_name.into_inner();
            let fetched = match $ent::belonging_to(&user)
            .filter(name.eq(&path_name))
            .first::<$ent>(&mut app_state.cpool()) {
                Ok(f) => f,
                Err(_) => return HttpResponse::NotFound().body("Entity not found"),
            };
            let net_amount = match fetched.get_net_amount(app_state.clone()) {
                Ok(t) => t,
                Err(_) => return HttpResponse::InternalServerError().body("E006: Unable to construct sum - failed to archive"),
            };
            if net_amount != 0f64 {
                return HttpResponse::BadRequest().body($err);
            }
            let count: usize = match diesel::update(&fetched)
                .set(archived.eq(true))
                .execute(&mut app_state.cpool()) {
                    Ok(f) => f,
                    Err(_) => return HttpResponse::NotFound().body("Entity not found"),
                };
            HttpResponse::Ok().json(CountResponse { count })
        }
    };
}

// TODO: FE should send another GET request for sources to display:
//  The balance exists in the following sources: <_>
archive_handler!(archive_currency, currencies, Currency, "\
    You cannot archive that currency while you still have balance within it.\
");
archive_handler!(archive_source, sources, Source, "\
    You cannot archive that source while it still has balance. \
    You can transfer all balance to another source of the same currency or \
    do a currency conversion to a different source of a different currency. \
");
archive_handler!(archive_category, categories, Category, "\
    You cannot archive that category while it has entries. \
    You can transfer all entries to another category and then proceed.\
");

pub async fn find_entries(
    query_params: web::Query<EntryQuery>,
    app_state: web::Data<AppState>,
    user: web::ReqData<User>,
) -> HttpResponse {
    let query_params = query_params.into_inner();
    let user = user.into_inner();
    let app_state = app_state.into_inner();

    match Entry::find_by_filter(&query_params, &user, app_state) {
        Ok(entries) => {
            let sum_amounts: f64 = entries.iter().map(|entry| entry.amount).sum();


            let mut sum_per_month: HashMap<String, f64> = HashMap::new();
            for entry in &entries {
                let month_year = entry.date.format("%Y-%m").to_string();
                *sum_per_month.entry(month_year).or_insert(0.0) += entry.amount;
            }
            let num_months = sum_per_month.len() as f64;
            let avg_per_month = sum_amounts / num_months;

            let mut sum_per_category_per_month: HashMap<(i32, String), f64> = HashMap::new();
            for entry in &entries {
                let month_year = entry.date.format("%Y-%m").to_string();
                let category_month_key = (entry.category_id.clone(), month_year.clone());
                *sum_per_category_per_month.entry(category_month_key).or_insert(0.0) +=
                    entry.amount;
            }

            HttpResponse::Ok().json(serde_json::json!({
                "sum_per_month": sum_per_month,
                "avg_per_month": avg_per_month,
                "sum_per_category_per_month": sum_per_category_per_month,
                "entries": entries,
            }))
        }
        Err(_) => {
            HttpResponse::InternalServerError().body("E007: Error finding entries")
        }
    }
}

// TODO: Work on BE of filtering, searching, bulk editing, and displaying required for FE
// TODO: Log errors in stderr in case of internal server errors

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

# General front-end
- Tables
- Printing

TODO at the very end: Look into diesel async, whcih would only require adding .await after each cpool() execute / load.
Look into the 3 other pooling crates other than r2d2.
*/
