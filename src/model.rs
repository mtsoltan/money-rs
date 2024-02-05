use chrono::NaiveDateTime;
use diesel::*;

// Needed by macros
use crate::schema::sql_types::EntryT;
use crate::schema::*;

#[derive(Debug, PartialEq, Clone, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "EntryT"]
pub enum EntryType {
    Spend,
    Income,
    Lend,
    Borrow,
    Convert,
}

#[derive(Debug, Queryable, Selectable, Identifiable, Insertable)]
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

impl Default for User {
    fn default() -> Self {
        User {
            id: 0,
            username: format!(""),
            password: format!(""), // TODO: Put a random hash here
            fixed_currency_id: None,
        }
    }
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
pub struct NewCurrency<'a> {
    pub user_id: i32,
    pub name: &'a str,
    pub rate_to_fixed: f64,
}

#[derive(AsChangeset)]
#[diesel(table_name = currencies)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct UpdatedCurrency<'a> {
    pub name: &'a str,
    pub rate_to_fixed: f64,
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

#[derive(Debug, Queryable, Identifiable, Associations, Insertable)]
#[diesel(table_name = categories)]
#[diesel(belongs_to(User))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Category {
    pub id: i32,
    pub user_id: i32,
    pub name: String,
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
    pub currency_id: i32,
    pub entry_type: EntryType,
    pub source_id: i32,
    pub conversion_rate: f64,
    pub conversion_rate_to_fixed: f64,
}
