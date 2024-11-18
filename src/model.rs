use std::sync::Arc;

use chrono::{NaiveDate, NaiveDateTime};
use log::warn;
use serde::{Deserialize, Serialize};

// Needed by macros
#[rustfmt::skip]
use {
    diesel::*, // Used by `inner_macros::Entity`
    crate::schema::*,
    crate::schema::sql_types::EntryT, // Used by `diesel_derive_enum::DbEnum`
    crate::AppState,
    inner_macros::Entity // The `inner_macros::Entity` derivable macro itself
};

#[derive(Debug, PartialEq, Clone, diesel_derive_enum::DbEnum, Serialize, Deserialize)]
#[ExistingTypePath = "EntryT"]
pub enum EntryType {
    Spend,
    Income,
    Lend,
    Borrow,
    Convert,
}

#[derive(thiserror::Error, Debug)]
pub enum StatefulTryFromError {
    #[error("One of the currencies / categories / sources you referenced does not exist")]
    ReferencedDoesNotExist(#[from] diesel::result::Error),
    #[error("Malformed date provided - please use YYYY-MM-DD")]
    DateTimeParseError(#[from] chrono::format::ParseError),
    #[error("Malformed request: {0}")]
    LogicBadRequestError(Box<str>),
    #[error("Internal Server Error: {0}")]
    LogicInternalError(Box<str>),
}

pub trait GetIdByNameAndUser<N, T> {
    fn get_id_by_name_and_user(
        name: N,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<T, diesel::result::Error>;
}

pub trait GetByNameAndUser<N, T> {
    fn get_by_name_and_user(
        name: N,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<T, diesel::result::Error>;
}

pub trait GetNameById<N, T> {
    fn get_name_by_id(id: N, app_state: Arc<AppState>) -> Result<T, diesel::result::Error>;
}

pub trait GetById<N, T> {
    fn get_by_id(id: N, app_state: Arc<AppState>) -> Result<T, diesel::result::Error>;
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
        impl<T> GetIdByNameAndUser<T, i32> for $type
        where
            T: Into<String>,
        {
            fn get_id_by_name_and_user(
                p_name: T,
                user: &User,
                app_state: Arc<AppState>,
            ) -> Result<i32, diesel::result::Error> {
                use crate::schema::$tb_name::dsl::*;
                $tb_name
                    .filter(name.eq(p_name.into()).and(user_id.eq(user.id)))
                    .select(id)
                    .first(&mut app_state.cpool())
            }
        }

        impl<T> GetIdByNameAndUser<Option<T>, Option<i32>> for $type
        where
            T: Into<String>,
        {
            fn get_id_by_name_and_user(
                p_name: Option<T>,
                user: &User,
                app_state: Arc<AppState>,
            ) -> Result<Option<i32>, diesel::result::Error> {
                Ok(match p_name {
                    None => None,
                    Some(c) => {
                        use crate::schema::$tb_name::dsl::*;
                        Some(
                            $tb_name
                                .filter(name.eq(c.into()).and(user_id.eq(user.id)))
                                .select(id)
                                .first(&mut app_state.cpool())?,
                        )
                    }
                })
            }
        }

        impl<T> GetByNameAndUser<T, $type> for $type
        where
            T: Into<String>,
        {
            fn get_by_name_and_user(
                p_name: T,
                user: &User,
                app_state: Arc<AppState>,
            ) -> Result<$type, diesel::result::Error> {
                use crate::schema::$tb_name::dsl::*;
                $tb_name
                    .filter(name.eq(p_name.into()).and(user_id.eq(user.id)))
                    .first(&mut app_state.cpool())
            }
        }

        impl<T> GetByNameAndUser<Option<T>, Option<$type>> for $type
        where
            T: Into<String>,
        {
            fn get_by_name_and_user(
                p_name: Option<T>,
                user: &User,
                app_state: Arc<AppState>,
            ) -> Result<Option<$type>, diesel::result::Error> {
                Ok(match p_name {
                    None => None,
                    Some(c) => {
                        use crate::schema::$tb_name::dsl::*;
                        Some(
                            $tb_name
                                .filter(name.eq(c.into()).and(user_id.eq(user.id)))
                                .first(&mut app_state.cpool())?,
                        )
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
                $tb_name.find(p_id).select(name).first(&mut app_state.cpool())
            }
        }

        impl GetNameById<Option<i32>, Option<String>> for $type {
            fn get_name_by_id(
                p_id: Option<i32>,
                app_state: Arc<AppState>,
            ) -> Result<Option<String>, diesel::result::Error> {
                Ok(match p_id {
                    None => None,
                    Some(c) => {
                        use crate::schema::$tb_name::dsl::*;
                        Some($tb_name.find(c).select(name).first(&mut app_state.cpool())?)
                    }
                })
            }
        }

        impl GetById<i32, $type> for $type {
            fn get_by_id(
                p_id: i32,
                app_state: Arc<AppState>,
            ) -> Result<$type, diesel::result::Error> {
                use crate::schema::$tb_name::dsl::*;
                $tb_name.find(p_id).first(&mut app_state.cpool())
            }
        }

        impl GetById<Option<i32>, Option<$type>> for $type {
            fn get_by_id(
                p_id: Option<i32>,
                app_state: Arc<AppState>,
            ) -> Result<Option<$type>, diesel::result::Error> {
                Ok(match p_id {
                    None => None,
                    Some(c) => {
                        use crate::schema::$tb_name::dsl::*;
                        Some($tb_name.find(c).first(&mut app_state.cpool())?)
                    }
                })
            }
        }
    };
}

get_impls!(Currency, currencies);
get_impls!(Category, categories);
get_impls!(Source, sources);

impl GetById<i32, Entry> for Entry {
    fn get_by_id(p_id: i32, app_state: Arc<AppState>) -> Result<Entry, diesel::result::Error> {
        use crate::schema::entries::dsl::*;
        entries.find(p_id).first(&mut app_state.cpool())
    }
}

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
    #[entity(NotInResponse, NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest, Id)]
    pub id: i32,
    #[entity(NotInResponse, NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest)]
    pub user_id: i32,
    pub name: String,
    /// This is the amount of fixed currency that fits within 1 this currency that fits within .
    /// For example, the JPY rate_to_fixed would be 0.00667 if the USD is fixed.
    pub rate_to_fixed: f64,
    #[entity(HasDefault, NotInCreateRequest)]
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
            archived: None,
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
    #[entity(NotInResponse, NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest, Id)]
    pub id: i32,
    #[entity(NotInResponse, NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest)]
    pub user_id: i32,
    pub name: String,
    #[entity(RepresentableAsString, NotInDatabaseUpdate, NotInUpdateRequest)]
    pub currency_id: i32,
    #[entity(HasDefault)]
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
                value.currency.as_str(),
                &user,
                app_state.clone(),
            )?,
            amount: value.amount,
            archived: value.archived,
        })
    }
}

impl StatefulTryFrom<UpdateSourceRequest> for UpdateSource {
    fn stateful_try_from(
        value: UpdateSourceRequest,
        _user: &User,
        _app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self { name: value.name, amount: value.amount, archived: value.archived })
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
    #[entity(NotInResponse, NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest, Id)]
    pub id: i32,
    #[entity(NotInResponse, NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest)]
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
    #[entity(NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest, Id)]
    pub id: i32,
    #[entity(NotInResponse, NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest)]
    pub user_id: i32,
    /// User-entered description, we can match this to previously entered descriptions and try to
    /// decide values for other fields. In case of multi-line descriptions, the first line of the
    /// description is displayed and used for filtering, while the rest of the description is
    /// kept for memory, but stored inside `long_description` displayed under ellipsis.
    pub description: String,
    /// If the user checks the multi-line checkbox, they can specify this. The first line of
    /// the `long_description` is cut out and used in `description`.
    ///
    /// When filtering, only `description` is used. When doing full search, search tries to find in
    /// `description` first, because it is indexed, then tries to find in `long_description` if it
    /// fails to find in `description`.
    pub long_description: Option<String>,
    /// For grouping lending and borrowing. Should be set only when `entry_type` is
    /// `EntryType::Borrow` or `EntryType::Lend`
    pub target: Option<String>,
    #[entity(RepresentableAsString)]
    pub category_id: i32,
    /// The amount input by the user, preserved as-is. The currency for this is the `currency_id`
    /// input by the user if any, and the currency of `source_id` if no currency was input.
    ///
    /// Positive amounts always add to `source_id` while negative amounts always subtract from it.
    /// When displayed, they are displayed with a color instead of a sign, and the EntryType is
    /// used to further indicate why they have this color.
    ///
    /// `amount` and all other amount-based values are not updatable. If you wish to update them,
    /// simply delete the entry and recreate it. This is to prevent confusion related to source
    /// value changes due to possible entry currency / amount changes in update.
    #[entity(NotInDatabaseUpdate, NotInUpdateRequest)]
    pub amount: f64,
    /// The amount input by the user, converted to the fixed currency.
    #[entity(NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest)]
    pub amount_in_fixed: f64,
    /// If `currency_id` is provided, we use it to denominate the amount of the entry.
    /// `currency_id` cannot be provided for entries of type `EntryType::Convert`, and are ignored
    /// if they are.
    ///
    /// If `currency_id` is not provided for entries of any type, the `currency_id` of the source
    /// is used.
    #[entity(RepresentableAsString, NotInDatabaseUpdate, NotInUpdateRequest, HasCalculatedDefault)]
    pub currency_id: i32,
    #[entity(NotInDatabaseUpdate, NotInUpdateRequest)]
    pub entry_type: EntryType,
    #[entity(RepresentableAsString, NotInDatabaseUpdate, NotInUpdateRequest)]
    pub source_id: i32,
    /// Specified if `currency_id` and `source_id` are of different currencies.
    /// Otherwise, uses the calculated default, which is the same exact amount as the specified
    /// `amount`. Like `currency_id`, this is ignored for entries of type `EntryType::Convert`.
    #[entity(HasCalculatedDefault, NotInDatabaseUpdate, NotInUpdateRequest)]
    pub source_amount: f64,
    /// Only for entry_type of `EntryType::Convert`, as it converts money from one currency to
    /// another, for two provided sources of different currencies. The source `from` is
    /// `source_id`, while the source `to` is `secondary_source_id`.
    #[entity(RepresentableAsString, NotInDatabaseUpdate, NotInUpdateRequest)]
    pub secondary_source_id: Option<i32>,
    #[entity(NotInDatabaseUpdate, NotInUpdateRequest)]
    pub secondary_source_amount: Option<f64>,
    /// Conversion rates for currencies may change, so we store the conversion rate at which this
    /// entry took place inside the entry itself, to keep track of how much it was worth at the
    /// time. This is only present for entries of type `EntryType::Convert` or for those in which
    /// `currency_id` is provided and is different from that of the provided `source_id`.
    ///
    /// This is `to / from`, so for example, the conversion rate for USD->JPY is 150.
    ///
    /// In the case of providing a `conversion_rate` for a non-`EntryType::Convert` entry, the
    /// `conversion_rate`'s `to` is the specified currency, and `from` is the `source_id`'s
    /// currency. For example, the conversion rate for specified currency = JPY when the source
    /// is a USD source is 150.
    ///
    /// If not present, it is filled using the value from currency. For the cases in which the
    /// `currency_id` is not provided, or it is the same as the one from `source_id`, this uses the
    /// default value of `1`, making it always-present.
    #[entity(NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest)]
    pub conversion_rate: f64,
    /// This is fetched from the currency itself for anything but those of type `Entry::Convert`,
    /// in which case it faithfully follows `conversion_rate` if specified, and is fetched from
    /// `rate_to_fixed` of the primary currency if not.
    ///
    /// This is the conversion rate of the amount converted to fixed.
    #[entity(NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest)]
    pub conversion_rate_to_fixed: f64,
    #[entity(RepresentableAsString)]
    pub date: NaiveDateTime,
    #[entity(
        RepresentableAsString,
        NotInDatabaseUpdate,
        NotInUpdateRequest,
        NotInCreateRequest,
        HasDefault
    )]
    pub created_at: NaiveDateTime,
    #[entity(HasDefault, NotInCreateRequest)]
    pub archived: bool,
}

