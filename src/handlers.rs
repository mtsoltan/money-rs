use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

use ::pbkdf2::Pbkdf2;
use actix_web::{HttpRequest, HttpResponse, web};
use diesel::query_builder::BoxedSelectStatement;
use diesel::query_dsl::methods::{FilterDsl, LimitDsl, OffsetDsl, OrderDsl, SelectDsl};
use diesel::{
    BelongingToDsl, BoolExpressionMethods, ExpressionMethods, SelectableHelper, insert_into,
};
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::{AsyncConnection, RunQueryDsl as _};
use futures::future::join_all;
use itertools::Itertools;
use log::error;
use password_hash::PasswordHash;
use serde::{Deserialize, Serialize};

use crate::consts;
use crate::consts::Conn;
use crate::env_vars::page_size;
use crate::http::{ArrayQuery, internal};
use crate::model::{EntryType, GetById};
#[allow(unused_imports)]
use crate::{
    AppState,
    model::{
        Category, CategoryResponse, CreateCategoryRequest, CreateCurrencyRequest,
        CreateEntryRequest, CreateSourceRequest, Currency, CurrencyResponse, Entry, EntryQuery,
        EntryResponse, GetNetAmount, HasSpecifier, NewCategory, NewCurrency, NewEntry, NewSource,
        Source, SourceResponse, StatefulTryFrom, StatefulTryFromError, UpdateCategory,
        UpdateCategoryRequest, UpdateCurrency, UpdateCurrencyRequest, UpdateEntry,
        UpdateEntryRequest, UpdateSource, UpdateSourceRequest, User,
    },
};

