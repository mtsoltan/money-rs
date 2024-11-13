use std::collections::HashMap;
use std::fmt::Debug;

use ::pbkdf2::Pbkdf2;
use actix_web::{web, HttpRequest, HttpResponse};
use diesel::insert_into;
use diesel::prelude::*;
use password_hash::PasswordHash;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::consts;
use crate::http::{internal, ArrayQuery};
#[allow(unused_imports)]
use crate::{
    model::{
        Category, CategoryResponse, CreateCategoryRequest, CreateCurrencyRequest,
        CreateEntryRequest, CreateSourceRequest, Currency, CurrencyResponse, Entry, EntryQuery,
        EntryResponse, GetNetAmount, HasSpecifier, NewCategory, NewCurrency, NewEntry, NewSource,
        Source, SourceResponse, StatefulTryFrom, StatefulTryFromError, UpdateCategory,
        UpdateCategoryRequest, UpdateCurrency, UpdateCurrencyRequest, UpdateEntry,
        UpdateEntryRequest, UpdateSource, UpdateSourceRequest, User,
    },
    AppState,
};
// We cannot skip serialization in any of the fields in the response, as in the tests,
// we will need to reconstruct the response from the JSON string to reason about it,
// in order to not have to write code that uses maps.
//
// The exception is CreateResponse, which serializes as empty response.

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateResponse {
    #[allow(dead_code)]
    #[serde(skip_serializing)]
    pub id: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmptyResponse {}

/// Used only when performing group-operation on entries (not entities).
/// Examples are deleting and archiving and block-updating entries.
#[derive(Debug, Serialize, Deserialize)]
pub struct CountResponse {
    pub count: usize,
}

#[derive(thiserror::Error, Debug)]
pub enum ExternalServiceError {
    #[error("Failed to generate password hash")]
    HashError(password_hash::Error),
    #[error("Diesel operation resulted in an error")]
    DieselError(#[from] diesel::result::Error),
}

impl From<password_hash::Error> for ExternalServiceError {
    fn from(value: password_hash::Error) -> Self { Self::HashError(value) }
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

    let user = items.pop().unwrap_or(User {
        id: 0,
        username: "".to_string(),
        // Some random hash to ensure hash comparison runs even if user does not exist,
        // preventing timing attacks.
        password: "$pbkdf2-sha256$i=600000,l=32$XpabVnRzlUG8YOvL$/\
                   rdEfUzDwQOBJBCfmc6P3DrbJDo13IrrY+6/O087CSI"
            .to_string(),
        fixed_currency_id: None,
        enabled: true,
    });
    let stored_hash = match PasswordHash::new(&user.password) {
        Ok(hash) => hash,
        Err(e) => return internal(e, "E002: Failed to log in"),
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
            })
            .execute(&mut app_state.cpool())?;

        user
    };

    match created_user {
        Ok(u) => HttpResponse::Ok().json(CreateResponse { id: u.id }),
        Err(ExternalServiceError::DieselError(e)) => internal(e, "User already exists"),
        Err(ExternalServiceError::HashError(e)) => internal(e, "E001: Failed to create entities"),
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
        diesel::delete(users.filter(username.eq(path_username))).execute(&mut app_state.cpool());

    match deleted_count {
        Ok(1) => HttpResponse::Ok().json(EmptyResponse {}),
        Ok(0) => HttpResponse::NotFound().finish(),
        Ok(2..) => internal("No underlying error", "E009: Deleted more than one user"),
        Err(e) => internal(e, "E008: Failed to delete user"),
    }
}

impl From<StatefulTryFromError> for HttpResponse {
    fn from(error: StatefulTryFromError) -> HttpResponse {
        HttpResponse::BadRequest().body(error.to_string())
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
                Err(e) => return HttpResponse::from(e),
                Ok(c) => c,
            };
            let created =
                insert_into($tb_name).values(creatable).get_result::<$ent>(&mut app_state.cpool());
            match created {
                Ok(c) => HttpResponse::Ok().json(CreateResponse { id: c.id }),
                Err(e) => {
                    if matches!(
                        e,
                        diesel::result::Error::DatabaseError(
                            diesel::result::DatabaseErrorKind::UniqueViolation,
                            _
                        )
                    ) {
                        HttpResponse::BadRequest()
                            .body(format!("{} already exists", <$ent>::specifier()))
                    } else {
                        internal(e, format!("E014: Failed to create {}", <$ent>::specifier()))
                    }
                }
            }
        }
    };
}

