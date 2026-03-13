use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GncNumeric {
    pub num: i64,
    pub denom: i64,
}

impl GncNumeric {
    pub fn new(num: i64, denom: i64) -> Self {
        GncNumeric { num, denom }
    }

    pub fn zero() -> Self {
        GncNumeric { num: 0, denom: 1 }
    }

    pub fn to_f64(&self) -> f64 {
        if self.denom == 0 {
            0.0
        } else {
            self.num as f64 / self.denom as f64
        }
    }

    pub fn check(&self) -> GncNumericErrorCode {
        if self.denom == 0 {
            match self.num {
                -1 => GncNumericErrorCode::Arg,
                -2 => GncNumericErrorCode::Overflow,
                -3 => GncNumericErrorCode::DenomDiff,
                -4 => GncNumericErrorCode::Remainder,
                _ => GncNumericErrorCode::Arg,
            }
        } else {
            GncNumericErrorCode::Ok
        }
    }

    pub fn error(code: GncNumericErrorCode) -> Self {
        GncNumeric {
            num: code as i64,
            denom: 0,
        }
    }

    /// Internal helper for GCD
    fn gcd(mut a: i128, mut b: i128) -> i128 {
        a = a.abs();
        b = b.abs();
        while b != 0 {
            a %= b;
            std::mem::swap(&mut a, &mut b);
        }
        a
    }

    /// Internal helper for LCM
    fn lcm(a: i128, b: i128) -> i128 {
        if a == 0 || b == 0 {
            return 0;
        }
        let g = Self::gcd(a, b);
        (a / g * b).abs()
    }

    pub fn reduce(mut self) -> Self {
        let check = self.check();
        if check != GncNumericErrorCode::Ok {
            return self;
        }
        if self.num == 0 {
            self.denom = 1;
            return self;
        }
        let common = Self::gcd(self.num as i128, self.denom as i128) as i64;
        self.num /= common;
        self.denom /= common;
        if self.denom < 0 {
            self.num = -self.num;
            self.denom = -self.denom;
        }
        self
    }

    /// Core rounding logic: divides num by denom using the specified mode.
    fn div_round(
        num: i128,
        denom: i128,
        how: GncNumericRounding,
    ) -> Result<i64, GncNumericErrorCode> {
        if denom == 0 {
            return Err(GncNumericErrorCode::Arg);
        }

        let quotient = num / denom;
        let remainder = num % denom;

        if remainder == 0 {
            return quotient
                .try_into()
                .map_err(|_| GncNumericErrorCode::Overflow);
        }

        if how == GncNumericRounding::Never {
            return Err(GncNumericErrorCode::Remainder);
        }

        let result = match how {
            GncNumericRounding::Floor => {
                if (num < 0) ^ (denom < 0) {
                    quotient - 1
                } else {
                    quotient
                }
            }
            GncNumericRounding::Ceil => {
                if (num < 0) ^ (denom < 0) {
                    quotient
                } else {
                    quotient + 1
                }
            }
            GncNumericRounding::Trunc => quotient,
            GncNumericRounding::Promote => {
                if (num < 0) ^ (denom < 0) {
                    quotient - 1
                } else {
                    quotient + 1
                }
            }
            GncNumericRounding::RoundHalfDown
            | GncNumericRounding::RoundHalfUp
            | GncNumericRounding::Round => {
                let abs_rem = remainder.abs();
                let abs_denom = denom.abs();

                if abs_rem * 2 < abs_denom {
                    quotient
                } else if abs_rem * 2 > abs_denom {
                    if (num < 0) ^ (denom < 0) {
                        quotient - 1
                    } else {
                        quotient + 1
                    }
                } else {
                    // It's exactly half
                    match how {
                        GncNumericRounding::RoundHalfDown => quotient,
                        GncNumericRounding::RoundHalfUp => {
                            if (num < 0) ^ (denom < 0) {
                                quotient - 1
                            } else {
                                quotient + 1
                            }
                        }
                        GncNumericRounding::Round => {
                            // Banker's Rounding: to nearest even
                            if quotient % 2 == 0 {
                                quotient
                            } else {
                                if (num < 0) ^ (denom < 0) {
                                    quotient - 1
                                } else {
                                    quotient + 1
                                }
                            }
                        }
                        _ => unreachable!(),
                    }
                }
            }
            GncNumericRounding::Never => unreachable!(),
        };

        result.try_into().map_err(|_| GncNumericErrorCode::Overflow)
    }

    pub fn add(
        a: Self,
        b: Self,
        denom: i64,
        how_rnd: GncNumericRounding,
        how_denom: GncNumericDenom,
    ) -> Self {
        if a.check() != GncNumericErrorCode::Ok {
            return a;
        }
        if b.check() != GncNumericErrorCode::Ok {
            return b;
        }

        let exact_denom = Self::lcm(a.denom as i128, b.denom as i128);
        let num_a = a.num as i128 * (exact_denom / a.denom as i128);
        let num_b = b.num as i128 * (exact_denom / b.denom as i128);
        let exact_num = num_a + num_b;

        let target_denom = if denom == GNC_DENOM_AUTO {
            match how_denom {
                GncNumericDenom::Reduce => {
                    let common = Self::gcd(exact_num, exact_denom);
                    (exact_denom / common) as i64
                }
                GncNumericDenom::Lcd => exact_denom as i64,
                GncNumericDenom::Fixed => {
                    if a.denom != b.denom {
                        return Self::error(GncNumericErrorCode::DenomDiff);
                    }
                    a.denom
                }
                _ => exact_denom as i64,
            }
        } else {
            denom
        };

        let final_num = exact_num * target_denom as i128;
        match Self::div_round(final_num, exact_denom, how_rnd) {
            Ok(n) => {
                let res = GncNumeric::new(n, target_denom);
                if how_denom == GncNumericDenom::Reduce {
                    res.reduce()
                } else {
                    res
                }
            }
            Err(e) => Self::error(e),
        }
    }

