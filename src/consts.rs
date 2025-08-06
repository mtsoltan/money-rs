use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::deadpool::Object;
use fpdec::{Decimal, Dec};

pub(crate) const EPSILON: Decimal = Dec!(0.00001);

pub type Pool = diesel_async::pooled_connection::deadpool::Pool<AsyncPgConnection>;
pub type Conn = Object<AsyncPgConnection>;

pub const MAX_N_FRAC_DIGITS: u8 = 18;