create_handler!(create_currency, currencies, CreateCurrencyRequest, NewCurrency, Currency);
create_handler!(create_source, sources, CreateSourceRequest, NewSource, Source);
create_handler!(create_category, categories, CreateCategoryRequest, NewCategory, Category);
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
                .load(&mut app_state.cpool())
            {
                Err(e) => {
                    return internal(
                        e,
                        format!("E003: Failed to get all {}", $ent::specifier_plural()).as_str(),
                    )
                }
                Ok(f) => f,
            };
            let responses = fetched
                .into_iter()
                .map(|f| $resp::stateful_try_from(f, &user, app_state.clone()))
                .collect::<Result<Vec<$resp>, _>>();

            match responses {
                Err(e) => HttpResponse::from(e),
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
    HttpResponse::NotImplemented().body("Unimplemented.".to_string())
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
                .first(&mut app_state.cpool())
            {
                Ok(f) => f,
                Err(e) => {
                    if matches!(e, diesel::result::Error::NotFound) {
                        return HttpResponse::NotFound()
                            .body(format!("{} not found", $ent::specifier()));
                    } else {
                        return internal(
                            e,
                            format!("E015: Failed to get {} by name", <$ent>::specifier()),
                        );
                    }
                }
            };
            let response = $resp::stateful_try_from(fetched, &user, app_state.clone());
            match response {
                Err(e) => HttpResponse::from(e),
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
    ArrayQuery(req): ArrayQuery<BulkRequest>,
    app_state: web::Data<AppState>,
    user: web::ReqData<User>,
) -> HttpResponse {
    use crate::schema::entries::dsl::*;
    let deleted_count =
        diesel::delete(Entry::belonging_to(&user.into_inner()).filter(id.eq_any(&req.ids)))
            .execute(&mut app_state.cpool());

    match deleted_count {
        Ok(count) => HttpResponse::Ok().json(CountResponse { count }),
        Err(e) => internal(e, "E004: Failed to delete entities"),
    }
}

pub async fn archive_entries(
    ArrayQuery(req): ArrayQuery<BulkRequest>,
    app_state: web::Data<AppState>,
    user: web::ReqData<User>,
) -> HttpResponse {
    use crate::schema::entries::dsl::*;
    let updated_count =
        diesel::update(Entry::belonging_to(&user.into_inner()).filter(id.eq_any(&req.ids)))
            .set(archived.eq(true))
            .execute(&mut app_state.cpool());
    match updated_count {
        Ok(count) => HttpResponse::Ok().json(CountResponse { count }),
        Err(e) => internal(e, "E005: Failed to archive entities"),
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
                Err(e) => return HttpResponse::from(e),
                Ok(c) => c,
            };
            match diesel::update($ent::belonging_to(&user).filter(name.eq(&path_name)))
                .set(change_set)
                .execute(&mut app_state.cpool())
            {
                Ok(1) => HttpResponse::Ok().json(EmptyResponse {}),
                Ok(0) => HttpResponse::NotFound().finish(),
                Ok(2..) => internal(
                    "No underlying error",
                    format!("E010: Updated more than one {}", $ent::specifier()),
                ),
                Err(e) => {
                    return internal(e, format!("E011: Could not update {}", $ent::specifier()))
                }
            }
        }
    };
}

update_handler!(update_currency, currencies, Currency, UpdateCurrency, UpdateCurrencyRequest);
update_handler!(update_source, sources, Source, UpdateSource, UpdateSourceRequest);
update_handler!(update_category, categories, Category, UpdateCategory, UpdateCategoryRequest);

