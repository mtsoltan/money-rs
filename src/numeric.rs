use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

use diesel::data_types::PgNumeric;
use diesel::deserialize::FromSql;
use diesel::pg::{Pg, PgValue};
use diesel::serialize::ToSql;
use diesel::{AsExpression, FromSqlRow, sql_types};
use fpdec::Decimal;
use fpdec::ParseDecimalError;
use serde::de::{Error, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, AsExpression, FromSqlRow)]
#[diesel(sql_type = sql_types::Numeric)]
pub struct Numeric {
    pg: PgNumeric,
    fpdec: Decimal,
}

impl Numeric {
    fn to_string(&self) -> String {
        match &self.pg {
            PgNumeric::NaN => "NaN".to_string(),
            PgNumeric::Positive { weight, scale, digits } => {
                decimal_to_string(true, *weight, *scale, digits)
            }
            PgNumeric::Negative { weight, scale, digits } => {
                decimal_to_string(false, *weight, *scale, digits)
            }
        }
    }

    /// Equivalent to Into<Decimal>
    pub fn dec(&self) -> Decimal {
        self.fpdec
    }
}

impl From<PgNumeric> for Numeric {
    fn from(value: PgNumeric) -> Self {
        Numeric { fpdec: pg_to_fpdec(&value).unwrap_or_default(), pg: value }
    }
}

impl From<Decimal> for Numeric {
    fn from(value: Decimal) -> Self { Numeric { pg: fpdec_to_pg(&value), fpdec: value } }
}

impl Into<PgNumeric> for Numeric {
    fn into(self) -> PgNumeric { self.pg }
}

impl Into<Decimal> for Numeric {
    fn into(self) -> Decimal { self.fpdec }
}

impl Into<PgNumeric> for &Numeric {
    fn into(self) -> PgNumeric { self.pg.clone() }
}

impl Into<Decimal> for &Numeric {
    fn into(self) -> Decimal { self.fpdec.clone() }
}

impl FromSql<sql_types::Numeric, Pg> for Numeric {
    fn from_sql(bytes: PgValue<'_>) -> diesel::deserialize::Result<Self> {
        PgNumeric::from_sql(bytes).map(|i| i.into())
    }
}

impl ToSql<sql_types::Numeric, Pg> for Numeric {
    fn to_sql<'b>(
        &'b self,
        out: &mut diesel::serialize::Output<'b, '_, Pg>,
    ) -> diesel::serialize::Result {
        <PgNumeric as diesel::serialize::ToSql<sql_types::Numeric, Pg>>::to_sql(&self.pg, out)
    }
}