impl StatefulTryFrom<CreateEntryRequest> for NewEntry {
    fn stateful_try_from(
        value: CreateEntryRequest,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        // To confirm this logic, take an example in which USD is fixed,
        // 300 JPY from an EGP source with JPY rate_to_fixed = 0.00667 and EGP rate_to_fixed = 0.02
        // This should withdraw 100 EGP from the source and track as 2 fixed USDs consumed.
        // For the secondary_* bindings, we can assume the 300 JPY are to be deposited in a
        // secondary source in an entry of type `EntryType::Convert`.

        let primary_source = Source::get_by_name_and_user(value.source, &user, app_state.clone())?;
        let primary_source_currency =
            Currency::get_by_id(primary_source.currency_id, app_state.clone())?;
        // We take the id and rate_to_fixed out because we're going to consume
        // primary_source_currency.
        let primary_source_currency_id = primary_source_currency.id;
        let primary_source_currency_rtf = primary_source_currency.rate_to_fixed;
        let secondary_source =
            Source::get_by_name_and_user(value.secondary_source, &user, app_state.clone())?;
        let secondary_source_currency = Currency::get_by_id(
            secondary_source.as_ref().map(|o| o.currency_id),
            app_state.clone(),
        )?;
        // to_currency = JPY, secondary_source_id = _, secondary_source_amount = 300 (if convert)
        let (to_currency, secondary_source_id, secondary_source_amount) = match (
            &value.entry_type,
            &value.source_amount,
            value.currency,
            &secondary_source,
            &value.secondary_source_amount,
        ) {
            (EntryType::Convert, None, None, Some(s), Some(a)) => match secondary_source_currency {
                None => {
                    return Err(StatefulTryFromError::LogicBadRequestError(Box::from(
                        "Malformed CreateEntryRequest: Entries of type convert should always \
                         specify a secondary source",
                    )))
                }
                Some(c) => (c, Some(s.id), Some(*a)),
            },
            (e, _, maybe_currency, None, None) if *e != EntryType::Convert => (
                Currency::get_by_name_and_user(maybe_currency, &user, app_state.clone())?
                    .unwrap_or(primary_source_currency),
                None,
                None,
            ),
            _ => {
                return Err(StatefulTryFromError::LogicBadRequestError(Box::from(
                    "Malformed CreateEntryRequest: Entry specifying wrong parameters for \
                     source_amount / currency / secondary_source_id / secondary_source_amount",
                )))
            }
        };
        // source_amount = 100 (from input)
        let source_amount = if to_currency.id != primary_source_currency_id {
            match value.source_amount {
                None => {
                    return Err(StatefulTryFromError::LogicBadRequestError(Box::from(
                        "Malformed CreateEntryRequest: Currency different from primary specified, \
                         but no source amount specified",
                    )))
                }
                Some(o) => o,
            }
        } else {
            value.amount
        };
        // amount_in_fixed = 100 * 0.02 = 2 (exact to rate)
        let amount_in_fixed = source_amount * primary_source_currency_rtf;
        // conversion_rate = 0.00667 / 0.02 = 0.3335 (not exact)
        // Anything that uses this will not be exact unless either currency or primary is fixed.
        // Therefore, this should never be used, we should always rely on source amount.
        let conversion_rate = to_currency.rate_to_fixed / primary_source_currency_rtf;
        // conversion_rate_to_fixed = 0.00667
        let conversion_rate_to_fixed = to_currency.rate_to_fixed;

        Ok(Self {
            user_id: user.id,
            description: value.description,
            long_description: value.long_description,
            target: value.target,
            category_id: Category::get_id_by_name_and_user(
                value.category.as_str(),
                &user,
                app_state.clone(),
            )?,
            amount: value.amount,
            date: NaiveDate::parse_from_str(value.date.as_str(), "%F")?.into(),
            created_at: None,
            entry_type: value.entry_type,
            currency_id: to_currency.id,
            amount_in_fixed,
            conversion_rate,
            conversion_rate_to_fixed,
            source_id: primary_source.id,
            source_amount,
            secondary_source_id,
            secondary_source_amount,
            archived: None,
        })
    }
}

