#![forbid(unsafe_code)]

use std::str::FromStr;

use rust_decimal::{Decimal as RustDecimal, RoundingStrategy};

use super::error::DomainError;

pub const MAX_SIGNIFICANT_DIGITS: usize = 28;
pub const MAX_AMOUNT_SCALE: u32 = 8;
pub const MAX_QUANTITY_SCALE: u32 = 12;
pub const MAX_UNIT_PRICE_SCALE: u32 = 12;
pub const MAX_FX_RATE_SCALE: u32 = 15;
pub const MAX_INTERNAL_SCALE: u32 = 18;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecimalUse {
    Amount,
    Quantity,
    UnitPrice,
    FxRate,
    Internal,
}

impl DecimalUse {
    #[must_use]
    pub const fn maximum_scale(self) -> u32 {
        match self {
            Self::Amount => MAX_AMOUNT_SCALE,
            Self::Quantity => MAX_QUANTITY_SCALE,
            Self::UnitPrice => MAX_UNIT_PRICE_SCALE,
            Self::FxRate => MAX_FX_RATE_SCALE,
            Self::Internal => MAX_INTERNAL_SCALE,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Decimal {
    text: String,
    value: RustDecimal,
}

impl Decimal {
    /// Parses a canonical decimal string for a specific financial use.
    ///
    /// # Errors
    ///
    /// Returns the ADR-0004 error selected by syntax, scale, precision, then
    /// representation priority.
    pub fn parse(text: &str, usage: DecimalUse) -> Result<Self, DomainError> {
        validate_shape(text)?;
        let scale = decimal_scale(text);
        if scale > usage.maximum_scale() {
            return Err(DomainError::DecimalScaleExceeded);
        }
        if significant_digits(text) > MAX_SIGNIFICANT_DIGITS {
            return Err(DomainError::DecimalPrecisionExceeded);
        }
        let value = RustDecimal::from_str(text).map_err(|_| DomainError::DecimalOverflow)?;
        if value.is_zero() && text.starts_with('-') {
            return Err(DomainError::DecimalInvalid);
        }
        Ok(Self {
            text: text.to_owned(),
            value,
        })
    }

    #[must_use]
    pub fn zero(_usage: DecimalUse) -> Self {
        Self {
            text: "0".to_owned(),
            value: RustDecimal::ZERO,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub const fn scale(&self) -> u32 {
        self.value.scale()
    }

    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.value.is_zero()
    }

    /// Adds without implicit rounding.
    ///
    /// # Errors
    ///
    /// Returns a Decimal contract error when the result exceeds its limits.
    pub fn checked_add(&self, other: &Self, usage: DecimalUse) -> Result<Self, DomainError> {
        let value = self
            .value
            .checked_add(other.value)
            .ok_or(DomainError::DecimalOverflow)?;
        Self::from_derived(value, usage)
    }

    /// Negates without implicit rounding.
    ///
    /// # Errors
    ///
    /// Returns `DECIMAL_OVERFLOW` when the coefficient cannot be represented.
    pub fn checked_neg(&self, usage: DecimalUse) -> Result<Self, DomainError> {
        let value = RustDecimal::ZERO
            .checked_sub(self.value)
            .ok_or(DomainError::DecimalOverflow)?;
        Self::from_derived(value, usage)
    }

    /// Multiplies and applies the explicit internal scale-18 half-up boundary.
    ///
    /// # Errors
    ///
    /// Returns a Decimal contract error when the result cannot be represented.
    pub fn checked_mul_internal(&self, other: &Self) -> Result<Self, DomainError> {
        let value = self
            .value
            .checked_mul(other.value)
            .ok_or(DomainError::DecimalOverflow)?;
        Self::from_internal_boundary(value)
    }

    /// Divides and applies the explicit internal scale-18 half-up boundary.
    ///
    /// # Errors
    ///
    /// Returns a Decimal contract error for division by zero or an
    /// unrepresentable result.
    pub fn checked_div_internal(&self, other: &Self) -> Result<Self, DomainError> {
        let value = self
            .value
            .checked_div(other.value)
            .ok_or(DomainError::DecimalOverflow)?;
        Self::from_internal_boundary(value)
    }

    /// Rounds with ADR-0004 midpoint-away-from-zero (`half-up`) semantics.
    ///
    /// # Errors
    ///
    /// Returns a Decimal contract error for an invalid scale or result.
    pub fn round_half_up(&self, target_scale: u32) -> Result<Self, DomainError> {
        if target_scale > MAX_INTERNAL_SCALE {
            return Err(DomainError::DecimalScaleExceeded);
        }
        let rounded = self
            .value
            .round_dp_with_strategy(target_scale, RoundingStrategy::MidpointAwayFromZero);
        let text = format_with_scale(rounded, target_scale);
        Self::parse(&text, DecimalUse::Internal)
    }

    fn from_internal_boundary(value: RustDecimal) -> Result<Self, DomainError> {
        let bounded = if value.scale() > MAX_INTERNAL_SCALE {
            value.round_dp_with_strategy(MAX_INTERNAL_SCALE, RoundingStrategy::MidpointAwayFromZero)
        } else {
            value
        };
        Self::from_derived(bounded, DecimalUse::Internal)
    }

    fn from_derived(value: RustDecimal, usage: DecimalUse) -> Result<Self, DomainError> {
        let text = value.to_string();
        if significant_digits(&text) > MAX_SIGNIFICANT_DIGITS {
            return Err(DomainError::DecimalPrecisionExceeded);
        }
        Self::parse(&text, usage)
    }
}

fn validate_shape(text: &str) -> Result<(), DomainError> {
    if text.is_empty()
        || text.starts_with('+')
        || text == "-"
        || text
            .chars()
            .any(|character| matches!(character, 'e' | 'E' | ',' | ' '))
    {
        return Err(DomainError::DecimalInvalid);
    }
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let mut parts = unsigned.split('.');
    let integer = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if parts.next().is_some()
        || integer.is_empty()
        || !integer.chars().all(|character| character.is_ascii_digit())
        || (integer.len() > 1 && integer.starts_with('0'))
        || fraction.is_some_and(|digits| {
            digits.is_empty() || !digits.chars().all(|character| character.is_ascii_digit())
        })
    {
        return Err(DomainError::DecimalInvalid);
    }
    Ok(())
}

fn decimal_scale(text: &str) -> u32 {
    text.split_once('.').map_or(0, |(_, fraction)| {
        u32::try_from(fraction.len()).expect("validated Decimal text is bounded by input memory")
    })
}

fn significant_digits(text: &str) -> usize {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    digits
        .find(|character| character != '0')
        .map_or(1, |index| digits.len() - index)
}

fn format_with_scale(value: RustDecimal, scale: u32) -> String {
    format!("{value:.precision$}", precision = scale as usize)
}

#[cfg(test)]
mod tests {
    use super::{Decimal, DecimalUse};
    use crate::domain::error::DomainError;

    #[test]
    fn validation_order_and_contract_boundaries_are_stable() {
        assert_eq!(
            Decimal::parse("1e2", DecimalUse::Amount),
            Err(DomainError::DecimalInvalid)
        );
        assert_eq!(
            Decimal::parse("1.000000000", DecimalUse::Amount),
            Err(DomainError::DecimalScaleExceeded)
        );
        assert_eq!(
            Decimal::parse("12345678901234567890123456789", DecimalUse::Internal),
            Err(DomainError::DecimalPrecisionExceeded)
        );
        assert_eq!(
            Decimal::parse("9999999999999999999999999999", DecimalUse::Internal)
                .unwrap()
                .as_str(),
            "9999999999999999999999999999"
        );
        assert_eq!(
            Decimal::parse("-0.00", DecimalUse::Amount),
            Err(DomainError::DecimalInvalid)
        );
        assert_eq!(
            Decimal::parse("1.123456789012345", DecimalUse::FxRate)
                .unwrap()
                .as_str(),
            "1.123456789012345"
        );
    }

    #[test]
    fn half_up_rounding_is_away_from_zero_at_midpoints() {
        let positive = Decimal::parse("1.005", DecimalUse::Internal).unwrap();
        let negative = Decimal::parse("-1.005", DecimalUse::Internal).unwrap();
        assert_eq!(positive.round_half_up(2).unwrap().as_str(), "1.01");
        assert_eq!(negative.round_half_up(2).unwrap().as_str(), "-1.01");
    }

    #[test]
    fn each_financial_use_enforces_its_scale_and_internal_boundary() {
        for (usage, valid, invalid) in [
            (DecimalUse::Amount, "1.12345678", "1.123456789"),
            (DecimalUse::Quantity, "1.123456789012", "1.1234567890123"),
            (DecimalUse::UnitPrice, "1.123456789012", "1.1234567890123"),
            (
                DecimalUse::FxRate,
                "1.123456789012345",
                "1.1234567890123456",
            ),
            (
                DecimalUse::Internal,
                "1.123456789012345678",
                "1.1234567890123456789",
            ),
        ] {
            assert!(Decimal::parse(valid, usage).is_ok());
            assert_eq!(
                Decimal::parse(invalid, usage),
                Err(DomainError::DecimalScaleExceeded)
            );
        }
        let one = Decimal::parse("1", DecimalUse::Internal).unwrap();
        let six = Decimal::parse("6", DecimalUse::Internal).unwrap();
        assert_eq!(
            one.checked_div_internal(&six).unwrap().as_str(),
            "0.166666666666666667"
        );
    }

    #[test]
    fn arithmetic_reports_overflow_instead_of_wrapping() {
        let left = Decimal::parse("7922816251426433759354395033", DecimalUse::Internal).unwrap();
        let right = Decimal::parse("11", DecimalUse::Internal).unwrap();
        assert_eq!(
            left.checked_mul_internal(&right),
            Err(DomainError::DecimalOverflow)
        );
    }
}
