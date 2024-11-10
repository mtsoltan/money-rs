use std::sync::Arc;

use chrono::{NaiveDate, NaiveDateTime};
use diesel::*;
use inner_macros::Entity;
use serde::{Deserialize, Serialize};

// Needed by macros
use crate::schema::sql_types::EntryT;
use crate::schema::*;
use crate::AppState;

#[derive(Debug, PartialEq, Clone, diesel_derive_enum::DbEnum, Serialize, Deserialize)]
#[ExistingTypePath = "EntryT"]
pub enum EntryType {
    Spend,
    Income,
    Lend,
    Borrow,
    Convert,
}

#[derive(Debug)]
pub enum StatefulTryFromError {
    ReferencedDoesNotExist(diesel::result::Error),
    DateTimeParseError(chrono::format::ParseError),
}

impl From<diesel::result::Error> for StatefulTryFromError {
    fn from(value: diesel::result::Error) -> Self { Self::ReferencedDoesNotExist(value) }
}

impl From<chrono::format::ParseError> for StatefulTryFromError {
    fn from(value: chrono::format::ParseError) -> Self { Self::DateTimeParseError(value) }
}

pub trait GetIdByNameAndUser<N, T> {
    fn get_id_by_name_and_user(
        name: N,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<T, diesel::result::Error>;
}

pub trait GetNameById<N, T> {
    fn get_name_by_id(id: N, app_state: Arc<AppState>) -> Result<T, diesel::result::Error>;
}

pub trait GetNetAmount {
    fn get_net_amount<'a>(&self, app_state: Arc<AppState>) -> Result<f64, diesel::result::Error>;
}

impl GetNetAmount for Currency {
    fn get_net_amount(&self, app_state: Arc<AppState>) -> Result<f64, diesel::result::Error> {
        use crate::schema::sources::dsl::*;
        let entry_amount_sum: f64 = Source::belonging_to(&self)
            .filter(archived.eq(false))
            .select(amount)
            .load(&mut app_state.cpool())?
            .iter()
            .sum();
        Ok(entry_amount_sum)
    }
}

impl GetNetAmount for Source {
    fn get_net_amount(&self, _app_state: Arc<AppState>) -> Result<f64, diesel::result::Error> {
        Ok(self.amount)
    }
}

impl GetNetAmount for Category {
    fn get_net_amount(&self, app_state: Arc<AppState>) -> Result<f64, diesel::result::Error> {
        use diesel::dsl::sum;

        use crate::schema::entries::dsl::*;
        let entry_amount_sum: f64 = Entry::belonging_to(&self)
            .filter(archived.eq(false))
            .select(sum(amount))
            .load::<Option<f64>>(&mut app_state.cpool())?
            .iter()
            .map(|x| x.unwrap_or(0.0f64))
            .sum();
        Ok(entry_amount_sum)
    }
}

macro_rules! get_impls {
    ($type:ty, $tb_name:ident) => {
        impl GetIdByNameAndUser<Option<String>, Option<i32>> for $type {
            fn get_id_by_name_and_user(
                name: Option<String>,
                user: &User,
                app_state: Arc<AppState>,
            ) -> Result<Option<i32>, diesel::result::Error> {
                Ok(match name {
                    None => None,
                    Some(c) => {
                        use crate::schema::$tb_name::dsl::*;
                        Some(
                            $tb_name
                                .filter(name.eq(c).and(user_id.eq(user.id)))
                                .select(id)
                                .first(&mut app_state.cpool())?,
                        )
                    }
                })
            }
        }

        impl GetIdByNameAndUser<String, i32> for $type {
            fn get_id_by_name_and_user(
                p_name: String,
                user: &User,
                app_state: Arc<AppState>,
            ) -> Result<i32, diesel::result::Error> {
                use crate::schema::$tb_name::dsl::*;
                Ok($tb_name
                    .filter(name.eq(p_name).and(user_id.eq(user.id)))
                    .select(id)
                    .first(&mut app_state.cpool())?)
            }
        }

        impl GetNameById<Option<i32>, Option<String>> for $type {
            fn get_name_by_id(
                id: Option<i32>,
                app_state: Arc<AppState>,
            ) -> Result<Option<String>, diesel::result::Error> {
                Ok(match id {
                    None => None,
                    Some(c) => {
                        use crate::schema::$tb_name::dsl::*;
                        Some($tb_name.find(c).select(name).first(&mut app_state.cpool())?)
                    }
                })
            }
        }

        impl GetNameById<i32, String> for $type {
            fn get_name_by_id(
                p_id: i32,
                app_state: Arc<AppState>,
            ) -> Result<String, diesel::result::Error> {
                use crate::schema::$tb_name::dsl::*;
                Ok($tb_name.find(p_id).select(name).first(&mut app_state.cpool())?)
            }
        }
    };
}

get_impls!(Currency, currencies);
get_impls!(Category, categories);
get_impls!(Source, sources);

pub trait StatefulTryFrom<S> {
    fn stateful_try_from(
        value: S,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError>
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
    pub enabled: bool,
}

#[derive(Insertable)]
#[diesel(table_name = users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewUser {
    pub username: String,
    pub password: String,
}

#[derive(
    Entity, Debug, Queryable, Selectable, Identifiable, Associations, Insertable, Serialize,
)]
#[diesel(table_name = currencies)]
#[diesel(belongs_to(User))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Currency {
    #[entity(NotUpdatable, NotViewable, NotSettable, Id)]
    pub id: i32,
    #[entity(NotUpdatable, NotViewable, NotSettable)]
    pub user_id: i32,
    pub name: String,
    pub rate_to_fixed: f64,
    #[entity(HasDefault)]
    pub archived: bool,
}

