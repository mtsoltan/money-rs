create type entry_t as enum ('spend', 'income', 'lend', 'borrow', 'convert');

create table users (
    id serial primary key,
    username varchar(63) unique not null,
    password varchar(1023) not null,
    fixed_currency_id int4,
    enabled boolean not null default false
);
create index users_enabled_idx on users(enabled);

create table currencies (
    id serial primary key,
    user_id int4 not null references users(id) on delete cascade,
    name varchar(63) not null,
    rate_to_fixed float8 not null,
    constraint unq_user_currency unique (user_id, name),
    archived boolean not null default false
);
create index currencies_name_idx on currencies(name);
create index currencies_archived_idx on currencies(archived);

alter table users add constraint fixed_currency_id_fkey
    foreign key (fixed_currency_id) references currencies(id) on delete set null;

create table categories (
    id serial primary key,
    user_id int4 not null references users(id) on delete cascade,
    name varchar(127) not null,
    constraint unq_user_category unique (user_id, name),
    archived boolean not null default false
);
create index categories_name_idx on categories(name);
create index categories_archived_idx on categories(archived);

create table sources (
    id serial primary key,
    user_id int4 not null references users(id) on delete cascade,
    name varchar(127) not null,
    currency_id int4 not null references currencies(id) on delete restrict,
    amount float8 not null default 0,
    constraint unq_user_source unique (user_id, name),
    archived boolean not null default false
);
create index sources_name_idx on sources(name);
create index sources_amount_idx on sources(amount);
create index sources_archived_idx on sources(archived);

create table entries (
    id serial primary key,
    user_id int4 not null references users(id) on delete cascade,
    description text not null,
    target varchar(127),
    category_id int4 not null references categories(id) on delete restrict,
    amount float8 not null,
    date timestamp not null,
    created_at timestamp not null default now(),
    currency_id int4 not null references currencies(id) on delete restrict,
    entry_type entry_t not null,
    source_id int4 not null references sources(id) on delete restrict,
    secondary_source_id int4 null references sources(id) on delete restrict,
    conversion_rate float8 null,
    conversion_rate_to_fixed float8 not null,
    archived boolean not null default false
);
create index entries_target_idx on entries(target) where target is not null;
create index entries_amount_idx on entries(amount);
create index entries_date_idx on entries(date);
create index entries_created_at_idx on entries(created_at);
create index entries_entry_type_idx on entries(entry_type);
create index entries_archived_idx on entries(archived);
