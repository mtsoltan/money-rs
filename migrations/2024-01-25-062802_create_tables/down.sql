drop table entries;
drop table sources;
drop table categories;
alter table users drop constraint fixed_currency_id_fk;
drop table currencies;
drop table users;
drop type entry_t;