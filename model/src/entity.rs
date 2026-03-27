use std::collections::HashMap;
use chrono::{NaiveDateTime};
use serde::{Deserialize, Serialize};

// Needed by macros, keep even if "unused"
#[allow(unused_imports)]
#[rustfmt::skip]
use {
    crate::numeric::Numeric, // inner_macros relies on this type
    fpdec::Decimal, // inner_macros relies on this type
    inner_macros::Entity // The `inner_macros::Entity` derivable macro itself
};
#[allow(unused_imports)]
#[rustfmt::skip]
#[cfg(feature = "backend")]
use {
    crate::schema::sql_types::EntryT, // used by #[ExistingTypePath = "EntryT"]
    crate::schema::*, // Used by `diesel_derive_enum::DbEnum`
};

#[cfg(feature = "backend")]
use diesel::{
    Associations, Identifiable,
    Insertable, Queryable, Selectable,
};


#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "backend", derive(diesel_derive_enum::DbEnum))]
#[cfg_attr(feature = "backend", ExistingTypePath = "EntryT")]
pub enum EntryType {
    Spend,
    Income,
    Lend,
    Borrow,
    Convert,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "backend", derive(Queryable, Selectable, Identifiable))]
#[cfg_attr(feature = "backend", diesel(table_name = users))]
#[cfg_attr(feature = "backend", diesel(check_for_backend(diesel::pg::Pg)))]
pub struct User {
    pub id: i32,
    pub username: String,
    pub password: String,
    pub fixed_currency_id: Option<i32>,
    pub enabled: bool,
}

#[cfg_attr(feature = "backend", derive(Insertable))]
#[cfg_attr(feature = "backend", diesel(table_name = users))]
#[cfg_attr(feature = "backend", diesel(check_for_backend(diesel::pg::Pg)))]
pub struct NewUser {
    pub username: String,
    pub password: String,
}

#[derive(Entity, Debug, Serialize)]
#[cfg_attr(feature = "backend", derive(Queryable, Selectable, Identifiable, Associations, Insertable))]
#[cfg_attr(feature = "backend", diesel(table_name = currencies))]
#[cfg_attr(feature = "backend", diesel(belongs_to(User)))]
#[cfg_attr(feature = "backend", diesel(check_for_backend(diesel::pg::Pg)))]
#[serde(deny_unknown_fields)]
pub struct Currency {
    #[entity(NotInResponse, NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest, Id)]
    pub id: i32,
    #[entity(NotInResponse, NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest)]
    pub user_id: i32,
    pub name: String,
    /// This is the amount of fixed currency that fits within 1 this currency that fits within.
    /// For example, the JPY rate_to_fixed would be 0.00667 if the USD is fixed.
    pub rate_to_fixed: Numeric,
    #[entity(HasDefault, NotInCreateRequest)]
    pub archived: bool,
}

#[derive(Entity, Debug, Serialize)]
#[cfg_attr(feature = "backend", derive(Queryable, Selectable, Identifiable, Associations, Insertable))]
#[cfg_attr(feature = "backend", diesel(table_name = sources))]
#[cfg_attr(feature = "backend", diesel(belongs_to(User)))]
#[cfg_attr(feature = "backend", diesel(belongs_to(Currency)))]
#[cfg_attr(feature = "backend", diesel(check_for_backend(diesel::pg::Pg)))]
#[serde(deny_unknown_fields)]
pub struct Source {
    #[entity(NotInResponse, NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest, Id)]
    pub id: i32,
    #[entity(NotInResponse, NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest)]
    pub user_id: i32,
    pub name: String,
    #[entity(RepresentableAsString, NotInDatabaseUpdate, NotInUpdateRequest)]
    pub currency_id: i32,
    #[entity(HasDefault)]
    pub amount: Numeric,
    #[entity(HasDefault)]
    pub archived: bool,
}

#[derive(Entity, Debug, Serialize)]
#[cfg_attr(feature = "backend", derive(Queryable, Selectable, Identifiable, Associations, Insertable))]
#[cfg_attr(feature = "backend", diesel(table_name = categories))]
#[cfg_attr(feature = "backend", diesel(belongs_to(User)))]
#[cfg_attr(feature = "backend", diesel(check_for_backend(diesel::pg::Pg)))]
#[serde(deny_unknown_fields)]
pub struct Category {
    #[entity(NotInResponse, NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest, Id)]
    pub id: i32,
    #[entity(NotInResponse, NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest)]
    pub user_id: i32,
    pub name: String,
    #[entity(HasDefault)]
    pub archived: bool,
}

