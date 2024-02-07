use chrono::{NaiveDate, NaiveDateTime};
use diesel::*;
use serde::{Deserialize, Serialize};

// Needed by macros
use crate::schema::sql_types::EntryT;
use crate::schema::*;

#[derive(Debug, PartialEq, Clone, diesel_derive_enum::DbEnum, Serialize, Deserialize)]
#[ExistingTypePath = "EntryT"]
pub enum EntryType {
    Spend,
    Income,
    Lend,
    Borrow,
    Convert,
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

#[derive(Debug, Queryable, Identifiable, Associations, Insertable)]
#[diesel(table_name = currencies)]
#[diesel(belongs_to(User))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Currency {
    pub id: i32,
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

impl NewCurrency {
    pub fn from_request(value: CurrencyRequest, user: User) -> Self {
        Self {
            user_id: user.id,
            name: value.name,
            rate_to_fixed: value.rate_to_fixed,
        }
    }
}

#[derive(Debug, Queryable, Identifiable, Associations, Insertable)]
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
    pub currency: i32,
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

impl NewSource {
    pub fn from_request(value: SourceRequest, user: User) -> Self {
        Self {
            user_id: user.id,
            name: value.name,
            currency_id: value.currency,
            amount: 0.0f64,
        }
    }
}

#[derive(Debug, Queryable, Identifiable, Associations, Insertable)]
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

impl NewCategory {
    pub fn from_request(value: CategoryRequest, user: User) -> Self {
        Self {
            user_id: user.id,
            name: value.name,
        }
    }
}

#[derive(Debug, Queryable, Identifiable, Associations, Insertable)]
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
    pub conversion_rate: f64,
    pub conversion_rate_to_fixed: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EntryRequest {
    pub description: String,
    pub category: i32,
    pub amount: f64,
    pub date: String,
    pub currency: i32,
    pub entry_type: EntryType,
    pub source: i32,
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
    pub currency_id: i32,
    pub entry_type: EntryType,
    pub source_id: i32,
    pub conversion_rate: f64,
    pub conversion_rate_to_fixed: f64,
}

// TODO: Automatically fetch conversion rates, change the signature of from_request for this
// make custom logic instead of using the create! macro.
impl NewEntry {
    pub fn try_from_request(
        value: EntryRequest,
        user: User,
    ) -> Result<Self, chrono::format::ParseError> {
        Ok(Self {
            user_id: user.id,
            description: value.description,
            category_id: value.category,
            amount: value.amount,
            date: NaiveDate::parse_from_str(value.date.as_str(), "%Y-%m-%d")?.into(),
            currency_id: value.currency,
            entry_type: value.entry_type,
            source_id: value.source,
            conversion_rate: value.conversion_rate,
            conversion_rate_to_fixed: value.conversion_rate_to_fixed,
        })
    }
}
