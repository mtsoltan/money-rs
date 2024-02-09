use std::sync::Arc;

use chrono::{NaiveDate, NaiveDateTime};
use diesel::*;
use serde::{Deserialize, Serialize};

// Needed by macros
use crate::schema::sql_types::EntryT;
use crate::{schema::*, AppState};

#[derive(Debug, PartialEq, Clone, diesel_derive_enum::DbEnum, Serialize, Deserialize)]
#[ExistingTypePath = "EntryT"]
pub enum EntryType {
    Spend,
    Income,
    Lend,
    Borrow,
    Convert,
}

pub enum TryFromRequestError {
    ReferencedDoesNotExist(diesel::result::Error),
    DateTimeParseError(chrono::format::ParseError),
}

impl From<diesel::result::Error> for TryFromRequestError {
    fn from(value: diesel::result::Error) -> Self {
        Self::ReferencedDoesNotExist(value)
    }
}

impl From<chrono::format::ParseError> for TryFromRequestError {
    fn from(value: chrono::format::ParseError) -> Self {
        Self::DateTimeParseError(value)
    }
}

pub trait TryFromRequest<S> {
    fn try_from_request(
        value: S,
        user: User,
        app_state: Arc<AppState>,
    ) -> Result<Self, TryFromRequestError>
    where
        Self: Sized;
}

#[derive(Debug, Queryable, Selectable, Identifiable, Clone)]
#[diesel(table_name = users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct User {
    pub id: i32,
    pub username: String,
    pub password: String,
    pub fixed_currency_id: Option<i32>,
}

#[derive(Insertable)]
#[diesel(table_name = users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewUser {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Queryable, Selectable, Identifiable, Associations, Insertable, Serialize)]
#[diesel(table_name = currencies)]
#[diesel(belongs_to(User))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Currency {
    #[serde(skip_serializing)]
    pub id: i32,
    #[serde(skip_serializing)]
    pub user_id: i32,
    pub name: String,
    pub rate_to_fixed: f64,
}

#[derive(Insertable)]
#[diesel(table_name = currencies)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewCurrency {
    pub user_id: i32,
    pub name: String,
    pub rate_to_fixed: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CurrencyRequest {
    pub name: String,
    pub rate_to_fixed: f64,
}

impl TryFromRequest<CurrencyRequest> for NewCurrency {
    fn try_from_request(
        value: CurrencyRequest,
        user: User,
        _app_state: Arc<AppState>,
    ) -> Result<Self, TryFromRequestError> {
        Ok(Self {
            user_id: user.id,
            name: value.name,
            rate_to_fixed: value.rate_to_fixed,
        })
    }
}

#[derive(Debug, Queryable, Selectable, Identifiable, Associations, Insertable, Serialize)]
#[diesel(table_name = sources)]
#[diesel(belongs_to(User))]
#[diesel(belongs_to(Currency))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Source {
    pub id: i32,
    pub user_id: i32,
    pub name: String,
    pub currency_id: i32,
    pub amount: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SourceRequest {
    pub name: String,
    pub currency: String,
}

#[derive(Insertable)]
#[diesel(table_name = sources)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewSource {
    pub user_id: i32,
    pub name: String,
    pub currency_id: i32,
    pub amount: f64,
}

impl TryFromRequest<SourceRequest> for NewSource {
    fn try_from_request(
        value: SourceRequest,
        user: User,
        app_state: Arc<AppState>,
    ) -> Result<Self, TryFromRequestError> {
        use crate::schema::currencies::dsl::*;
        let currency_id: i32 = currencies
            .filter(name.eq(value.currency).and(user_id.eq(user.id)))
            .select(id)
            .first(&mut app_state.cpool())?;
        Ok(Self {
            user_id: user.id,
            name: value.name,
            currency_id,
            amount: 0.0f64,
        })
    }
}