impl Serialize for Numeric {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let s = self.to_string();
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for Numeric {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Numeric;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a decimal string")
            }
            fn visit_str<E: Error>(self, v: &str) -> Result<Self::Value, E> {
                let (neg, s) = if let Some(rest) = v.strip_prefix('-') {
                    (true, rest)
                } else if let Some(rest) = v.strip_prefix('+') {
                    (false, rest)
                } else {
                    (false, v)
                };
                let parts: Vec<_> = s.split('.').collect();
                if parts.len() > 2 {
                    return Err(E::custom("invalid numeric"));
                }
                let ip = parts[0];
                let fp = parts.get(1).cloned().unwrap_or("");
                let scale = fp.len() as u16;
                let mut all = format!("{}{}", ip, fp);
                let groups = ((all.len() + 3) / 4) as usize;
                let target = groups * 4;
                all.push_str(&"0".repeat(target - all.len()));
                let digits = all
                    .as_bytes()
                    .chunks(4)
                    .map(|chunk| {
                        std::str::from_utf8(chunk)
                            .map_err(|_| E::custom("bad utf8"))?
                            .parse::<i16>()
                            .map_err(|_| E::custom("bad digits"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let weight = (ip.len() as i16 + 3) / 4 - 1;
                Ok(match neg {
                    false => PgNumeric::Positive { weight, scale, digits }.into(),
                    true => PgNumeric::Negative { weight, scale, digits }.into(),
                })
            }
        }
        deserializer.deserialize_str(V)
    }
}

fn decimal_to_string(sign: bool, weight: i16, scale: u16, digits: &[i16]) -> String {
    // join each base-10000 “digit” as a zero-padded 4-digit block
    let mut s = digits.iter().map(|d| format!("{:04}", d)).collect::<Vec<_>>().join("");
    let int_groups = (weight + 1) as usize;
    let int_len = int_groups * 4;
    // ensure we have enough chars for frac
    if s.len() < int_len + scale as usize {
        s.push_str(&"0".repeat(int_len + scale as usize - s.len()));
    }
    let int_str = &s[..int_len];
    let frac_str = &s[int_len..int_len + scale as usize];
    let mut out = if sign { String::new() } else { "-".to_string() };
    let int_str = int_str.trim_start_matches('0');
    out.push_str(if int_str.is_empty() { "0" } else { int_str });
    if scale > 0 {
        out.push('.');
        out.push_str(frac_str);
    }
    out
}

impl fmt::Display for Numeric {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}", self.to_string()) }
}

pub fn pg_to_fpdec(pgn: &PgNumeric) -> Result<Decimal, ParseDecimalError> {
    match pgn {
        PgNumeric::NaN => Err(ParseDecimalError::Invalid),
        PgNumeric::Positive { weight, scale, digits }
        | PgNumeric::Negative { weight, scale, digits } => {
            let sign = matches!(pgn, PgNumeric::Negative { .. });
            let scale_u8 = match u8::try_from(*scale) {
                Ok(e) => e,
                Err(_) => return Err(ParseDecimalError::FracDigitLimitExceeded),
            };
            // coeff = sum digits[i] * 10^(4*(weight-i) + scale)
            let mut coeff: i128 = 0;
            for (i, d) in digits.into_iter().enumerate() {
                let exp = 4 * (weight - i as i16) + *scale as i16;
                coeff = coeff
                    .checked_add(
                        (*d as i128)
                            .checked_mul(10i128.pow(exp as u32))
                            .ok_or_else(|| ParseDecimalError::InternalOverflow)?,
                    )
                    .ok_or_else(|| ParseDecimalError::InternalOverflow)?;
            }
            if sign {
                coeff = -coeff;
            }
            Ok(Decimal::new_raw(coeff, scale_u8))
        }
    }
}

pub fn pg_numeric_zero() -> PgNumeric {
    PgNumeric::Positive { weight: 0, scale: 0, digits: vec![0] }
}

impl Default for Numeric {
    fn default() -> Self { Self { pg: pg_numeric_zero(), fpdec: Decimal::ZERO } }
}

pub fn fpdec_to_pg(d: &Decimal) -> PgNumeric {
    if *d == Decimal::ZERO {
        return pg_numeric_zero();
    }
    let sign = d.coefficient() < 0;
    let mut rem = d.coefficient().abs();
    let scale = d.n_frac_digits();
    let scale_u16 = scale as u16;
    // how many integer decimal digits?
    let mag = d.magnitude(); // floor(log10(value))
    let int_digits = if mag >= 0 { (mag + 1) as u32 } else { 1 };
    let int_groups = ((int_digits + 3) / 4) as usize;
    let frac_groups = ((scale as u32 + 3) / 4) as usize;
    let total = int_groups + frac_groups;
    let mut groups = Vec::with_capacity(total);
    for i in 0..total {
        // each group spans 4 decimal digits
        let exp = 4 * (total - 1 - i) as u32;
        let base = 10i128.pow(exp);
        let g = (rem / base) as i16;
        groups.push(g);
        rem %= base;
    }
    let weight = (int_groups as i16) - 1;
    let pgn = if sign {
        PgNumeric::Negative { weight, scale: scale_u16, digits: groups }
    } else {
        PgNumeric::Positive { weight, scale: scale_u16, digits: groups }
    };
    pgn
}

macro_rules! impl_operation_numeric {
    ($trait:ident, $fn_name:ident, $oper:tt, $lhs:ty, $rhs:ty) => {
        impl $trait<$rhs> for $lhs {
            type Output = Decimal;
            fn $fn_name(self, rhs: $rhs) -> Decimal { self.fpdec $oper rhs.fpdec }
        }
    }
}

macro_rules! impl_operation_bidirectional_dec {
    ($trait:ident, $fn_name:ident, $oper:tt, $num:ty, $dec:ty) => {
        impl $trait<$num> for $dec {
            type Output = Decimal;
            fn $fn_name(self, rhs: $num) -> Decimal { self $oper rhs.fpdec }
        }
        impl $trait<$dec> for $num {
            type Output = Decimal;
            fn $fn_name(self, rhs: $dec) -> Decimal { self.fpdec $oper rhs }
        }
    }
}

macro_rules! impl_all_operations_numeric {
    ($lhs:ty, $rhs:ty) => {
        impl_operation_numeric!(Add, add, +, $lhs, $rhs);
        impl_operation_numeric!(Sub, sub, -, $lhs, $rhs);
        impl_operation_numeric!(Mul, mul, *, $lhs, $rhs);
        impl_operation_numeric!(Div, div, /, $lhs, $rhs);
    }

}

macro_rules! impl_all_operations_bidirectional_dec {
    ($num:ty, $dec:ty) => {
        impl_operation_bidirectional_dec!(Add, add, +, $num, $dec);
        impl_operation_bidirectional_dec!(Sub, sub, -, $num, $dec);
        impl_operation_bidirectional_dec!(Mul, mul, *, $num, $dec);
        impl_operation_bidirectional_dec!(Div, div, /, $num, $dec);
    }
}

impl_all_operations_numeric!(Numeric, Numeric);
impl_all_operations_numeric!(Numeric, &Numeric);
impl_all_operations_numeric!(&Numeric, Numeric);
impl_all_operations_numeric!(&Numeric, &Numeric);
impl_all_operations_bidirectional_dec!(Numeric, Decimal);
impl_all_operations_bidirectional_dec!(&Numeric, Decimal);
impl_all_operations_bidirectional_dec!(Numeric, &Decimal);
impl_all_operations_bidirectional_dec!(&Numeric, &Decimal);

impl<'a> std::iter::Sum<Numeric> for Decimal {
    fn sum<I: Iterator<Item = Numeric>>(iter: I) -> Self {
        iter.map(|e| e.fpdec).fold(Decimal::ZERO, |a, b| a + b)
    }
}

impl<'a> std::iter::Sum<&'a Numeric> for Decimal {
    fn sum<I: Iterator<Item = &'a Numeric>>(iter: I) -> Self {
        iter.map(|e| e.fpdec).fold(Decimal::ZERO, |a, b| a + b)
    }
}
