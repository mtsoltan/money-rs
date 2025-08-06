use std::fmt;
use std::ops::{Add, Div, Mul, Sub};
use std::str::FromStr;

use diesel::data_types::PgNumeric;
use diesel::deserialize::FromSql;
use diesel::pg::{Pg, PgValue};
use diesel::serialize::ToSql;
use diesel::{AsExpression, FromSqlRow, sql_types};
use fpdec::{Decimal, ParseDecimalError};
use serde::de::{Error, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::consts;

#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow)]
#[diesel(sql_type = sql_types::Numeric)]
pub struct Numeric(Decimal);

impl Numeric {
    /// Equivalent to Into<Decimal>
    pub fn dec(&self) -> Decimal { self.0 }
}

impl From<PgNumeric> for Numeric {
    fn from(value: PgNumeric) -> Self { Numeric(pg_to_fpdec(&value).unwrap()) }
}

impl From<Decimal> for Numeric {
    fn from(value: Decimal) -> Self { Numeric(value) }
}

impl Into<Decimal> for Numeric {
    fn into(self) -> Decimal { self.0 }
}

impl Into<Decimal> for &Numeric {
    fn into(self) -> Decimal { self.0 }
}

impl FromSql<sql_types::Numeric, Pg> for Numeric {
    fn from_sql(bytes: PgValue<'_>) -> diesel::deserialize::Result<Self> {
        PgNumeric::from_sql(bytes).map(|i| i.into())
    }
}

fn fpdec_to_pg(decimal: &Decimal) -> Result<PgNumeric, ParseDecimalError> {
    let (mut integer, scale) = (decimal.coefficient(), decimal.n_frac_digits());
    if integer == 0 {
        return Ok(PgNumeric::Positive { digits: vec![0], scale: 0, weight: 0 });
    }
    let sign = integer > 0;
    integer = integer.abs();

    // Ensure that the decimal will always lie on a digit boundary
    // This may multiply by 1000, which means 1701 4118 3460 4692 3173 1687 3037 1588 4105 is the
    // largest number we can represent without headache.

    for _ in 0..((4 - scale % 4) % 4) {
        integer = match integer.checked_mul(10) {
            Some(r) => r,
            None => return Err(ParseDecimalError::InternalOverflow),
        };
    }

    let mut digits = vec![];
    while integer > 0 {
        digits.push((integer % 10000) as i16);
        integer /= 10000;
    }

    digits.reverse();
    let digits_after_decimal = if scale == 0 { 0 } else { (scale - 1) / 4 + 1 };
    // Original implementation used to subtract an extra one,
    // which means having a digit left of the dot would yield 0 and not would yield -1.
    let weight = i16::try_from(digits.len())
        .expect("Max digit number is expected to fit into 16 bit")
        - i16::try_from(digits_after_decimal)
            .expect("Max digit number is expected to fit into 16 bit")
        - 1;

    let unnecessary_zeroes = digits.iter().rev().take_while(|i| **i == 0).count();

    let relevant_digits = digits.len() - unnecessary_zeroes;
    digits.truncate(relevant_digits);

    Ok(match sign {
        true => PgNumeric::Positive { digits, scale: scale as u16, weight },
        false => PgNumeric::Negative { digits, scale: scale as u16, weight },
    })
}

impl ToSql<sql_types::Numeric, Pg> for Numeric {
    fn to_sql<'b>(
        &self,
        out: &mut diesel::serialize::Output<'b, '_, Pg>,
    ) -> diesel::serialize::Result {
        use byteorder::{NetworkEndian, WriteBytesExt};
        use diesel::serialize::IsNull;
        let pg_numeric = fpdec_to_pg(&self.0).unwrap();

        let sign = match pg_numeric {
            PgNumeric::Positive { .. } => 0,
            PgNumeric::Negative { .. } => 0x4000,
            PgNumeric::NaN => 0xC000,
        };
        let empty_vec = Vec::new();
        let digits = match pg_numeric {
            PgNumeric::Positive { ref digits, .. } | PgNumeric::Negative { ref digits, .. } => {
                digits
            }
            PgNumeric::NaN => &empty_vec,
        };
        let weight = match pg_numeric {
            PgNumeric::Positive { weight, .. } | PgNumeric::Negative { weight, .. } => weight,
            PgNumeric::NaN => 0,
        };
        let scale = match pg_numeric {
            PgNumeric::Positive { scale, .. } | PgNumeric::Negative { scale, .. } => scale,
            PgNumeric::NaN => 0,
        };
        out.write_u16::<NetworkEndian>(digits.len().try_into()?)?;
        out.write_i16::<NetworkEndian>(weight)?;
        out.write_u16::<NetworkEndian>(sign)?;
        out.write_u16::<NetworkEndian>(scale)?;
        for digit in digits.iter() {
            out.write_i16::<NetworkEndian>(*digit)?;
        }

        Ok(IsNull::No)
    }
}

