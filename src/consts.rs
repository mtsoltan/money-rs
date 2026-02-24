use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::deadpool::Object;
use fpdec::{Dec, Decimal};

// TODO(40): Move to model to use in front-end
pub(crate) const EPSILON: Decimal = Dec!(0.00001);

pub type Pool = diesel_async::pooled_connection::deadpool::Pool<AsyncPgConnection>;
pub type Conn = Object<AsyncPgConnection>;