/// See the `CreateEntryRequest` implementation for more details on the logic.
impl StatefulTryFrom<UpdateEntryRequest> for UpdateEntry {
    fn stateful_try_from(
        value: UpdateEntryRequest,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self {
            description: value.description,
            long_description: value.long_description,
            target: value.target,
            category_id: Category::get_id_by_name_and_user(
                value.category,
                &user,
                app_state.clone(),
            )?,
            date: match value.date {
                None => None,
                Some(c) => Some(NaiveDate::parse_from_str(c.as_str(), "%F")?.into()),
            },
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
            long_description: value.long_description,
            target: value.target,
            category: Category::get_name_by_id(value.category_id, app_state.clone())?,
            amount: value.amount,
            amount_in_fixed: value.amount_in_fixed,
            date: value.date.format("%F").to_string(),
            created_at: value.created_at.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
            currency: Currency::get_name_by_id(value.currency_id, app_state.clone())?,
            entry_type: value.entry_type,
            source: Source::get_name_by_id(value.source_id, app_state.clone())?,
            source_amount: value.source_amount,
            secondary_source: Source::get_name_by_id(value.secondary_source_id, app_state.clone())?,
            secondary_source_amount: value.secondary_source_amount,
            conversion_rate: value.conversion_rate,
            conversion_rate_to_fixed: value.conversion_rate_to_fixed,
            archived: value.archived,
        })
    }
}

