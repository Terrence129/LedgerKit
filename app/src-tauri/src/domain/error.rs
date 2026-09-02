#![forbid(unsafe_code)]

use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DomainError {
    DecimalInvalid,
    DecimalScaleExceeded,
    DecimalPrecisionExceeded,
    DecimalOverflow,
    CurrencyPrecisionConfirmationRequired,
    CurrencyInvalid,
    LocalDateInvalid,
    UuidV7Invalid,
    SequenceInvalid,
    CalculationVersionInvalid,
    ProjectionWatermarkInvalid,
    BaseCurrencyFrozen,
    CashAccountCurrencyFrozen,
    InstrumentCurrencyFrozen,
    EventInvariantViolation,
    PostingInvariantViolation,
}

impl DomainError {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::DecimalInvalid => "DECIMAL_INVALID",
            Self::DecimalScaleExceeded => "DECIMAL_SCALE_EXCEEDED",
            Self::DecimalPrecisionExceeded => "DECIMAL_PRECISION_EXCEEDED",
            Self::DecimalOverflow => "DECIMAL_OVERFLOW",
            Self::CurrencyPrecisionConfirmationRequired => {
                "CURRENCY_PRECISION_CONFIRMATION_REQUIRED"
            }
            Self::CurrencyInvalid => "CURRENCY_INVALID",
            Self::LocalDateInvalid => "LOCAL_DATE_INVALID",
            Self::UuidV7Invalid => "UUID_V7_INVALID",
            Self::SequenceInvalid => "SEQUENCE_INVALID",
            Self::CalculationVersionInvalid => "CALCULATION_VERSION_INVALID",
            Self::ProjectionWatermarkInvalid => "PROJECTION_WATERMARK_INVALID",
            Self::BaseCurrencyFrozen => "BASE_CURRENCY_FROZEN",
            Self::CashAccountCurrencyFrozen => "CASH_ACCOUNT_CURRENCY_FROZEN",
            Self::InstrumentCurrencyFrozen => "INSTRUMENT_TRADE_CURRENCY_FROZEN",
            Self::EventInvariantViolation => "EVENT_INVARIANT_VIOLATION",
            Self::PostingInvariantViolation => "POSTING_INVARIANT_VIOLATION",
        }
    }
}

impl Display for DomainError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for DomainError {}
