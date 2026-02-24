use std::sync::Arc;

use async_trait::async_trait;
use chrono::{NaiveDate, NaiveDateTime};
use diesel::{
    BelongingToDsl, BoolExpressionMethods, ExpressionMethods, PgTextExpressionMethods, QueryDsl,
};
use diesel_async::RunQueryDsl as _;
use futures::future::join_all;
use log::warn;

use crate::AppState;

#[rustfmt::skip]
use {
    model::entity::*, // Entities like User and Currency
    model::numeric::Numeric, // inner_macros relies on this type
    fpdec::Decimal, // inner_macros relies on this type
};

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

macro_rules! lbr {
    ($error:tt) => {
        return Err(StatefulTryFromError::LogicBadRequestError(Box::from($error)))
    };
}

#[async_trait]
pub trait GetIdByNameAndUser<N, T> {
    async fn get_id_by_name_and_user(
        name: N,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<T, diesel::result::Error>
    where
        N: 'async_trait,
        T: 'async_trait;
}

#[async_trait]
pub trait GetByNameAndUser<N, T> {
    async fn get_by_name_and_user(
        name: N,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<T, diesel::result::Error>
    where
        N: 'async_trait,
        T: 'async_trait;
}

#[async_trait]
pub trait GetNameById<N, T> {
    async fn get_name_by_id(id: N, app_state: Arc<AppState>) -> Result<T, diesel::result::Error>
    where
        N: 'async_trait,
        T: 'async_trait;
}

#[async_trait]
pub trait GetById<N, T> {
    async fn get_by_id(id: N, app_state: Arc<AppState>) -> Result<T, diesel::result::Error>
    where
        N: 'async_trait,
        T: 'async_trait;
}

#[async_trait]
pub trait GetNetAmount {
    async fn get_net_amount(
        &self,
        app_state: Arc<AppState>,
    ) -> Result<Decimal, diesel::result::Error>;
}

#[async_trait]
impl GetNetAmount for Currency {
    async fn get_net_amount(
        &self,
        app_state: Arc<AppState>,
    ) -> Result<Decimal, diesel::result::Error> {
        use model::schema::sources::dsl::*;
        let entry_amount_sum = Source::belonging_to(&self)
            .filter(archived.eq(false))
            .select(amount)
            .load::<Numeric>(&mut app_state.cpool().await)
            .await?
            .into_iter()
            .sum();
        Ok(entry_amount_sum)
    }
}

#[async_trait]
impl GetNetAmount for Source {
    async fn get_net_amount(
        &self,
        _app_state: Arc<AppState>,
    ) -> Result<Decimal, diesel::result::Error> {
        Ok((&self.amount).into())
    }
}

#[async_trait]
impl GetNetAmount for Category {
    async fn get_net_amount(
        &self,
        app_state: Arc<AppState>,
    ) -> Result<Decimal, diesel::result::Error> {
        use diesel::dsl::sum;
        use model::schema::entries::dsl::*;
        let entry_amount_sum = Entry::belonging_to(&self)
            .filter(archived.eq(false))
            .select(sum(amount))
            .load::<Option<Numeric>>(&mut app_state.cpool().await)
            .await?
            .into_iter()
            .map(|x| x.unwrap_or(Numeric::default()))
            .sum();
        Ok(entry_amount_sum)
    }
}

macro_rules! get_impls {
    ($type:ty, $tb_name:ident) => {
        #[async_trait]
        impl<T> GetIdByNameAndUser<T, i32> for $type
        where
            T: Into<String> + Send,
        {
            async fn get_id_by_name_and_user(
                p_name: T,
                user: &User,
                app_state: Arc<AppState>,
            ) -> Result<i32, diesel::result::Error>
            where
                T: 'async_trait,
            {
                use model::schema::$tb_name::dsl::*;
                $tb_name
                    .filter(name.eq(p_name.into()).and(user_id.eq(user.id)))
                    .select(id)
                    .first(&mut app_state.cpool().await)
                    .await
            }
        }

        #[async_trait]
        impl<T> GetIdByNameAndUser<Option<T>, Option<i32>> for $type
        where
            T: Into<String> + Send,
        {
            async fn get_id_by_name_and_user(
                p_name: Option<T>,
                user: &User,
                app_state: Arc<AppState>,
            ) -> Result<Option<i32>, diesel::result::Error>
            where
                T: 'async_trait,
            {
                Ok(match p_name {
                    None => None,
                    Some(c) => {
                        use model::schema::$tb_name::dsl::*;
                        Some(
                            $tb_name
                                .filter(name.eq(c.into()).and(user_id.eq(user.id)))
                                .select(id)
                                .first(&mut app_state.cpool().await)
                                .await?,
                        )
                    }
                })
            }
        }

        #[async_trait]
        impl<T> GetByNameAndUser<T, $type> for $type
        where
            T: Into<String> + Send,
        {
            async fn get_by_name_and_user(
                p_name: T,
                user: &User,
                app_state: Arc<AppState>,
            ) -> Result<$type, diesel::result::Error>
            where
                T: 'async_trait,
            {
                use model::schema::$tb_name::dsl::*;
                $tb_name
                    .filter(name.eq(p_name.into()).and(user_id.eq(user.id)))
                    .first(&mut app_state.cpool().await)
                    .await
            }
        }

        #[async_trait]
        impl<T> GetByNameAndUser<Option<T>, Option<$type>> for $type
        where
            T: Into<String> + Send,
        {
            async fn get_by_name_and_user(
                p_name: Option<T>,
                user: &User,
                app_state: Arc<AppState>,
            ) -> Result<Option<$type>, diesel::result::Error>
            where
                T: 'async_trait,
            {
                Ok(match p_name {
                    None => None,
                    Some(c) => {
                        use model::schema::$tb_name::dsl::*;
                        Some(
                            $tb_name
                                .filter(name.eq(c.into()).and(user_id.eq(user.id)))
                                .first(&mut app_state.cpool().await)
                                .await?,
                        )
                    }
                })
            }
        }

        #[async_trait]
        impl GetNameById<i32, String> for $type {
            async fn get_name_by_id(
                p_id: i32,
                app_state: Arc<AppState>,
            ) -> Result<String, diesel::result::Error> {
                use model::schema::$tb_name::dsl::*;
                $tb_name.find(p_id).select(name).first(&mut app_state.cpool().await).await
            }
        }

        #[async_trait]
        impl GetNameById<Option<i32>, Option<String>> for $type {
            async fn get_name_by_id(
                p_id: Option<i32>,
                app_state: Arc<AppState>,
            ) -> Result<Option<String>, diesel::result::Error> {
                Ok(match p_id {
                    None => None,
                    Some(c) => {
                        use model::schema::$tb_name::dsl::*;
                        Some(
                            $tb_name
                                .find(c)
                                .select(name)
                                .first(&mut app_state.cpool().await)
                                .await?,
                        )
                    }
                })
            }
        }

        #[async_trait]
        impl GetById<i32, $type> for $type {
            async fn get_by_id(
                p_id: i32,
                app_state: Arc<AppState>,
            ) -> Result<$type, diesel::result::Error> {
                use model::schema::$tb_name::dsl::*;
                $tb_name.find(p_id).first(&mut app_state.cpool().await).await
            }
        }

        #[async_trait]
        impl GetById<Option<i32>, Option<$type>> for $type {
            async fn get_by_id(
                p_id: Option<i32>,
                app_state: Arc<AppState>,
            ) -> Result<Option<$type>, diesel::result::Error> {
                Ok(match p_id {
                    None => None,
                    Some(c) => {
                        use model::schema::$tb_name::dsl::*;
                        Some($tb_name.find(c).first(&mut app_state.cpool().await).await?)
                    }
                })
            }
        }
    };
}

get_impls!(Currency, currencies);
get_impls!(Category, categories);
get_impls!(Source, sources);

#[async_trait]
impl GetById<i32, Entry> for Entry {
    async fn get_by_id(
        p_id: i32,
        app_state: Arc<AppState>,
    ) -> Result<Entry, diesel::result::Error> {
        use model::schema::entries::dsl::*;
        entries.find(p_id).first(&mut app_state.cpool().await).await
    }
}

#[async_trait]
pub trait StatefulTryFrom<S> {
    async fn stateful_try_from(
        value: S,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError>
    where
        S: Send,
        Self: Sized;
}

#[async_trait]
impl StatefulTryFrom<CreateCurrencyRequest> for NewCurrency {
    async fn stateful_try_from(
        value: CreateCurrencyRequest,
        user: &User,
        _app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self {
            user_id: user.id,
            name: value.name,
            rate_to_fixed: value.rate_to_fixed.into(),
            archived: None,
        })
    }
}

#[async_trait]
impl StatefulTryFrom<UpdateCurrencyRequest> for UpdateCurrency {
    async fn stateful_try_from(
        value: UpdateCurrencyRequest,
        _user: &User,
        _app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self {
            name: value.name,
            rate_to_fixed: value.rate_to_fixed.map(Numeric::from),
            archived: value.archived,
        })
    }
}

#[async_trait]
impl StatefulTryFrom<Currency> for CurrencyResponse {
    async fn stateful_try_from(
        value: Currency,
        _user: &User,
        _app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self {
            name: value.name,
            rate_to_fixed: value.rate_to_fixed.into(),
            archived: value.archived,
        })
    }
}

