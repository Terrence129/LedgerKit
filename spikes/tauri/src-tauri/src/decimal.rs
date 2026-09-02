use std::str::FromStr;

use rust_decimal::Decimal;

use crate::error::{SpikeError, SpikeResult};

pub const MAX_AMOUNT_SCALE: u32 = 8;
pub const MAX_SIGNIFICANT_DIGITS: usize = 28;

#[derive(Debug, Clone)]
pub struct ValidatedAmount {
    pub text: String,
    pub value: Decimal,
}

pub fn validate_positive_amount(
    text: &str,
    currency_precision_confirmed: bool,
) -> SpikeResult<ValidatedAmount> {
    validate_shape(text)?;

    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let scale = unsigned
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len() as u32);
    if scale > MAX_AMOUNT_SCALE {
        return Err(SpikeError::DecimalScaleExceeded);
    }

    let digits: String = unsigned.chars().filter(char::is_ascii_digit).collect();
    let first_significant = digits.find(|character| character != '0');
    let significant_digits = first_significant.map_or(1, |index| digits.len() - index);
    if significant_digits > MAX_SIGNIFICANT_DIGITS {
        return Err(SpikeError::DecimalPrecisionExceeded);
    }

    let value = Decimal::from_str(text).map_err(|_| SpikeError::DecimalInvalid)?;
    if value.is_zero() && text.starts_with('-') {
        return Err(SpikeError::DecimalInvalid);
    }
    if value <= Decimal::ZERO {
        return Err(SpikeError::AmountMustBePositive);
    }
    if scale > 2 && !currency_precision_confirmed {
        return Err(SpikeError::CurrencyPrecisionConfirmationRequired);
    }

    Ok(ValidatedAmount {
        text: text.to_owned(),
        value,
    })
}

pub fn parse_stored_decimal(text: &str) -> SpikeResult<Decimal> {
    Decimal::from_str(text).map_err(|_| SpikeError::DecimalInvalid)
}

fn validate_shape(text: &str) -> SpikeResult<()> {
    if text.is_empty()
        || text.starts_with('+')
        || text
            .chars()
            .any(|character| matches!(character, 'e' | 'E' | ',' | ' '))
        || text == "-"
    {
        return Err(SpikeError::DecimalInvalid);
    }

    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let mut parts = unsigned.split('.');
    let integer = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if parts.next().is_some()
        || integer.is_empty()
        || !integer.chars().all(|character| character.is_ascii_digit())
        || (integer.len() > 1 && integer.starts_with('0'))
        || fraction.is_some_and(|value| {
            value.is_empty() || !value.chars().all(|character| character.is_ascii_digit())
        })
    {
        return Err(SpikeError::DecimalInvalid);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_decimal_contract_order() {
        assert_eq!(
            validate_positive_amount("1e2", false)
                .expect_err("exponent must fail")
                .code(),
            "DECIMAL_INVALID"
        );
        assert_eq!(
            validate_positive_amount("1.000000000", true)
                .expect_err("scale must fail")
                .code(),
            "DECIMAL_SCALE_EXCEEDED"
        );
        assert_eq!(
            validate_positive_amount("0.00000001", false)
                .expect_err("confirmation must fail")
                .code(),
            "CURRENCY_PRECISION_CONFIRMATION_REQUIRED"
        );
        assert_eq!(
            validate_positive_amount("0.00000001", true)
                .expect("confirmed value")
                .text,
            "0.00000001"
        );
    }
}