#[derive(Debug, Queryable, Selectable, Identifiable, Associations, Insertable, Serialize)]
#[diesel(table_name = categories)]
#[diesel(belongs_to(User))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Category {
    pub id: i32,
    pub user_id: i32,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryRequest {
    pub name: String,
}

#[derive(Insertable)]
#[diesel(table_name = categories)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewCategory {
    pub user_id: i32,
    pub name: String,
}

impl TryFromRequest<CategoryRequest> for NewCategory {
    fn try_from_request(
        value: CategoryRequest,
        user: User,
        _app_state: Arc<AppState>,
    ) -> Result<Self, TryFromRequestError> {
        Ok(Self {
            user_id: user.id,
            name: value.name,
        })
    }
}

#[derive(Debug, Queryable, Selectable, Identifiable, Associations, Insertable, Serialize)]
#[diesel(table_name = entries)]
#[diesel(belongs_to(User))]
#[diesel(belongs_to(Source))]
#[diesel(belongs_to(Category))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Entry {
    pub id: i32,
    pub user_id: i32,
    pub description: String,
    pub category_id: i32,
    pub amount: f64,
    pub date: NaiveDateTime,
    pub created_at: NaiveDateTime,
    pub currency_id: i32,
    pub entry_type: EntryType,
    pub source_id: i32,
    pub secondary_source_id: Option<i32>,
    pub conversion_rate: f64,
    pub conversion_rate_to_fixed: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EntryRequest {
    pub description: String,
    pub category: String,
    pub amount: f64,
    pub date: String,
    pub currency: String,
    pub entry_type: EntryType,
    pub source: String,
    pub secondary_source: Option<String>,
    pub conversion_rate: f64,
    pub conversion_rate_to_fixed: f64,
}

#[derive(Insertable)]
#[diesel(table_name = entries)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewEntry {
    pub user_id: i32,
    pub description: String,
    pub category_id: i32,
    pub amount: f64,
    pub date: NaiveDateTime,
    pub created_at: Option<NaiveDateTime>,
    pub currency_id: i32,
    pub entry_type: EntryType,
    pub source_id: i32,
    pub secondary_source_id: Option<i32>,
    pub conversion_rate: f64,
    pub conversion_rate_to_fixed: f64,
}

impl TryFromRequest<EntryRequest> for NewEntry {
    fn try_from_request(
        value: EntryRequest,
        user: User,
        app_state: Arc<AppState>,
    ) -> Result<Self, TryFromRequestError> {
        let currency_id: i32 = {
            use crate::schema::currencies::dsl::*;

            currencies
                .filter(name.eq(value.currency).and(user_id.eq(user.id)))
                .select(id)
                .first(&mut app_state.cpool())?
        };
        let category_id: i32 = {
            use crate::schema::categories::dsl::*;
            categories
                .filter(name.eq(value.category).and(user_id.eq(user.id)))
                .select(id)
                .first(&mut app_state.cpool())?
        };
        let source_id: i32 = {
            use crate::schema::sources::dsl::*;
            sources
                .filter(name.eq(value.source).and(user_id.eq(user.id)))
                .select(id)
                .first(&mut app_state.cpool())?
        };
        let secondary_source_id: Option<i32> = match value.secondary_source {
            None => None,
            Some(s) => {
                use crate::schema::sources::dsl::*;
                Some(
                    sources
                        .filter(name.eq(s).and(user_id.eq(user.id)))
                        .select(id)
                        .first(&mut app_state.cpool())?,
                )
            }
        };
        Ok(Self {
            user_id: user.id,
            description: value.description,
            category_id,
            amount: value.amount,
            date: NaiveDate::parse_from_str(value.date.as_str(), "%Y-%m-%d")?.into(),
            created_at: None,
            currency_id,
            entry_type: value.entry_type,
            source_id,
            secondary_source_id,
            conversion_rate: value.conversion_rate,
            conversion_rate_to_fixed: value.conversion_rate_to_fixed,
        })
    }
}