    pub fn sub(
        a: Self,
        b: Self,
        denom: i64,
        how_rnd: GncNumericRounding,
        how_denom: GncNumericDenom,
    ) -> Self {
        Self::add(
            a,
            GncNumeric::new(-b.num, b.denom),
            denom,
            how_rnd,
            how_denom,
        )
    }

    pub fn mul(
        a: Self,
        b: Self,
        denom: i64,
        how_rnd: GncNumericRounding,
        how_denom: GncNumericDenom,
    ) -> Self {
        if a.check() != GncNumericErrorCode::Ok {
            return a;
        }
        if b.check() != GncNumericErrorCode::Ok {
            return b;
        }

        let exact_num = a.num as i128 * b.num as i128;
        let exact_denom = a.denom as i128 * b.denom as i128;

        let target_denom = if denom == GNC_DENOM_AUTO {
            match how_denom {
                GncNumericDenom::Reduce => {
                    let common = Self::gcd(exact_num, exact_denom);
                    (exact_denom / common) as i64
                }
                _ => exact_denom as i64,
            }
        } else {
            denom
        };

        let final_num = exact_num * target_denom as i128;
        match Self::div_round(final_num, exact_denom, how_rnd) {
            Ok(n) => {
                let res = GncNumeric::new(n, target_denom);
                if how_denom == GncNumericDenom::Reduce {
                    res.reduce()
                } else {
                    res
                }
            }
            Err(e) => Self::error(e),
        }
    }
}

#[derive(Debug)]
pub struct ParseNumericError;

impl std::fmt::Display for ParseNumericError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid numeric string")
    }
}

impl std::error::Error for ParseNumericError {}

impl FromStr for GncNumeric {
    type Err = ParseNumericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((num_str, denom_str)) = s.split_once('/') {
            let num = num_str.parse::<i64>().map_err(|_| ParseNumericError)?;
            let denom = denom_str.parse::<i64>().map_err(|_| ParseNumericError)?;
            Ok(GncNumeric::new(num, denom))
        } else {
            // Assume integer
            let num = s.parse::<i64>().map_err(|_| ParseNumericError)?;
            Ok(GncNumeric::new(num, 1))
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GncNumericRounding {
    Floor = 0x01,
    Ceil = 0x02,
    Trunc = 0x03,
    Promote = 0x04,
    RoundHalfDown = 0x05,
    RoundHalfUp = 0x06,
    Round = 0x07,
    Never = 0x08,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GncNumericDenom {
    Exact = 0x10,
    Reduce = 0x20,
    Lcd = 0x30,
    Fixed = 0x40,
    SigFig = 0x50,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GncNumericErrorCode {
    Ok = 0,
    Arg = -1,
    Overflow = -2,
    DenomDiff = -3,
    Remainder = -4,
}

pub const GNC_DENOM_AUTO: i64 = 0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reduce() {
        let n = GncNumeric::new(2, 4).reduce();
        assert_eq!(n.num, 1);
        assert_eq!(n.denom, 2);
    }

    #[test]
    fn test_from_str() {
        let n = GncNumeric::from_str("10129/100").unwrap();
        assert_eq!(n.num, 10129);
        assert_eq!(n.denom, 100);
    }

    #[test]
    fn test_bankers_rounding() {
        // 2.5 rounds to 2
        assert_eq!(
            GncNumeric::div_round(5, 2, GncNumericRounding::Round).unwrap(),
            2
        );
        // 3.5 rounds to 4
        assert_eq!(
            GncNumeric::div_round(7, 2, GncNumericRounding::Round).unwrap(),
            4
        );
    }

    #[test]
    fn test_add_simple() {
        let a = GncNumeric::new(1, 2);
        let b = GncNumeric::new(1, 4);
        let res = GncNumeric::add(
            a,
            b,
            GNC_DENOM_AUTO,
            GncNumericRounding::Never,
            GncNumericDenom::Reduce,
        );
        assert_eq!(res.num, 3);
        assert_eq!(res.denom, 4);
    }

    #[test]
    fn test_sub_simple() {
        let a = GncNumeric::new(1, 2);
        let b = GncNumeric::new(1, 4);
        let res = GncNumeric::sub(
            a,
            b,
            GNC_DENOM_AUTO,
            GncNumericRounding::Never,
            GncNumericDenom::Reduce,
        );
        assert_eq!(res.num, 1);
        assert_eq!(res.denom, 4);
    }

    #[test]
    fn test_mul_simple() {
        let a = GncNumeric::new(1, 2);
        let b = GncNumeric::new(1, 2);
        let res = GncNumeric::mul(
            a,
            b,
            GNC_DENOM_AUTO,
            GncNumericRounding::Never,
            GncNumericDenom::Reduce,
        );
        assert_eq!(res.num, 1);
        assert_eq!(res.denom, 4);
    }
}
