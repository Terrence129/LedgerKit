#![forbid(unsafe_code)]

use std::fmt::{Display, Formatter};
use std::time::{SystemTime, UNIX_EPOCH};

use super::decimal::{Decimal, DecimalUse};
use super::error::DomainError;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Currency([u8; 3]);

impl Currency {
    /// Parses a three-letter uppercase currency code.
    ///
    /// # Errors
    ///
    /// Returns `CURRENCY_INVALID` when the code is not three uppercase ASCII letters.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let bytes: [u8; 3] = value
            .as_bytes()
            .try_into()
            .map_err(|_| DomainError::CurrencyInvalid)?;
        if !bytes.iter().all(u8::is_ascii_uppercase) {
            return Err(DomainError::CurrencyInvalid);
        }
        Ok(Self(bytes))
    }

    /// Returns the validated ASCII currency code.
    ///
    /// # Panics
    ///
    /// This cannot panic because construction validates all three bytes as ASCII.
    #[must_use]
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("currency is validated ASCII")
    }

    #[must_use]
    pub const fn common_scale(self) -> u32 {
        if matches!(
            self.0,
            [b'B', b'H', b'D'] | [b'K', b'W', b'D'] | [b'O', b'M', b'R']
        ) {
            3
        } else if matches!(self.0, [b'J', b'P', b'Y'] | [b'K', b'R', b'W']) {
            0
        } else {
            2
        }
    }
}

impl Display for Currency {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Money {
    amount: Decimal,
    currency: Currency,
}

impl Money {
    /// Constructs money while preserving the amount's meaningful source scale.
    ///
    /// # Errors
    ///
    /// Returns a Decimal error or requests explicit confirmation when the
    /// amount exceeds the currency's common display precision.
    pub fn parse(
        amount: &str,
        currency: Currency,
        currency_precision_confirmed: bool,
    ) -> Result<Self, DomainError> {
        let amount = Decimal::parse(amount, DecimalUse::Amount)?;
        if amount.scale() > currency.common_scale() && !currency_precision_confirmed {
            return Err(DomainError::CurrencyPrecisionConfirmationRequired);
        }
        Ok(Self { amount, currency })
    }

    #[must_use]
    pub const fn amount(&self) -> &Decimal {
        &self.amount
    }

    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LocalDate(String);

impl LocalDate {
    /// Parses an exact proleptic-Gregorian `YYYY-MM-DD` local date.
    ///
    /// # Errors
    ///
    /// Returns `LOCAL_DATE_INVALID` for malformed or impossible dates.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let bytes = value.as_bytes();
        if bytes.len() != 10
            || bytes[4] != b'-'
            || bytes[7] != b'-'
            || bytes
                .iter()
                .enumerate()
                .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
        {
            return Err(DomainError::LocalDateInvalid);
        }
        let year = parse_date_part(&bytes[0..4])?;
        let month = parse_date_part(&bytes[5..7])?;
        let day = parse_date_part(&bytes[8..10])?;
        let maximum_day = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if is_leap_year(year) => 29,
            2 => 28,
            _ => return Err(DomainError::LocalDateInvalid),
        };
        if year == 0 || day == 0 || day > maximum_day {
            return Err(DomainError::LocalDateInvalid);
        }
        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn parse_date_part(bytes: &[u8]) -> Result<u32, DomainError> {
    std::str::from_utf8(bytes)
        .map_err(|_| DomainError::LocalDateInvalid)?
        .parse()
        .map_err(|_| DomainError::LocalDateInvalid)
}

const fn is_leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && !year.is_multiple_of(100) || year.is_multiple_of(400)
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UuidV7([u8; 16]);

impl UuidV7 {
    /// Creates a `UUIDv7` from the current Unix-millisecond timestamp and OS randomness.
    ///
    /// # Errors
    ///
    /// Returns `UUID_V7_INVALID` when time or OS randomness is unavailable.
    pub fn new() -> Result<Self, DomainError> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| DomainError::UuidV7Invalid)?
            .as_millis();
        if timestamp > 0x0000_ffff_ffff_ffff {
            return Err(DomainError::UuidV7Invalid);
        }
        let mut random = [0u8; 10];
        getrandom::fill(&mut random).map_err(|_| DomainError::UuidV7Invalid)?;
        Self::from_parts(
            u64::try_from(timestamp).map_err(|_| DomainError::UuidV7Invalid)?,
            random,
        )
    }

    /// Creates a deterministic `UUIDv7` from validated layout parts.
    ///
    /// # Errors
    ///
    /// Returns `UUID_V7_INVALID` when the timestamp exceeds 48 bits.
    pub fn from_parts(timestamp_ms: u64, random: [u8; 10]) -> Result<Self, DomainError> {
        if timestamp_ms > 0x0000_ffff_ffff_ffff {
            return Err(DomainError::UuidV7Invalid);
        }
        let timestamp = timestamp_ms.to_be_bytes();
        let mut bytes = [0u8; 16];
        bytes[0..6].copy_from_slice(&timestamp[2..8]);
        bytes[6] = 0x70 | (random[0] & 0x0f);
        bytes[7] = random[1];
        bytes[8] = 0x80 | (random[2] & 0x3f);
        bytes[9..16].copy_from_slice(&random[3..10]);
        Ok(Self(bytes))
    }