#[async_trait]
impl StatefulTryFrom<CreateSourceRequest> for NewSource {
    async fn stateful_try_from(
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
            )
            .await?,
            amount: value.amount.map(Numeric::from),
            archived: value.archived,
        })
    }
}

#[async_trait]
impl StatefulTryFrom<UpdateSourceRequest> for UpdateSource {
    async fn stateful_try_from(
        value: UpdateSourceRequest,
        _user: &User,
        _app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self {
            name: value.name,
            amount: value.amount.map(Numeric::from),
            archived: value.archived,
        })
    }
}

#[async_trait]
impl StatefulTryFrom<Source> for SourceResponse {
    async fn stateful_try_from(
        value: Source,
        _user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self {
            name: value.name,
            currency: Currency::get_name_by_id(value.currency_id, app_state.clone()).await?,
            amount: value.amount.into(),
            archived: value.archived,
        })
    }
}

#[async_trait]
impl StatefulTryFrom<CreateCategoryRequest> for NewCategory {
    async fn stateful_try_from(
        value: CreateCategoryRequest,
        user: &User,
        _app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self { user_id: user.id, name: value.name, archived: value.archived })
    }
}

#[async_trait]
impl StatefulTryFrom<UpdateCategoryRequest> for UpdateCategory {
    async fn stateful_try_from(
        value: UpdateCategoryRequest,
        _user: &User,
        _app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self { name: value.name, archived: value.archived })
    }
}