/// To un-archive, we update with `{ "archived": false }`
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
                .first::<$ent>(&mut app_state.cpool())
            {
                Ok(f) => f,
                Err(_) => {
                    return HttpResponse::NotFound().body(format!("{} not found", $ent::specifier()))
                }
            };
            let net_amount = match fetched.get_net_amount(app_state.clone()) {
                Ok(t) => t,
                Err(e) => return internal(e, "E006: Unable to construct sum - failed to archive"),
            };
            if (net_amount - 0f64).abs() > consts::EPSILON {
                return HttpResponse::BadRequest().body($err);
            }
            match diesel::update(&fetched).set(archived.eq(true)).execute(&mut app_state.cpool()) {
                Ok(1) => HttpResponse::Ok().json(EmptyResponse {}),
                Ok(0) => HttpResponse::NotFound().finish(),
                Ok(2..) => internal(
                    "No underlying error",
                    format!("E012: Archived more than one {}", $ent::specifier()).as_str(),
                ),
                Err(e) => {
                    internal(e, format!("E013: Could not archive {}", $ent::specifier()).as_str())
                }
            }
        }
    };
}

// TODO(15): ENDPOINT: /currency/{name}/sources - sources with balance in a country because: FE
//  should send another GET request for sources to display:  The balance exists in the following
//  sources: <_>
archive_handler!(
    archive_currency,
    currencies,
    Currency,
    "You cannot archive that currency while you still have balance within it."
);
archive_handler!(
    archive_source,
    sources,
    Source,
    "You cannot archive that source while it still has balance. You can transfer all balance to \
     another source of the same currency or do a currency conversion to a different source of a \
     different currency. "
);
archive_handler!(
    archive_category,
    categories,
    Category,
    "You cannot archive that category while it has entries. You can transfer all entries to \
     another category and then proceed."
);

pub async fn find_entries(
    ArrayQuery(query_params): ArrayQuery<EntryQuery>,
    app_state: web::Data<AppState>,
    user: web::ReqData<User>,
) -> HttpResponse {
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

            let mut sum_per_category_per_month: HashMap<String, f64> = HashMap::new();
            for entry in &entries {
                let month_year = entry.date.format("%Y-%m").to_string();
                let category_month_key =
                    format!("{}|{}", entry.category_id.clone(), month_year.clone());
                *sum_per_category_per_month.entry(category_month_key).or_insert(0.0) +=
                    entry.amount;
            }

            // TODO(09): STRUCTURE: Replace me with a proper response struct
            HttpResponse::Ok().json(json!({
                "sum_per_month": sum_per_month,
                "avg_per_month": avg_per_month,
                "sum_per_category_per_month": sum_per_category_per_month,
                "entries": entries,
            }))
        }
        Err(e) => internal(e, "E007: Error finding entries"),
    }
}

// TODO(20): DESIGN: Work on BE of filtering, searching, bulk editing, and displaying required for
//  FE

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
- TODO(80): DESIGN: decide the rest of categories functionality

# Currencies functionality
- Change display currency (for all of the above) - defaults to the fixed currency of the user
- TODO(80): DESIGN: decide the rest of currencies functionality

# Sources functionality
- TODO(80): DESIGN: decide the rest of sources functionality

# General front-end
- Tables
- Printing

TODO(75): STRUCTURE: at the very end: Look into diesel async, which would only require adding .await after each cpool() execute / load.
Look into the 3 other pooling crates other than r2d2.

TODO(70): EXTRA: Automatic price fetching from an online API
TODO(70): EXTRA: Automatic tagging of entries:
  - allow a box for amount + currency (prefix / suffix) and a dropdown for currency - locked if typed inside the box
  - box placeholder should have currency as prefix
  - Third input is for description, with an AI button beside it, that when tapped will try to fill all the remaining inputs from AI
  - This input should have autocomplete from existing ones (combo box like)
  Automatically tag:
  - entry type - deduce from description
  - category - deduce from description
  - source id - deduce from description
  - secondary source id - deduce from description
  - date if specified in description, otherwise current date
  - description (updated to no longer have category, date, and entry type),
  Deduction from description works by trying to match to an existing description in database (by strict matching, or asking an LLM),
  and if not, by asking an LLM to come up with something of its own
*/
