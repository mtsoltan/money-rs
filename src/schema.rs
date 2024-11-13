// @generated automatically by Diesel CLI.

pub mod sql_types {
    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "entry_t"))]
    pub struct EntryT;
}

diesel::table! {
    categories (id) {
        id -> Int4,
        user_id -> Int4,
        #[max_length = 127]
        name -> Varchar,
        archived -> Bool,
    }
}

diesel::table! {
    currencies (id) {
        id -> Int4,
        user_id -> Int4,
        #[max_length = 63]
        name -> Varchar,
        rate_to_fixed -> Float8,
        archived -> Bool,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::EntryT;

    entries (id) {
        id -> Int4,
        user_id -> Int4,
        description -> Text,
        #[max_length = 127]
        target -> Nullable<Varchar>,
        category_id -> Int4,
        amount -> Float8,
        date -> Timestamp,
        created_at -> Timestamp,
        currency_id -> Int4,
        entry_type -> EntryT,
        source_id -> Int4,
        secondary_source_id -> Nullable<Int4>,
        conversion_rate -> Nullable<Float8>,
        conversion_rate_to_fixed -> Float8,
        archived -> Bool,
    }
}

diesel::table! {
    sources (id) {
        id -> Int4,
        user_id -> Int4,
        #[max_length = 127]
        name -> Varchar,
        currency_id -> Int4,
        amount -> Float8,
        archived -> Bool,
    }
}

diesel::table! {
    users (id) {
        id -> Int4,
        #[max_length = 63]
        username -> Varchar,
        #[max_length = 1023]
        password -> Varchar,
        fixed_currency_id -> Nullable<Int4>,
        enabled -> Bool,
    }
}

diesel::joinable!(categories -> users (user_id));
diesel::joinable!(entries -> categories (category_id));
diesel::joinable!(entries -> currencies (currency_id));
diesel::joinable!(entries -> users (user_id));
diesel::joinable!(sources -> currencies (currency_id));
diesel::joinable!(sources -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(categories, currencies, entries, sources, users,);