impl StatefulTryFrom<CreateCurrencyRequest> for NewCurrency {
    fn stateful_try_from(
        value: CreateCurrencyRequest,
        user: &User,
        _app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self {
            user_id: user.id,
            name: value.name,
            rate_to_fixed: value.rate_to_fixed,
            archived: value.archived,
        })
    }
}

impl StatefulTryFrom<UpdateCurrencyRequest> for UpdateCurrency {
    fn stateful_try_from(
        value: UpdateCurrencyRequest,
        _user: &User,
        _app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self { name: value.name, rate_to_fixed: value.rate_to_fixed, archived: value.archived })
    }
}

impl StatefulTryFrom<Currency> for CurrencyResponse {
    fn stateful_try_from(
        value: Currency,
        _user: &User,
        _app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self { name: value.name, rate_to_fixed: value.rate_to_fixed, archived: value.archived })
    }
}

#[derive(
    Entity, Debug, Queryable, Selectable, Identifiable, Associations, Insertable, Serialize,
)]
#[diesel(table_name = sources)]
#[diesel(belongs_to(User))]
#[diesel(belongs_to(Currency))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Source {
    #[entity(NotUpdatable, NotViewable, NotSettable, Id)]
    pub id: i32,
    #[entity(NotUpdatable, NotViewable, NotSettable)]
    pub user_id: i32,
    pub name: String,
    #[entity(RepresentableAsString)]
    pub currency_id: i32,
    pub amount: f64,
    #[entity(HasDefault)]
    pub archived: bool,
}

impl StatefulTryFrom<CreateSourceRequest> for NewSource {
    fn stateful_try_from(
        value: CreateSourceRequest,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self {
            user_id: user.id,
            name: value.name,
            currency_id: Currency::get_id_by_name_and_user(
                value.currency,
                &user,
                app_state.clone(),
            )?,
            amount: 0.0f64,
            archived: value.archived,
        })
    }
}

impl StatefulTryFrom<UpdateSourceRequest> for UpdateSource {
    fn stateful_try_from(
        value: UpdateSourceRequest,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self {
            name: value.name,
            currency_id: Currency::get_id_by_name_and_user(
                value.currency,
                &user,
                app_state.clone(),
            )?,
            amount: value.amount,
            archived: value.archived,
        })
    }
}

impl StatefulTryFrom<Source> for SourceResponse {
    fn stateful_try_from(
        value: Source,
        _user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self {
            name: value.name,
            currency: Currency::get_name_by_id(value.currency_id, app_state.clone())?,
            amount: value.amount,
            archived: value.archived,
        })
    }
}

#[derive(
    Entity, Debug, Queryable, Selectable, Identifiable, Associations, Insertable, Serialize,
)]
#[diesel(table_name = categories)]
#[diesel(belongs_to(User))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Category {
    #[entity(NotUpdatable, NotViewable, NotSettable, Id)]
    pub id: i32,
    #[entity(NotUpdatable, NotViewable, NotSettable)]
    pub user_id: i32,
    pub name: String,
    #[entity(HasDefault)]
    archived: bool,
}

impl StatefulTryFrom<CreateCategoryRequest> for NewCategory {
    fn stateful_try_from(
        value: CreateCategoryRequest,
        user: &User,
        _app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self { user_id: user.id, name: value.name, archived: value.archived })
    }
}

impl StatefulTryFrom<UpdateCategoryRequest> for UpdateCategory {
    fn stateful_try_from(
        value: UpdateCategoryRequest,
        _user: &User,
        _app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self { name: value.name, archived: value.archived })
    }
}

impl StatefulTryFrom<Category> for CategoryResponse {
    fn stateful_try_from(
        value: Category,
        _user: &User,
        _app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self { name: value.name, archived: value.archived })
    }
}