#[async_trait]
impl StatefulTryFrom<Category> for CategoryResponse {
    async fn stateful_try_from(
        value: Category,
        _user: &User,
        _app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self { name: value.name, archived: value.archived })
    }
}

#[async_trait]
impl StatefulTryFrom<CreateEntryRequest> for NewEntry {
    async fn stateful_try_from(
        value: CreateEntryRequest,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        // CreateEntryRequest has:
        // Non-functional: description, long_description, category_id, date
        // Functional: amount, source_id
        // Non-convert: currency_id, source_amount
        // Convert: secondary_source_id, secondary_source_amount
        // Borrow/Lend: target
        //
        // To confirm this logic, take an example in which USD is fixed,
        // 300 JPY from an EGP source with:
        // - JPY rate_to_fixed = 0.00667
        // - EGP rate_to_fixed = 0.02
        // This should withdraw 100 EGP from the source and track as 2 fixed USDs consumed.
        // For the secondary_* bindings, we can assume that the 300 JPY are to be deposited in a
        // secondary source in an entry of type `EntryType::Convert`.

        let primary_source =
            Source::get_by_name_and_user(value.source, &user, app_state.clone()).await?;
        let primary_source_currency =
            Currency::get_by_id(primary_source.currency_id, app_state.clone()).await?;
        let value_currency =
            Currency::get_by_name_and_user(value.currency.as_ref(), &user, app_state.clone())
                .await?;

        let currency = match &value_currency {
            Some(c) => c,
            None => &primary_source_currency,
        };

        // primary_source_amount = 100 (from conversion from 300 JPY to EGP, not exact)
        let source_amount = match (value.source_amount, &value_currency) {
            // Explicitly specified source amount in request.
            // Then, provided value amount is just for show.
            (Some(a), _) => a,
            // Explicitly provided currency in request.
            // Then, the amount in request is assumed to be of that currency, and converted
            // using the rtf conversion.
            (None, Some(c)) => convert_currency(value.amount, c, &primary_source_currency),
            // No source amount or currency specified in request.
            // Then, the amount in request is assumed to be of the primary source directly.
            (None, None) => value.amount,
        };
        // 100 * 0.02 = 2 USD (exact to rate)
        let amount_in_fixed = value.amount * currency.rate_to_fixed;
        let conversion_rate;
        let conversion_rate_to_fixed;
        let mut secondary_source_id = None;
        match &value.entry_type {
            EntryType::Convert => {
                let secondary_source = match Source::get_by_name_and_user(
                    value.secondary_source,
                    &user,
                    app_state.clone(),
                )
                .await?
                {
                    Some(s) => s,
                    None => lbr!(
                        "Malformed CreateEntryRequest: Convert without secondary source (or with \
                         an invalid secondary source)"
                    ),
                };
                let secondary_source_currency =
                    Currency::get_by_id(secondary_source.currency_id, app_state.clone()).await?;
                secondary_source_id = Some(secondary_source.id);
                if value.secondary_source_amount.is_none() {
                    lbr!("Malformed CreateEntryRequest: Convert without secondary source amount");
                };
                if value.source_amount.is_some()
                    && let Some(a) = value.source_amount
                    && a != source_amount
                {
                    lbr!(
                        "Malformed CreateEntryRequest: Convert specified `source_amount` is a bad \
                         idea so we disable it. Only specify the `amount` field"
                    );
                };
                if value.currency.is_some()
                    && let Some(c) = value.currency
                    && c != primary_source_currency.name
                {
                    lbr!(
                        "Malformed CreateEntryRequest: Convert specified `currency` is a bad idea \
                         so we disable it. Let's just use the currency from primary source"
                    );
                };
                // amount_in_fixed = 100 * 0.02 = 2 (exact to rate)
                // conversion_rate = 0.02 / 0.00667 = 3 (not exact)
                // Anything that uses this will not be exact unless either currency or primary is
                // fixed. Therefore, this should never be used, we should always
                // rely on source amount.
                conversion_rate =
                    primary_source_currency.rate_to_fixed / secondary_source_currency.rate_to_fixed;
                conversion_rate_to_fixed = secondary_source_currency.rate_to_fixed;
            }
            e => {
                if (*e == EntryType::Borrow || *e == EntryType::Lend) && value.target.is_none() {
                    lbr!("Malformed CreateEntryRequest: Borrow/lend require a `target`");
                }
                // amount + source -> amount is in the currency of source and is subtracted
                if value.secondary_source.is_some() {
                    lbr!(
                        "Malformed CreateEntryRequest: Non-convert with specified secondary source"
                    );
                }
                if value.secondary_source_amount.is_some() {
                    lbr!(
                        "Malformed CreateEntryRequest: Non-convert with specified secondary \
                         source amount"
                    );
                }
                // From the specified value currency to the primary source's.
                conversion_rate = currency.rate_to_fixed / primary_source_currency.rate_to_fixed;
                conversion_rate_to_fixed = primary_source_currency.rate_to_fixed.clone();
            }
        };

        Ok(Self {
            user_id: user.id,
            description: value.description,
            long_description: value.long_description,
            target: value.target,
            category_id: Category::get_id_by_name_and_user(
                value.category.as_str(),
                &user,
                app_state.clone(),
            )
            .await?,
            amount: value.amount.into(),
            date: NaiveDate::parse_from_str(value.date.as_str(), "%F")?.into(),
            created_at: None,
            entry_type: value.entry_type,
            currency_id: currency.id,
            amount_in_fixed: amount_in_fixed.into(),
            conversion_rate: conversion_rate.into(),
            conversion_rate_to_fixed,
            source_id: primary_source.id,
            source_amount: source_amount.into(),
            secondary_source_id,
            secondary_source_amount: value.secondary_source_amount.map(Numeric::from),
            archived: None,
        })
    }
}