#[derive(Entity, Debug, Serialize)]
#[cfg_attr(feature = "backend", derive(Queryable, Selectable, Identifiable, Associations, Insertable))]
#[cfg_attr(feature = "backend", diesel(table_name = entries))]
#[cfg_attr(feature = "backend", diesel(belongs_to(User)))]
#[cfg_attr(feature = "backend", diesel(belongs_to(Source)))]
#[cfg_attr(feature = "backend", diesel(belongs_to(Category)))]
#[cfg_attr(feature = "backend", diesel(belongs_to(Currency)))]
#[cfg_attr(feature = "backend", diesel(check_for_backend(diesel::pg::Pg)))]
#[serde(deny_unknown_fields)]
pub struct Entry {
    #[entity(NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest, Id)]
    pub id: i32,
    #[entity(NotInResponse, NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest)]
    pub user_id: i32,
    /// User-entered description, we can match this to previously entered descriptions and try to
    /// decide values for other fields. In the case of multi-line descriptions, the first line of
    /// the description is displayed and used for filtering, while the rest of the description is
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
    pub amount: Numeric,
    /// The amount input by the user, converted to the fixed currency.
    #[entity(NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest)]
    pub amount_in_fixed: Numeric,
    /// If `currency_id` is provided, we use it to denominate the amount of the entry.
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
    pub source_amount: Numeric,
    /// Only for entry_type of `EntryType::Convert`, as it converts money from one currency to
    /// another, for two provided sources of different currencies. The source `from` is
    /// `source_id`, while the source `to` is `secondary_source_id`.
    #[entity(RepresentableAsString, NotInDatabaseUpdate, NotInUpdateRequest)]
    pub secondary_source_id: Option<i32>,
    #[entity(NotInDatabaseUpdate, NotInUpdateRequest)]
    pub secondary_source_amount: Option<Numeric>,
    /// Conversion rates for currencies may change, so we store the conversion rate at which this
    /// entry took place inside the entry itself, to keep track of how much it was worth at the
    /// time. This is only present for entries of type `EntryType::Convert` or for those in which
    /// `currency_id` is provided and is different from that of the provided `source_id`.
    ///
    /// This is `from_rtf / to_rtf`, so for example, the conversion rate for EGP->JPY is 3.
    ///
    /// It is filled using the value from currency. For the cases in which the
    /// `currency_id` is not provided, or it is the same as the one from `source_id`, this uses the
    /// default value of `1`, making it always-present.
    #[entity(NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest)]
    pub conversion_rate: Numeric,
    /// This is fetched from the currency itself for anything but those of type `Entry::Convert`,
    /// in which case it faithfully follows `conversion_rate` if specified, and is fetched from
    /// `rate_to_fixed` of the primary currency if not.
    ///
    /// This is the conversion rate of the amount converted to fixed.
    #[entity(NotInDatabaseUpdate, NotInUpdateRequest, NotInCreateRequest)]
    pub conversion_rate_to_fixed: Numeric,
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
/// Needs Clone trait because it is used in the front-end state
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct EntryQuery {
    pub ids: Option<Vec<i32>>,
    pub sources: Option<Vec<String>>,
    pub categories: Option<Vec<String>>,
    pub currencies: Option<Vec<String>>,
    pub currency: Option<String>,
    pub amount: Option<Decimal>,
    pub min_amount: Option<Decimal>,
    pub max_amount: Option<Decimal>,
    pub min_amount_in_fixed: Option<Decimal>,
    pub max_amount_in_fixed: Option<Decimal>,
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoginResponse {
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub currency: String,
}

// We cannot skip serialization in any of the fields in the response, as in the tests,
// we will need to reconstruct the response from the JSON string to reason about it,
// to not have to write code that uses maps.
//
// The exception is CreateResponse, which serializes as empty response.

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateResponse {
    #[allow(dead_code)]
    #[serde(skip_serializing)]
    pub id: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EmptyResponse {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeBasedRequest {
    pub now: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CategoryStatsResponse {
    pub year_sum_in_fixed: Decimal,
    pub monthly_average_in_fixed: Decimal,
    pub month_breakdown_in_fixed: Vec<Decimal>,
    pub current_month_in_fixed: Decimal,
    pub year_largest_spends: Vec<EntryResponse>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CurrencyStatsResponse {
    pub year_sum: Decimal,
    pub monthly_average: Decimal,
    pub month_breakdown: Vec<Decimal>,
    pub current_month: Decimal,
    pub year_largest_spends: Vec<EntryResponse>,
}

/// Used only when performing group-operation on entries (not entities).
/// Examples are deleting and archiving and block-updating entries.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CountResponse {
    pub count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimplePaginatedRequest {
    pub page: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEntriesRequest {
    pub page: Option<u32>,
    pub primary_only: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FindEntriesResponse {
    pub sum_per_month: HashMap<String, Decimal>,
    pub monthly_average: Decimal,
    pub sum_per_category_per_month: HashMap<String, Decimal>,
    pub entries: Vec<EntryResponse>,
}