#[derive(
    Entity, Debug, Queryable, Selectable, Identifiable, Associations, Insertable, Serialize,
)]
#[diesel(table_name = entries)]
#[diesel(belongs_to(User))]
#[diesel(belongs_to(Source))]
#[diesel(belongs_to(Category))]
#[diesel(belongs_to(Currency))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Entry {
    #[entity(NotUpdatable, NotSettable, Id)]
    pub id: i32,
    #[entity(NotUpdatable, NotViewable, NotSettable)]
    pub user_id: i32,
    pub description: String,
    #[entity(RepresentableAsString)]
    pub category_id: i32,
    pub amount: f64,
    #[entity(RepresentableAsString)]
    pub date: NaiveDateTime,
    #[entity(NotUpdatable, NotSettable, HasDefault, RepresentableAsString)]
    pub created_at: NaiveDateTime,
    #[entity(RepresentableAsString)]
    pub currency_id: i32,
    pub entry_type: EntryType,
    #[entity(RepresentableAsString)]
    pub source_id: i32,
    #[entity(RepresentableAsString)]
    pub secondary_source_id: Option<i32>,
    pub conversion_rate: Option<f64>,
    pub conversion_rate_to_fixed: f64,
    #[entity(HasDefault)]
    pub archived: bool,
}

impl StatefulTryFrom<CreateEntryRequest> for NewEntry {
    fn stateful_try_from(
        value: CreateEntryRequest,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        let new_secondary_source_id =
            Source::get_id_by_name_and_user(value.secondary_source, &user, app_state.clone())?;
        let new_conversion_rate =
            if new_secondary_source_id.is_some() { value.conversion_rate } else { None };
        Ok(Self {
            user_id: user.id,
            description: value.description,
            category_id: Category::get_id_by_name_and_user(
                value.category,
                &user,
                app_state.clone(),
            )?,
            amount: value.amount,
            date: NaiveDate::parse_from_str(value.date.as_str(), "%F")?.into(),
            created_at: None,
            currency_id: Currency::get_id_by_name_and_user(
                value.currency,
                &user,
                app_state.clone(),
            )?,
            entry_type: value.entry_type,
            source_id: Source::get_id_by_name_and_user(value.source, &user, app_state.clone())?,
            secondary_source_id: new_secondary_source_id,
            conversion_rate: new_conversion_rate,
            conversion_rate_to_fixed: value.conversion_rate_to_fixed,
            archived: value.archived,
        })
    }
}

impl StatefulTryFrom<UpdateEntryRequest> for UpdateEntry {
    fn stateful_try_from(
        value: UpdateEntryRequest,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        let new_secondary_source_id =
            Source::get_id_by_name_and_user(value.secondary_source, &user, app_state.clone())?;
        let new_conversion_rate =
            if new_secondary_source_id.is_some() { value.conversion_rate } else { None };
        Ok(Self {
            description: value.description,
            category_id: Category::get_id_by_name_and_user(
                value.category,
                &user,
                app_state.clone(),
            )?,
            amount: value.amount,
            date: match value.date {
                None => None,
                Some(c) => Some(NaiveDate::parse_from_str(c.as_str(), "%F")?.into()),
            },
            currency_id: Currency::get_id_by_name_and_user(
                value.currency,
                &user,
                app_state.clone(),
            )?,
            entry_type: value.entry_type,
            source_id: Source::get_id_by_name_and_user(value.source, &user, app_state.clone())?,
            secondary_source_id: new_secondary_source_id,
            conversion_rate: new_conversion_rate,
            conversion_rate_to_fixed: value.conversion_rate_to_fixed,
            archived: value.archived,
        })
    }
}

impl StatefulTryFrom<Entry> for EntryResponse {
    fn stateful_try_from(
        value: Entry,
        _user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self {
            id: value.id,
            description: value.description,
            category: Category::get_name_by_id(value.category_id, app_state.clone())?,
            amount: value.amount,
            date: value.date.format("%F").to_string(),
            created_at: value.created_at.format("%+").to_string(),
            currency: Currency::get_name_by_id(value.currency_id, app_state.clone())?,
            entry_type: value.entry_type,
            source: Source::get_name_by_id(value.source_id, app_state.clone())?,
            secondary_source: Source::get_name_by_id(value.secondary_source_id, app_state.clone())?,
            conversion_rate: value.conversion_rate,
            conversion_rate_to_fixed: value.conversion_rate_to_fixed,
            archived: value.archived,
        })
    }
}