// We cannot skip serialization in any of the fields in the response, as in the tests,
// we will need to reconstruct the response from the JSON string to reason about it,
// to not have to write code that uses maps.
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimplePaginatedRequest {
    page: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEntriesRequest {
    page: Option<u32>,
    primary_only: Option<bool>,
}

#[cfg(any(test, feature = "create_user"))]
#[derive(thiserror::Error, Debug)]
pub enum ExternalServiceError {
    #[error("Failed to generate password hash")]
    HashError(password_hash::Error),
    #[error("Diesel operation resulted in an error")]
    DieselError(#[from] diesel::result::Error),
}

#[cfg(any(test, feature = "create_user"))]
impl From<password_hash::Error> for ExternalServiceError {
    fn from(value: password_hash::Error) -> Self { Self::HashError(value) }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FindEntriesResponse {
    pub sum_per_month: HashMap<String, f64>,
    pub monthly_average: f64,
    pub sum_per_category_per_month: HashMap<String, f64>,
    pub entries: Vec<EntryResponse>,
}

pub async fn login(data: web::Json<LoginRequest>, app_state: web::Data<AppState>) -> HttpResponse {
    use crate::schema::users::dsl::*;
    let mut err = 0;

    let mut items = users
        .filter(username.eq(&data.username))
        .load::<User>(&mut app_state.cpool().await)
        .await
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

#[cfg(any(test, feature = "create_user"))]
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
        rand::rng().fill_bytes(&mut bytes);
        let base64_string = base64::engine::general_purpose::STANDARD.encode(bytes);
        let generated_salt =
            Salt::from_b64(base64_string.as_str()).expect("Salt construction should work");
        let hash = PasswordHash::generate(Pbkdf2, data.password.as_bytes(), generated_salt)?;

        let user = insert_into(users)
            .values(NewUser { password: hash.to_string(), username: data.username.to_string() })
            .get_result::<User>(&mut app_state.cpool().await)
            .await?;
        insert_into(currencies)
            .values(NewCurrency {
                user_id: user.id,
                name: std::mem::take(&mut data.currency),
                // IEEE-754 float64 multiplication by 1 is always exact.
                rate_to_fixed: 1.0f64,
                archived: None,
            })
            .execute(&mut app_state.cpool().await)
            .await?;

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
    let deleted_count = diesel::delete(users.filter(username.eq(path_username)))
        .execute(&mut app_state.cpool().await)
        .await;

    match deleted_count {
        Ok(1) => HttpResponse::Ok().json(EmptyResponse {}),
        Ok(0) => HttpResponse::NotFound().finish(),
        Ok(2..) => internal("No underlying error", "E009: Deleted more than one user"),
        Err(e) => internal(e, "E008: Failed to delete user"),
    }
}

impl From<StatefulTryFromError> for HttpResponse {
    fn from(error: StatefulTryFromError) -> HttpResponse {
        match error {
            StatefulTryFromError::LogicInternalError(_) => internal(error, "LogicInternalError"),
            _ => HttpResponse::BadRequest().body(error.to_string()),
        }
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
            )
            .await;
            let creatable = match creatable {
                Err(e) => return HttpResponse::from(e),
                Ok(c) => c,
            };
            let created = insert_into($tb_name)
                .values(creatable)
                .get_result::<$ent>(&mut app_state.cpool().await)
                .await;
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

pub async fn create_entry(
    _req: HttpRequest,
    data: web::Json<CreateEntryRequest>,
    app_state: web::Data<AppState>,
    user: web::ReqData<User>,
) -> HttpResponse {
    use crate::schema::entries::dsl;
    let app_state = app_state.into_inner();
    let creatable = <NewEntry as StatefulTryFrom<CreateEntryRequest>>::stateful_try_from(
        data.into_inner(),
        &user.into_inner(),
        app_state.clone(),
    )
    .await;
    let creatable = match creatable {
        Err(e) => return HttpResponse::from(e),
        Ok(c) => c,
    };

    let conn = &mut app_state.cpool().await;

    enum TransactionError {
        // BadRequest gives a static error message that overlooks the underlying error due to
        // check
        #[allow(dead_code)]
        BadRequestAlreadyExists(diesel::result::Error),
        InternalE014(diesel::result::Error),
        InternalE018(UpdateEntrySourcesError, String),
        TransactionError(diesel::result::Error),
    }

    impl From<diesel::result::Error> for TransactionError {
        fn from(value: diesel::result::Error) -> Self { TransactionError::TransactionError(value) }
    }

    match conn
        .transaction(|tx| {
            async move {
                let created =
                    insert_into(dsl::entries).values(creatable).get_result::<Entry>(tx).await;

                let created = match created {
                    Ok(c) => c,
                    Err(e) => {
                        return if matches!(
                            e,
                            diesel::result::Error::DatabaseError(
                                diesel::result::DatabaseErrorKind::UniqueViolation,
                                _
                            )
                        ) {
                            Err(TransactionError::BadRequestAlreadyExists(e))
                        } else {
                            Err(TransactionError::InternalE014(e))
                        };
                    }
                };
                let source_result = update_entry_sources(
                    &created,
                    app_state.clone(),
                    tx,
                    UpdateEntrySourcesType::Create,
                )
                .await;
                match source_result {
                    Err(e) => {
                        let (eid, sid) = match e {
                            UpdateEntrySourcesError::NoSource { entry_id, source_id, .. } => {
                                (entry_id, source_id)
                            }
                            UpdateEntrySourcesError::MalformedEntry { entry_id, source_id } => {
                                (entry_id, source_id)
                            }
                            UpdateEntrySourcesError::UpdateError {
                                entry_id, source_id, ..
                            } => (entry_id, source_id),
                        };
                        let error_string = format!(
                            "E018: Successfully created {} {}, but failed to get {} {:?} for it",
                            Entry::specifier(),
                            eid,
                            Source::specifier(),
                            sid,
                        );
                        Err(TransactionError::InternalE018(e, error_string))
                    }
                    Ok(_) => Ok(CreateResponse { id: created.id }),
                }
            }
            .scope_boxed()
        })
        .await
    {
        Err(e) => match e {
            TransactionError::BadRequestAlreadyExists(_) => {
                HttpResponse::BadRequest().body(format!("{} already exists", Entry::specifier()))
            }
            TransactionError::InternalE014(e) => {
                internal(e, format!("E014: Failed to create {}", Entry::specifier()))
            }
            TransactionError::InternalE018(e, error_string) => internal(e, error_string),
            TransactionError::TransactionError(e) => {
                internal(e, "E019: Internal transaction error")
            }
        },
        Ok(e) => HttpResponse::Ok().json(e),
    }
}

macro_rules! get_all_handler {
    ($fn_name:ident, $ent:ident, $resp:ident, $order:expr) => {
        pub async fn $fn_name(
            web::Query(req): web::Query<SimplePaginatedRequest>,
            app_state: web::Data<AppState>,
            user: web::ReqData<User>,
        ) -> HttpResponse {
            let user = user.into_inner();
            let app_state = app_state.into_inner();

            // Boxing the query allows us to mutate it without changing its type.
            let mut query = diesel::QueryDsl::into_boxed(
                $ent::belonging_to(&user).select($ent::as_select()).order($order),
            );
            if let Some(page) = req.page {
                query = query.limit(page_size().into()).offset((page_size() * (page - 1)).into());
            }
            let fetched = match query.load(&mut app_state.cpool().await).await {
                Err(e) => {
                    return internal(
                        e,
                        format!("E003: Failed to get all {}", $ent::specifier_plural()).as_str(),
                    )
                }
                Ok(f) => f,
            };
            let responses = join_all(
                fetched
                    .into_iter()
                    .map(async |f| $resp::stateful_try_from(f, &user, app_state.clone()).await),
            )
            .await
            .into_iter()
            .collect::<Result<Vec<$resp>, _>>();

            match responses {
                Err(e) => HttpResponse::from(e),
                Ok(c) => HttpResponse::Ok().json(c),
            }
        }
    };
}

get_all_handler!(
    get_currencies,
    Currency,
    CurrencyResponse,
    crate::schema::currencies::dsl::id.asc()
);
get_all_handler!(get_sources, Source, SourceResponse, crate::schema::sources::dsl::name.asc());
get_all_handler!(
    get_categories,
    Category,
    CategoryResponse,
    crate::schema::categories::dsl::name.asc()
);
get_all_handler!(
    get_entries,
    Entry,
    EntryResponse,
    (
        crate::schema::entries::dsl::date.asc(),
        crate::schema::entries::dsl::created_at.asc(),
        crate::schema::entries::dsl::id.asc()
    )
);

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
                .first(&mut app_state.cpool().await)
                .await
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
            let response = $resp::stateful_try_from(fetched, &user, app_state.clone()).await;
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
#[serde(deny_unknown_fields)]
pub struct BulkRequest {
    ids: Vec<i32>,
}

#[derive(thiserror::Error, Debug, Serialize)]
pub enum UpdateEntrySourcesError {
    #[error("failed to get source {source_id:?} for entry {entry_id}")]
    NoSource {
        entry_id: i32,
        /// Secondary sources may be non-present. Wrap primary sources with Some() always.
        source_id: Option<i32>,
        #[serde(skip_serializing)]
        #[source]
        error: diesel::result::Error,
    },
    #[error("Entry {entry_id} is malformed, with source {source_id:?}")]
    MalformedEntry { entry_id: i32, source_id: Option<i32> },
    #[error("Failed to update source {source_id:?} for entry {entry_id}")]
    UpdateError {
        entry_id: i32,
        source_id: Option<i32>,
        #[serde(skip_serializing)]
        #[source]
        error: diesel::result::Error,
    },
}

pub enum UpdateEntrySourcesType {
    /// Adds entry values to source.
    Create,
    /// Subtracts entry values from source.
    Delete,
}

/// app_state here is for read-only queries
/// conn, be it a connection or a transcaction, is for updates
#[allow(clippy::single_match)]
async fn update_entry_sources(
    entry: &Entry,
    app_state: Arc<AppState>,
    mut conn: &mut Conn,
    update_type: UpdateEntrySourcesType,
) -> Result<i32, UpdateEntrySourcesError> {
    let c1 = match update_type {
        UpdateEntrySourcesType::Create => 1f64,
        UpdateEntrySourcesType::Delete => -1f64,
    };
    let c2 = match entry.entry_type {
        EntryType::Borrow => 1f64, // borrowing increases the source
        EntryType::Lend => -1f64,
        EntryType::Income => 1f64, // income increases the source
        EntryType::Spend => -1f64,
        EntryType::Convert => -1f64, // convert decreases primary source
    };
    let source = match Source::get_by_id(entry.source_id, app_state.clone()).await {
        Err(e) => {
            return Err(UpdateEntrySourcesError::NoSource {
                entry_id: entry.id,
                source_id: Some(entry.source_id),
                error: e,
            });
        }
        Ok(s) => s,
    };
    let secondary_source =
        match Source::get_by_id(entry.secondary_source_id, app_state.clone()).await {
            Err(e) => {
                return Err(UpdateEntrySourcesError::NoSource {
                    entry_id: entry.id,
                    source_id: entry.secondary_source_id,
                    error: e,
                });
            }
            Ok(s) => s,
        };
    if entry.entry_type != EntryType::Convert && secondary_source.is_some() {
        return Err(UpdateEntrySourcesError::MalformedEntry {
            entry_id: entry.id,
            source_id: entry.secondary_source_id,
        });
    }
    use crate::schema::sources::dsl::*;

    match diesel::update(&source)
        .set(amount.eq(source.amount + c1 * c2 * entry.source_amount))
        .execute(&mut conn)
        .await
    {
        Err(e) => {
            return Err(UpdateEntrySourcesError::UpdateError {
                entry_id: entry.id,
                source_id: Some(entry.source_id),
                error: e,
            });
        }
        Ok(_) => {}
    };
    if let Some(a) = entry.secondary_source_amount
        && let Some(ss) = secondary_source
    {
        match diesel::update(&ss).set(amount.eq(ss.amount + c1 * a)).execute(&mut conn).await {
            Err(e) => {
                return Err(UpdateEntrySourcesError::UpdateError {
                    entry_id: entry.id,
                    source_id: entry.secondary_source_id,
                    error: e,
                });
            }
            Ok(_) => {}
        };
    }

    Ok(entry.id)
}

/// Deleting returns amounts to their respective sources. Please use archive if you do not wish
/// your entries to vanish from existence and their amounts be returned.
///
/// We do not use transactions between update sources and delete here, we simply update sources
/// then delete for each entry where the update was successful, ignoring the ones that weren't.
pub async fn delete_entries(
    ArrayQuery(req): ArrayQuery<BulkRequest>,
    app_state: web::Data<AppState>,
    user: web::ReqData<User>,
) -> HttpResponse {
    use crate::schema::entries::dsl::*;

    let user = &user.into_inner();
    let app_state = app_state.into_inner();

    let fetched = match Entry::belonging_to(&user)
        .select(Entry::as_select())
        .filter(id.eq_any(&req.ids))
        .load(&mut app_state.cpool().await)
        .await
    {
        Err(e) => {
            return internal(
                e,
                format!("E016: Failed to get {} for deletion", Entry::specifier_plural()).as_str(),
            );
        }
        Ok(f) => f,
    };

    let futures = fetched.iter().map(async |e| {
        update_entry_sources(
            e,
            app_state.clone(),
            &mut app_state.cpool().await,
            UpdateEntrySourcesType::Delete,
        )
        .await
    });
    let source_map_result = join_all(futures).await;
    let (oks, errs): (Vec<_>, Vec<_>) = source_map_result.into_iter().partition_result();
    let deleted_count = diesel::delete(Entry::belonging_to(&user).filter(id.eq_any(oks)))
        .execute(&mut app_state.cpool().await)
        .await;

    let errs_json = match serde_json::to_string(&errs) {
        Err(e) => {
            error!(
                "Failed to serialize {} transaction errors as string {:?} with error {:?}",
                Source::specifier(),
                &errs,
                e
            );
            format!(
                "Failed to serialize {} transaction errors as string {:?}",
                Source::specifier(),
                &errs
            )
        }
        Ok(o) => o,
    };

    match (deleted_count, errs.len()) {
        (Ok(count), 0) => HttpResponse::Ok().json(CountResponse { count }),
        (Ok(count), _) => internal(
            errs,
            format!(
                "E017: Successfully deleted {} {}, but failed to get {} for {}: {}",
                count,
                Entry::specifier_plural(),
                Source::specifier_plural(),
                Entry::specifier_plural(),
                errs_json,
            ),
        ),
        (Err(e), _) => internal(e, format!("E004: Failed to delete {}", Entry::specifier_plural())),
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
            .execute(&mut app_state.cpool().await)
            .await;
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
            let change_set =
                match $changeset::stateful_try_from(data, &user, app_state.clone()).await {
                    Err(e) => return HttpResponse::from(e),
                    Ok(c) => c,
                };
            match diesel::update($ent::belonging_to(&user).filter(name.eq(&path_name)))
                .set(change_set)
                .execute(&mut app_state.cpool().await)
                .await
            {
                Ok(1) => HttpResponse::Ok().json(EmptyResponse {}),
                Ok(0) => HttpResponse::NotFound().finish(),
                Ok(2..) => internal(
                    "No underlying error",
                    format!("E010: Updated more than one {}", $ent::specifier()),
                ),
                Err(e) => internal(e, format!("E011: Could not update {}", $ent::specifier())),
            }
        }
    };
}

update_handler!(update_currency, currencies, Currency, UpdateCurrency, UpdateCurrencyRequest);
update_handler!(update_source, sources, Source, UpdateSource, UpdateSourceRequest);
update_handler!(update_category, categories, Category, UpdateCategory, UpdateCategoryRequest);

pub async fn update_entry(
    path_id: web::Path<i32>,
    app_state: web::Data<AppState>,
    data: web::Json<UpdateEntryRequest>,
    user: web::ReqData<User>,
) -> HttpResponse {
    use crate::schema::entries::dsl::*;
    let user = user.into_inner();
    let app_state = app_state.into_inner();
    let path_id = path_id.into_inner();
    let data = data.into_inner();
    let change_set = match UpdateEntry::stateful_try_from(data, &user, app_state.clone()).await {
        Err(e) => return HttpResponse::from(e),
        Ok(c) => c,
    };
    match diesel::update(Entry::belonging_to(&user).filter(id.eq(path_id)))
        .set(change_set)
        .execute(&mut app_state.cpool().await)
        .await
    {
        Ok(1) => HttpResponse::Ok().json(EmptyResponse {}),
        Ok(0) => HttpResponse::NotFound().finish(),
        Ok(2..) => internal(
            "No underlying error",
            format!("E010: Updated more than one {}", Entry::specifier()),
        ),
        Err(e) => internal(e, format!("E011: Could not update {}", Entry::specifier())),
    }
}

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
                .first::<$ent>(&mut app_state.cpool().await)
                .await
            {
                Ok(f) => f,
                Err(_) => {
                    return HttpResponse::NotFound().body(format!("{} not found", $ent::specifier()))
                }
            };
            let net_amount = match fetched.get_net_amount(app_state.clone()).await {
                Ok(t) => t,
                Err(e) => return internal(e, "E006: Unable to construct sum - failed to archive"),
            };
            if (net_amount - 0f64).abs() > consts::EPSILON {
                return HttpResponse::BadRequest().body($err);
            }
            match diesel::update(&fetched)
                .set(archived.eq(true))
                .execute(&mut app_state.cpool().await)
                .await
            {
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

macro_rules! get_entries_for {
    ($fn_name:ident, $parent_table:ident, $ent:ident, $filter_expr:expr,) => {
        pub async fn $fn_name(
            web::Query(req): web::Query<SimplePaginatedRequest>,
            path_name: web::Path<String>,
            app_state: web::Data<AppState>,
            user: web::ReqData<User>,
        ) -> HttpResponse {
            use crate::schema::entries::dsl::archived;
            let app_state = app_state.into_inner();
            let user = user.into_inner();
            let path_name = path_name.into_inner();

            let parent = match $ent::belonging_to(&user)
                .filter(crate::schema::$parent_table::dsl::name.eq(&path_name))
                .first::<$ent>(&mut app_state.cpool().await)
                .await
            {
                Ok(p) => p,
                Err(_) => {
                    return HttpResponse::NotFound().body(format!(
                        "{} {} not found",
                        $ent::specifier(),
                        path_name
                    ))
                }
            };

            // Boxing the query allows us to mutate it without changing its type.
            let mut query = diesel::QueryDsl::into_boxed(
                Entry::belonging_to(&user).filter($filter_expr(parent.id).and(archived.eq(false))),
            );
            if let Some(page) = req.page {
                query = query.limit(page_size().into()).offset((page_size() * (page - 1)).into());
            }
            let found = query.load::<Entry>(&mut app_state.cpool().await).await;

            match found {
                Ok(entries) => {
                    let out =
                        join_all(entries.into_iter().map(|e| {
                            EntryResponse::stateful_try_from(e, &user, app_state.clone())
                        }))
                        .await
                        .into_iter()
                        .filter_map(Result::ok)
                        .collect::<Vec<_>>();
                    HttpResponse::Ok().json(out)
                }
                Err(e) => internal(
                    e,
                    format!("E020: Failed to get entries for {} {}", $ent::specifier(), path_name),
                ),
            }
        }
    };
}

get_entries_for!(get_currency_entries, currencies, Currency, |input_id| {
    crate::schema::entries::dsl::currency_id.eq(input_id)
},);

get_entries_for!(get_category_entries, categories, Category, |input_id| {
    crate::schema::entries::dsl::category_id.eq(input_id)
},);

pub async fn get_source_entries(
    web::Query(req): web::Query<SourceEntriesRequest>,
    path_name: web::Path<String>,
    app_state: web::Data<AppState>,
    user: web::ReqData<User>,
) -> HttpResponse {
    use crate::schema::entries::dsl::archived;
    let app_state = app_state.into_inner();
    let user = user.into_inner();
    let path_name = path_name.into_inner();

    let parent = match Source::belonging_to(&user)
        .filter(crate::schema::sources::dsl::name.eq(&path_name))
        .first::<Source>(&mut app_state.cpool().await)
        .await
    {
        Ok(p) => p,
        Err(_) => {
            return HttpResponse::NotFound().body(format!(
                "{} {} not found",
                Source::specifier(),
                path_name
            ));
        }
    };

    // Boxing the query allows us to mutate it without changing its type.
    let mut query = diesel::QueryDsl::into_boxed(Entry::belonging_to(&user));

    if let Some(primary_only) = req.primary_only
        && primary_only
    {
        query = query.filter(crate::schema::entries::dsl::source_id.eq(parent.id));
    } else {
        query = query.filter(
            crate::schema::entries::dsl::source_id
                .eq(parent.id)
                .or(crate::schema::entries::dsl::secondary_source_id.eq(Some(parent.id))),
        );
    }
    query = query.filter(archived.eq(false));
    if let Some(page) = req.page {
        query = query.limit(page_size().into()).offset((page_size() * (page - 1)).into());
    }
    let found = query.load::<Entry>(&mut app_state.cpool().await).await;

    match found {
        Ok(entries) => {
            let out = join_all(
                entries
                    .into_iter()
                    .map(|e| EntryResponse::stateful_try_from(e, &user, app_state.clone())),
            )
            .await
            .into_iter()
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
            HttpResponse::Ok().json(out)
        }
        Err(e) => internal(
            e,
            format!("E020: Failed to get entries for {} {}", Source::specifier(), path_name),
        ),
    }
}

pub async fn find_entries(
    ArrayQuery(query_params): ArrayQuery<EntryQuery>,
    app_state: web::Data<AppState>,
    user: web::ReqData<User>,
) -> HttpResponse {
    let user = user.into_inner();
    let app_state = app_state.into_inner();

    match Entry::find_by_filter(&query_params, &user, app_state.clone()).await {
        Ok(entries) => {
            let sum_amounts: f64 = entries.iter().map(|entry| entry.amount).sum();

            let mut sum_per_month: HashMap<String, f64> = HashMap::new();
            for entry in &entries {
                let month_year = entry.date.format("%Y-%m").to_string();
                *sum_per_month.entry(month_year).or_insert(0.0) += entry.amount;
            }
            let num_months = sum_per_month.len();
            let monthly_average =
                if num_months != 0 { sum_amounts / (num_months as f64) } else { 0.0f64 };

            let mut sum_per_category_per_month: HashMap<String, f64> = HashMap::new();
            for entry in &entries {
                let month_year = entry.date.format("%Y-%m").to_string();
                let category_month_key =
                    format!("{}|{}", entry.category_id.clone(), month_year.clone());
                *sum_per_category_per_month.entry(category_month_key).or_insert(0.0) +=
                    entry.amount;
            }

            HttpResponse::Ok().json(FindEntriesResponse {
                sum_per_month,
                monthly_average,
                sum_per_category_per_month,
                entries: join_all(entries.into_iter().map(async |o| {
                    EntryResponse::stateful_try_from(o, &user, app_state.clone()).await
                }))
                .await
                .into_iter()
                .filter_map(|o| o.ok())
                .collect(),
            })
        }
        Err(e) => e.into(),
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

/*
todos: three 5s, one 9, one 10, one 12, six 15s, one 20, two 30s, one 40, two 70s, one 75, three 80s
after those todos, API will be pretty much done
I can start work on front-end, and then v1 of thi
*/
