-- TODO: Add indices

create type entry_t as enum ('spend', 'income', 'lend', 'borrow', 'convert');

create table users (
    id serial primary key,
    username varchar(1023) unique not null,
    password varchar(1023) not null,
    fixed_currency_id int4,
    enabled boolean not null default false
);

create table currencies (
    id serial primary key,
    user_id int4 not null references users(id) on delete cascade,
    name varchar(1023) not null,
    rate_to_fixed float8 not null,
    constraint user_currency unique (user_id, name),
    archived boolean not null default false
);

alter table users add constraint fixed_currency_id_fk
    foreign key (fixed_currency_id) references currencies(id) on delete set null;

create table categories (
    id serial primary key,
    user_id int4 not null references users(id) on delete cascade,
    name varchar(1023) not null,
    constraint user_category unique (user_id, name),
    archived boolean not null default false
);

create table sources (
    id serial primary key,
    user_id int4 not null references users(id) on delete cascade,
    name varchar(1023) not null,
    currency_id int4 not null references currencies(id) on delete restrict,
    amount float8 not null,
    constraint user_source unique (user_id, name),
    archived boolean not null default false
);

create table entries (
    id serial primary key,
    user_id int4 not null references users(id) on delete cascade,
    description text not null,
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