/// - ids (IN) - for multi-select
/// - sources (IN)
/// - currencies (IN)
/// - categories (IN)
/// - amount (EQ - care float)
/// - min_amount (GTE)
/// - max_amount (LTE)
/// - date (EQ)
/// - after (GTE)
/// - before (LTE)
/// - created_after (GTE)
/// - created_before (LTE)
/// - description (LIKE)
/// - entry_types (IN)
/// - limit (default: 500)
#[derive(Deserialize)]
pub struct EntryQuery {
    ids: Option<Vec<i32>>,
    sources: Option<Vec<String>>,
    currencies: Option<Vec<String>>,
    categories: Option<Vec<String>>,
    amount: Option<f64>,
    min_amount: Option<f64>,
    max_amount: Option<f64>,
    date: Option<String>,
    after: Option<String>,
    before: Option<String>,
    created_after: Option<String>,
    created_before: Option<String>,
    description: Option<String>,
    entry_types: Option<Vec<EntryType>>,
    limit: Option<i64>,
    sort: Option<String>,
}

impl Entry {
    pub fn find_by_filter(
        query_params: &EntryQuery,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Vec<Entry>, StatefulTryFromError> {
        use crate::schema::entries::dsl::*;
        let mut query = entries.into_boxed();

        if let Some(ids) = &query_params.ids {
            query = query.filter(id.eq_any(ids));
        }

        if let Some(names) = &query_params.sources {
            let ids: Vec<_> = names
                .iter()
                .filter_map(|name| {
                    Source::get_id_by_name_and_user(name.clone(), &user, app_state.clone()).ok()
                })
                .collect();

            query = query.filter(source_id.eq_any(ids));
        }
        if let Some(names) = &query_params.currencies {
            let ids: Vec<_> = names
                .iter()
                .filter_map(|name| {
                    Currency::get_id_by_name_and_user(name.clone(), &user, app_state.clone()).ok()
                })
                .collect();

            query = query.filter(currency_id.eq_any(ids));
        }
        if let Some(names) = &query_params.categories {
            let ids: Vec<_> = names
                .iter()
                .filter_map(|name| {
                    Category::get_id_by_name_and_user(name.clone(), &user, app_state.clone()).ok()
                })
                .collect();

            query = query.filter(category_id.eq_any(ids));
        }

        if let Some(q_amount) = &query_params.amount {
            query = query.filter(amount.eq(q_amount));
        }

        if let Some(min_amount) = query_params.min_amount {
            query = query.filter(amount.ge(min_amount));
        }

        if let Some(max_amount) = query_params.max_amount {
            query = query.filter(amount.le(max_amount));
        }

        if let Some(q_date) = &query_params.date {
            let ndt: NaiveDateTime = NaiveDate::parse_from_str(q_date, "%F")?.into();
            query = query.filter(date.eq(ndt));
        }

        if let Some(after) = &query_params.after {
            let ndt: NaiveDateTime = NaiveDate::parse_from_str(after, "%F")?.into();
            query = query.filter(date.gt(ndt));
        }

        if let Some(before) = &query_params.before {
            let ndt: NaiveDateTime = NaiveDate::parse_from_str(before, "%F")?.into();
            query = query.filter(date.lt(ndt));
        }

        if let Some(created_after) = &query_params.created_after {
            let created_after_datetime = NaiveDateTime::parse_from_str(created_after, "%+")?;

            query = query.filter(created_at.gt(created_after_datetime));
        }

        if let Some(created_before) = &query_params.created_before {
            let created_before_datetime = NaiveDateTime::parse_from_str(created_before, "%+")?;

            query = query.filter(created_at.lt(created_before_datetime));
        }

        if let Some(q_description) = &query_params.description {
            query = query.filter(description.ilike(format!("%{q_description}%")));
        }

        if let Some(entry_types) = &query_params.entry_types {
            query = query.filter(entry_type.eq_any(entry_types));
        }

        if let Some(limit) = query_params.limit {
            query = query.limit(limit);
        }

        if let Some(sort) = &query_params.sort {
            match sort.as_str() {
                "amount_asc" => query = query.order(amount.asc()),
                "amount_desc" => query = query.order(amount.desc()),
                "date_asc" => query = query.order(date.asc()),
                "date_desc" => query = query.order(date.desc()),
                _ => (),
            }
        }

        let r_entries = query.load::<Entry>(&mut app_state.cpool())?;

        Ok(r_entries)
    }
}

pub trait HasSpecifier {
    fn specifier() -> &'static str;
    fn specifier_plural() -> &'static str;
}

impl HasSpecifier for User {
    fn specifier() -> &'static str { "user" }
    fn specifier_plural() -> &'static str { "users" }
}

impl HasSpecifier for Currency {
    fn specifier() -> &'static str { "currency" }
    fn specifier_plural() -> &'static str { "currencies" }
}

impl HasSpecifier for Category {
    fn specifier() -> &'static str { "category" }
    fn specifier_plural() -> &'static str { "categories" }
}

impl HasSpecifier for Entry {
    fn specifier() -> &'static str { "entry" }
    fn specifier_plural() -> &'static str { "entries" }
}

impl HasSpecifier for Source {
    fn specifier() -> &'static str { "source" }
    fn specifier_plural() -> &'static str { "sources" }
}