/// - ids (IN) - for multi-select
/// - sources (IN)
/// - categories (IN)
/// - currencies (IN)
/// - currency (EQ) - takes precedence over currencies
/// - amount (EQ - care float) - must also specify currency
/// - min_amount (GTE)
/// - max_amount (LTE)
/// - min_amount_in_fixed (GTE) - does not need currency, uses fixed, compares to all entries
/// - max_amount_in_fixed (LTE) - does not need currency, uses fixed, compares to all entries
/// - date (EQ)
/// - after (GTE)
/// - before (LTE)
/// - created_after (GTE)
/// - created_before (LTE)
/// - description (LIKE)
/// - entry_types (IN)
/// - limit (default: 500)
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct EntryQuery {
    pub ids: Option<Vec<i32>>,
    pub sources: Option<Vec<String>>,
    pub categories: Option<Vec<String>>,
    pub currencies: Option<Vec<String>>,
    pub currency: Option<String>,
    pub amount: Option<f64>,
    pub min_amount: Option<f64>,
    pub max_amount: Option<f64>,
    pub min_amount_in_fixed: Option<f64>,
    pub max_amount_in_fixed: Option<f64>,
    pub date: Option<String>,
    pub after: Option<String>,
    pub before: Option<String>,
    pub created_after: Option<String>,
    pub created_before: Option<String>,
    pub description: Option<String>,
    pub entry_types: Option<Vec<EntryType>>,
    pub limit: Option<i64>,
    pub sort: Option<String>,
}