impl Serialize for Numeric {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let s = self.0.to_string();
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
                match Decimal::from_str(v) {
                    Ok(e) => Ok(Numeric(e)),
                    Err(_) => Err(Error::custom(format!("Invalid decimal string"))),
                }
            }
        }
        deserializer.deserialize_str(V)
    }
}

impl fmt::Display for Numeric {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}", self.0.to_string()) }
}

impl Default for Numeric {
    fn default() -> Self { Self(Decimal::ZERO) }
}

macro_rules! impl_operation_numeric {
    ($trait:ident, $fn_name:ident, $oper:tt, $lhs:ty, $rhs:ty) => {
        impl $trait<$rhs> for $lhs {
            type Output = Decimal;
            fn $fn_name(self, rhs: $rhs) -> Decimal { self.0 $oper rhs.0 }
        }
    }
}

macro_rules! impl_operation_bidirectional_dec {
    ($trait:ident, $fn_name:ident, $oper:tt, $num:ty, $dec:ty) => {
        impl $trait<$num> for $dec {
            type Output = Decimal;
            fn $fn_name(self, rhs: $num) -> Decimal { self $oper rhs.0 }
        }
        impl $trait<$dec> for $num {
            type Output = Decimal;
            fn $fn_name(self, rhs: $dec) -> Decimal { self.0 $oper rhs }
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
impl_all_operations_bidirectional_dec!(Numeric, Decimal);

impl<'a> std::iter::Sum<Numeric> for Decimal {
    fn sum<I: Iterator<Item = Numeric>>(iter: I) -> Self {
        iter.map(|e| e.0).fold(Decimal::ZERO, |a, b| a + b)
    }
}

impl<'a> std::iter::Sum<&'a Numeric> for Decimal {
    fn sum<I: Iterator<Item = &'a Numeric>>(iter: I) -> Self {
        iter.map(|e| e.0).fold(Decimal::ZERO, |a, b| a + b)
    }
}

pub fn pg_to_fpdec(pgn: &PgNumeric) -> Result<Decimal, ParseDecimalError> {
    let (sign, _, weight, digits) = match pgn {
        PgNumeric::NaN => return Err(ParseDecimalError::Invalid),
        PgNumeric::Positive { scale, digits, weight } => (1, *scale, *weight, digits),
        PgNumeric::Negative { scale, digits, weight } => (-1, *scale, *weight, digits),
    };
    // Scale from DB will always be 20 so we can't use it to figure out what's happening to
    // decimals, we rely on weight. Biggest digit: [1701, 4118, 3460, 4692, 3173, 1687, 3037,
    // 1588, 4105] Of course, we can store bigger stuff in our decimal by losing precision, but
    // we just error. Values that are bigger should never make it to our database anyway.
    if digits.len() == 0 {
        return Ok(Decimal::ZERO);
    }
    let mut i = digits[0].clone();
    let mut shifter = 1;
    while i > 0 {
        shifter *= 10;
        i /= 10;
    }
    if digits.len() > 9 && digits[digits.len() - 1] % shifter != 0 {
        return Err(ParseDecimalError::InternalOverflow);
    }

    let mut result = 0i128;
    for digit in digits {
        result *= 10_000i128;
        result += i128::from(*digit);
    }
    let mut quads_after_decimal_dot = digits.len() as i16 - weight - 1;
    while quads_after_decimal_dot < 0 {
        result *= 10_000i128;
        quads_after_decimal_dot += 1;
    }
    let mut digits_after_decimal_dot =
        if quads_after_decimal_dot < 0 { 0 } else { quads_after_decimal_dot * 4 };
    if quads_after_decimal_dot > 0 {
        while result % 10 == 0 {
            result /= 10;
            digits_after_decimal_dot -= 1;
        }
    }

    if digits_after_decimal_dot as u8 > consts::MAX_N_FRAC_DIGITS {
        return Err(ParseDecimalError::FracDigitLimitExceeded);
    }

    let result = Decimal::new_raw(sign * result, digits_after_decimal_dot as u8);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use diesel::data_types::PgNumeric;
    use fpdec::Dec;

    use super::*;

    #[test]
    fn integer_simple() {
        let pg = PgNumeric::Positive { weight: 0, scale: 0, digits: vec![42] };
        let fpdec = Dec!(42);
        assert_eq!(pg_to_fpdec(&pg).unwrap(), fpdec);
        assert_eq!(fpdec_to_pg(&fpdec).unwrap(), pg);
    }

    #[test]
    fn integer_multi_group() {
        let pg = PgNumeric::Positive { weight: 0, scale: 2, digits: vec![123, 4500] };
        let fpdec = Dec!(123.45);
        assert_eq!(pg_to_fpdec(&pg).unwrap(), fpdec);
        assert_eq!(fpdec_to_pg(&fpdec).unwrap(), pg);
    }

    #[test]
    fn fractional_exact_group() {
        let pg = PgNumeric::Positive { weight: -1, scale: 4, digits: vec![1] };
        let fpdec = Dec!(0.0001);
        assert_eq!(pg_to_fpdec(&pg).unwrap(), Dec!(0.0001));
        assert_eq!(fpdec_to_pg(&fpdec).unwrap(), pg);
    }

    #[test]
    fn negative_fraction() {
        let pg = PgNumeric::Negative { weight: -1, scale: 4, digits: vec![1] };
        let fpdec = Dec!(-0.0001);
        assert_eq!(pg_to_fpdec(&pg).unwrap(), Dec!(-0.0001));
        assert_eq!(fpdec_to_pg(&fpdec).unwrap(), pg);
    }

    #[test]
    fn on_edge() {
        let pg = PgNumeric::Negative {
            weight: 7,
            scale: 6,
            digits: vec![17, 141, 1834, 6046, 9231, 7316, 8730, 3715, 8841, 500],
        };
        let fpdec = Dec!(-170141183460469231731687303715.884105);
        assert_eq!(pg_to_fpdec(&pg).unwrap(), fpdec);
        assert_eq!(fpdec_to_pg(&fpdec).unwrap(), pg);
    }

    #[test]
    fn on_edge_single_way() {
        let pg = PgNumeric::Negative {
            weight: 7,
            scale: 7,
            digits: vec![1, 7014, 1183, 4604, 6923, 1731, 6873, 371, 5884, 1050],
        };
        let fpdec = Dec!(-17014118346046923173168730371.5884105);
        assert_eq!(pg_to_fpdec(&pg).unwrap(), fpdec);
        assert_eq!(fpdec_to_pg(&fpdec).unwrap(), pg);
    }

    // fails - does not overflow
    #[test]
    fn overflow_single_way() {
        let pg = PgNumeric::Negative {
            weight: 7,
            scale: 7,
            digits: vec![1, 7014, 1183, 4604, 6923, 1731, 6873, 371, 5884, 1051],
        };
        assert_eq!(pg_to_fpdec(&pg).unwrap_err(), ParseDecimalError::InternalOverflow);
    }

    #[test]
    fn overflow_big_number() {
        let pg = PgNumeric::Negative {
            weight: -1,
            scale: 7,
            digits: vec![170, 1411, 8346, 0469, 2317, 3168, 7303, 7158, 8410, 5728],
        };
        assert_eq!(pg_to_fpdec(&pg).unwrap_err(), ParseDecimalError::InternalOverflow);
    }

    #[test]
    fn fract_limit_exceeded() {
        let pg = PgNumeric::Negative {
            weight: -1,
            scale: 20,
            digits: vec![1701, 4118, 3460, 4692, 3173, 1687, 3037, 1588, 4105],
        };
        assert_eq!(pg_to_fpdec(&pg).unwrap_err(), ParseDecimalError::FracDigitLimitExceeded);
    }

    #[test]
    fn numbers_with_negative_quads_succeed() {
        let pg = PgNumeric::Positive { weight: 1, scale: 0, digits: vec![5] };
        let fpdec = Dec!(50000);
        assert_eq!(pg_to_fpdec(&pg).unwrap(), fpdec);
        assert_eq!(fpdec_to_pg(&fpdec).unwrap(), pg);
    }

    #[test]
    fn largest_number_nofrac() {
        let pg = PgNumeric::Negative {
            weight: 8,
            scale: 0,
            digits: vec![1701, 4118, 3460, 4692, 3173, 1687, 3037, 1588, 4105],
        };
        let fpdec = Dec!(-170141183460469231731687303715884105);
        assert_eq!(pg_to_fpdec(&pg).unwrap(), fpdec);
        assert_eq!(fpdec_to_pg(&fpdec).unwrap(), pg);
    }

    #[test]
    fn overflow_largest_number_nofrac() {
        let pg = PgNumeric::Negative {
            weight: 8,
            scale: 0,
            digits: vec![1701, 4118, 3460, 4692, 3173, 1687, 3037, 1588, 4106],
        };
        let fpdec = Dec!(-170141183460469231731687303715884106);
        assert_eq!(pg_to_fpdec(&pg).unwrap(), fpdec);
        assert_eq!(fpdec_to_pg(&fpdec).unwrap(), pg);
    }
}