    /// Parses and validates `UUIDv7` version and RFC variant bits.
    ///
    /// # Errors
    ///
    /// Returns `UUID_V7_INVALID` for invalid text, version, or variant bits.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let bytes = value.as_bytes();
        if bytes.len() != 36
            || bytes[8] != b'-'
            || bytes[13] != b'-'
            || bytes[18] != b'-'
            || bytes[23] != b'-'
        {
            return Err(DomainError::UuidV7Invalid);
        }
        let mut decoded = [0u8; 16];
        let mut output = 0;
        let mut input = 0;
        while input < bytes.len() {
            if matches!(input, 8 | 13 | 18 | 23) {
                input += 1;
                continue;
            }
            let high = decode_hex(bytes[input]).ok_or(DomainError::UuidV7Invalid)?;
            let low = decode_hex(bytes[input + 1]).ok_or(DomainError::UuidV7Invalid)?;
            decoded[output] = high << 4 | low;
            output += 1;
            input += 2;
        }
        if decoded[6] >> 4 != 7 || decoded[8] >> 6 != 2 {
            return Err(DomainError::UuidV7Invalid);
        }
        Ok(Self(decoded))
    }

    #[must_use]
    pub fn as_hyphenated(self) -> String {
        use std::fmt::Write as _;
        let hex = self
            .0
            .iter()
            .fold(String::with_capacity(32), |mut output, byte| {
                write!(output, "{byte:02x}").expect("writing to a String cannot fail");
                output
            });
        format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )
    }
}

impl Display for UuidV7 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.as_hyphenated())
    }
}

const fn decode_hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Sequence(u64);

impl Sequence {
    /// Constructs a positive canonical-JSON-safe ordering sequence.
    ///
    /// # Errors
    ///
    /// Returns `SEQUENCE_INVALID` for zero or values above the safe-integer limit.
    pub fn new(value: u64) -> Result<Self, DomainError> {
        if value == 0 || value > 9_007_199_254_740_991 {
            return Err(DomainError::SequenceInvalid);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalculationVersion(String);

impl CalculationVersion {
    /// Parses a bounded stable calculation-version identifier.
    ///
    /// # Errors
    ///
    /// Returns `CALCULATION_VERSION_INVALID` for empty, long, or unsafe identifiers.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        if value.is_empty()
            || value.len() > 64
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/')
            })
        {
            return Err(DomainError::CalculationVersionInvalid);
        }
        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProjectionWatermark(u64);

impl ProjectionWatermark {
    /// Constructs a canonical-JSON-safe projection watermark.
    ///
    /// # Errors
    ///
    /// Returns `PROJECTION_WATERMARK_INVALID` above the safe-integer limit.
    pub fn new(value: u64) -> Result<Self, DomainError> {
        if value > 9_007_199_254_740_991 {
            return Err(DomainError::ProjectionWatermarkInvalid);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CalculationVersion, Currency, LocalDate, Money, ProjectionWatermark, Sequence, UuidV7,
    };
    use crate::domain::error::DomainError;

    #[test]
    fn money_requires_explicit_currency_precision_confirmation() {
        let cny = Currency::parse("CNY").unwrap();
        assert_eq!(
            Money::parse("1.001", cny, false),
            Err(DomainError::CurrencyPrecisionConfirmationRequired)
        );
        assert_eq!(
            Money::parse("1.001", cny, true).unwrap().amount().as_str(),
            "1.001"
        );
    }

    #[test]
    fn local_date_validates_gregorian_boundaries() {
        assert_eq!(
            LocalDate::parse("2024-02-29").unwrap().as_str(),
            "2024-02-29"
        );
        assert_eq!(
            LocalDate::parse("2026-02-29"),
            Err(DomainError::LocalDateInvalid)
        );
        assert_eq!(
            LocalDate::parse("2026-01-01T00:00:00"),
            Err(DomainError::LocalDateInvalid)
        );
    }

    #[test]
    fn uuid_v7_round_trips_and_rejects_other_versions() {
        let id = UuidV7::from_parts(1_777_777_777_777, [0x11; 10]).unwrap();
        assert_eq!(UuidV7::parse(&id.to_string()), Ok(id));
        assert_eq!(
            UuidV7::parse("00000000-0000-4000-8000-000000000000"),
            Err(DomainError::UuidV7Invalid)
        );
    }

    #[test]
    fn sequence_and_calculation_versions_are_bounded() {
        assert_eq!(Sequence::new(0), Err(DomainError::SequenceInvalid));
        assert!(Sequence::new(1).is_ok());
        assert_eq!(
            ProjectionWatermark::new(9_007_199_254_740_992),
            Err(DomainError::ProjectionWatermarkInvalid)
        );
        assert!(CalculationVersion::parse("ledger-calculation-v1").is_ok());
        assert_eq!(
            CalculationVersion::parse("not valid"),
            Err(DomainError::CalculationVersionInvalid)
        );
    }
}
