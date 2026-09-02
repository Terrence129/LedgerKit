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
    CatalogTextInvalid,
    BusinessIdInvalid,
    CategoryKindInvalid,
    SemanticRoleInvalid,
    SortOrderInvalid,
    PositiveValueRequired,
    FxSelfRateImmutable,
    PortfolioInstitutionMismatch,
    AccountBalanceNonzero,
    RevisionImmutable,
    AmountMustBePositive,
    AdjustmentZero,
    CategoryDirectionMismatch,
    TransferAccountSame,
    TransferCurrencyMismatch,
    ExchangeCurrencySame,
    FeeAccountCurrencyMismatch,
    FxOverrideReasonRequired,
    RevisionReasonRequired,
    RevisionTargetNotEffective,
    ReversalReasonRequired,
    EventAlreadyReversed,
    RevisionChainCycle,
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
            Self::CatalogTextInvalid => "CATALOG_TEXT_INVALID",
            Self::BusinessIdInvalid => "BUSINESS_ID_INVALID",
            Self::CategoryKindInvalid => "CATEGORY_KIND_INVALID",
            Self::SemanticRoleInvalid => "SEMANTIC_ROLE_INVALID",
            Self::SortOrderInvalid => "SORT_ORDER_INVALID",
            Self::PositiveValueRequired => "POSITIVE_VALUE_REQUIRED",
            Self::FxSelfRateImmutable => "FX_SELF_RATE_IMMUTABLE",
            Self::PortfolioInstitutionMismatch => "PORTFOLIO_INSTITUTION_MISMATCH",
            Self::AccountBalanceNonzero => "ACCOUNT_BALANCE_NONZERO",
            Self::RevisionImmutable => "MARKET_REVISION_IMMUTABLE",
            Self::AmountMustBePositive => "AMOUNT_MUST_BE_POSITIVE",
            Self::AdjustmentZero => "ADJUSTMENT_ZERO",
            Self::CategoryDirectionMismatch => "CATEGORY_DIRECTION_MISMATCH",
            Self::TransferAccountSame => "TRANSFER_ACCOUNT_SAME",
            Self::TransferCurrencyMismatch => "TRANSFER_CURRENCY_MISMATCH",
            Self::ExchangeCurrencySame => "EXCHANGE_CURRENCY_SAME",
            Self::FeeAccountCurrencyMismatch => "FEE_ACCOUNT_CURRENCY_MISMATCH",
            Self::FxOverrideReasonRequired => "FX_OVERRIDE_REASON_REQUIRED",
            Self::RevisionReasonRequired => "REVISION_REASON_REQUIRED",
            Self::RevisionTargetNotEffective => "REVISION_TARGET_NOT_EFFECTIVE",
            Self::ReversalReasonRequired => "REVERSAL_REASON_REQUIRED",
            Self::EventAlreadyReversed => "EVENT_ALREADY_REVERSED",
            Self::RevisionChainCycle => "REVISION_CHAIN_CYCLE",
        }
    }
}

impl Display for DomainError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for DomainError {}