impl Entry {
    pub fn find_by_filter(
        query_params: &EntryQuery,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Vec<Entry>, StatefulTryFromError> {
        use crate::schema::entries::dsl::*;
        let mut query = entries.into_boxed();

        let amount_specified =
            &query_params.amount.or(query_params.min_amount).or(query_params.max_amount);

        if amount_specified.is_some() && query_params.currency.is_none() {
            return Err(StatefulTryFromError::LogicBadRequestError(Box::from(
                "If you specify amount(s), you should always also specify currency. If you want \
                 to do currency-agnostic comparison, use *_amount_in_fixed instead.",
            )));
        }

        let currencies = if let Some(c) = &query_params.currency {
            if let Some(cs) = &query_params.currencies {
                warn!("Currency has been specified: {:?}, overriding currencies: {:?}", c, cs);
            }
            &Some(vec![c.clone()])
        } else {
            &query_params.currencies
        };

        if let Some(ids) = &query_params.ids {
            query = query.filter(id.eq_any(ids));
        }

        if let Some(names) = &query_params.sources {
            let ids: Vec<_> = names
                .iter()
                .filter_map(|name| {
                    Source::get_id_by_name_and_user(name.as_str(), &user, app_state.clone()).ok()
                })
                .collect();

            query = query.filter(source_id.eq_any(ids));
        }
        if let Some(names) = currencies {
            let ids: Vec<_> = names
                .iter()
                .filter_map(|name| {
                    Currency::get_id_by_name_and_user(name.as_str(), &user, app_state.clone()).ok()
                })
                .collect();

            query = query.filter(currency_id.eq_any(ids));
        }
        if let Some(names) = &query_params.categories {
            let ids: Vec<_> = names
                .iter()
                .filter_map(|name| {
                    Category::get_id_by_name_and_user(name.as_str(), &user, app_state.clone()).ok()
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

        if let Some(min_amount_in_fixed) = query_params.min_amount_in_fixed {
            query = query.filter(amount_in_fixed.ge(min_amount_in_fixed));
        }

        if let Some(max_amount_in_fixed) = query_params.max_amount_in_fixed {
            query = query.filter(amount_in_fixed.le(max_amount_in_fixed));
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