/// See the `CreateEntryRequest` implementation for more details on the logic.
#[async_trait]
impl StatefulTryFrom<UpdateEntryRequest> for UpdateEntry {
    async fn stateful_try_from(
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
            )
            .await?,
            date: match value.date {
                None => None,
                Some(c) => Some(NaiveDate::parse_from_str(c.as_str(), "%F")?.into()),
            },
            archived: value.archived,
        })
    }
}

#[async_trait]
impl StatefulTryFrom<Entry> for EntryResponse {
    async fn stateful_try_from(
        value: Entry,
        _user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Self, StatefulTryFromError> {
        Ok(Self {
            id: value.id,
            description: value.description,
            long_description: value.long_description,
            target: value.target,
            category: Category::get_name_by_id(value.category_id, app_state.clone()).await?,
            amount: value.amount.into(),
            amount_in_fixed: value.amount_in_fixed.into(),
            date: value.date.format("%F").to_string(),
            created_at: value.created_at.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
            currency: Currency::get_name_by_id(value.currency_id, app_state.clone()).await?,
            entry_type: value.entry_type,
            source: Source::get_name_by_id(value.source_id, app_state.clone()).await?,
            source_amount: value.source_amount.into(),
            secondary_source: Source::get_name_by_id(value.secondary_source_id, app_state.clone())
                .await?,
            secondary_source_amount: value.secondary_source_amount.map(Numeric::into),
            conversion_rate: value.conversion_rate.into(),
            conversion_rate_to_fixed: value.conversion_rate_to_fixed.into(),
            archived: value.archived,
        })
    }
}

fn convert_currency<T: Into<Decimal>>(amount: T, from: &Currency, to: &Currency) -> Decimal {
    from.rate_to_fixed / to.rate_to_fixed * amount.into()
}

pub trait FindByFilter {
    async fn find_by_filter(
        query_params: &EntryQuery,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Vec<Entry>, StatefulTryFromError>;
}

impl FindByFilter for Entry {
    async fn find_by_filter(
        query_params: &EntryQuery,
        user: &User,
        app_state: Arc<AppState>,
    ) -> Result<Vec<Entry>, StatefulTryFromError> {
        use model::schema::entries::dsl::*;
        // Boxing the query allows us to mutate it without changing its type.
        let mut query = entries.into_boxed();

        let amount_specified =
            &query_params.amount.or(query_params.min_amount).or(query_params.max_amount);

        if amount_specified.is_some() && query_params.currency.is_none() {
            lbr!(
                "If you specify amount(s), you should always also specify currency. If you want \
                 to do currency-agnostic comparison, use *_amount_in_fixed instead."
            );
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
            let ids = join_all(names.into_iter().map(async |name| {
                Source::get_id_by_name_and_user(name, &user, app_state.clone()).await
            }))
            .await
            .into_iter()
            .filter_map(|id_result| id_result.ok())
            .collect::<Vec<i32>>();

            query = query.filter(source_id.eq_any(ids));
        }
        if let Some(names) = currencies {
            let ids: Vec<_> = join_all(names.iter().map(async |name| {
                Currency::get_id_by_name_and_user(name.as_str(), &user, app_state.clone()).await
            }))
            .await
            .into_iter()
            .filter_map(|id_result| id_result.ok())
            .collect::<Vec<i32>>();

            query = query.filter(currency_id.eq_any(ids));
        }
        if let Some(names) = &query_params.categories {
            let ids: Vec<_> = join_all(names.iter().map(async |name| {
                Category::get_id_by_name_and_user(name.as_str(), &user, app_state.clone()).await
            }))
            .await
            .into_iter()
            .filter_map(|id_result| id_result.ok())
            .collect::<Vec<i32>>();

            query = query.filter(category_id.eq_any(ids));
        }

        if let Some(q_amount) = query_params.amount {
            query = query.filter(amount.eq(Numeric::from(q_amount)));
        }

        if let Some(min_amount) = query_params.min_amount {
            query = query.filter(amount.ge(Numeric::from(min_amount)));
        }

        if let Some(max_amount) = query_params.max_amount {
            query = query.filter(amount.le(Numeric::from(max_amount)));
        }

        if let Some(min_amount_in_fixed) = query_params.min_amount_in_fixed {
            query = query.filter(amount_in_fixed.ge(Numeric::from(min_amount_in_fixed)));
        }

        if let Some(max_amount_in_fixed) = query_params.max_amount_in_fixed {
            query = query.filter(amount_in_fixed.le(Numeric::from(max_amount_in_fixed)));
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
                "create_asc" => query = query.order((created_at.asc(), id.asc())),
                "create_desc" => query = query.order((created_at.asc(), id.asc())),
                "amount_asc" => query = query.order((amount.asc(), created_at.asc(), id.asc())),
                "amount_desc" => query = query.order((amount.desc(), created_at.asc(), id.asc())),
                "date_asc" => query = query.order((date.asc(), created_at.asc(), id.asc())),
                "date_desc" => query = query.order((date.desc(), created_at.desc(), id.desc())),
                _ => query = query.order((date.asc(), created_at.asc(), id.asc())),
            }
        }

        // dbg!(diesel::debug_query(&query));
        let r_entries = query.load::<Entry>(&mut app_state.cpool().await).await?;

        Ok(r_entries)
    }
}
