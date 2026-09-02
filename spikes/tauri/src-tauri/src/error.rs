use serde::Serialize;
use thiserror::Error;

pub type SpikeResult<T> = Result<T, SpikeError>;

#[derive(Debug, Error)]
pub enum SpikeError {
    #[error("decimal text is invalid")]
    DecimalInvalid,
    #[error("decimal scale exceeds the contract")]
    DecimalScaleExceeded,
    #[error("decimal precision exceeds the contract")]
    DecimalPrecisionExceeded,
    #[error("currency precision requires explicit confirmation")]
    CurrencyPrecisionConfirmationRequired,
    #[error("amount must be positive")]
    AmountMustBePositive,
    #[error("the requested event type is not supported")]
    EventTypeUnsupported,
    #[error("the request contains an invalid date")]
    DateInvalid,
    #[error("the page request is outside the bounded range")]
    PageInvalid,
    #[error("the selected file authorization is absent, expired, or already used")]
    FileAuthorizationRejected,
    #[error("the selected attachment is too large")]
    AttachmentTooLarge,
    #[error("the selected workbook does not match the known synthetic template")]
    WorkbookContractMismatch,
    #[error("the encrypted backup password or authentication tag is invalid")]
    BackupAuthenticationFailed,
    #[error("the backup KDF or format version is unsupported")]
    BackupVersionUnsupported,
    #[error("the backup payload failed integrity validation")]
    BackupIntegrityFailed,
    #[error("the requested backup identifier is not authorized")]
    BackupAuthorizationRejected,
    #[error("the operation was cancelled")]
    Cancelled,
    #[error("the synthetic failpoint rolled back the transaction")]
    SyntheticFailpoint,
    #[error("internal application state is unavailable")]
    InternalState,
    #[error("database operation failed")]
    Database(#[from] rusqlite::Error),
    #[error("file operation failed")]
    Io(#[from] std::io::Error),
    #[error("workbook operation failed")]
    Workbook(String),
    #[error("cryptographic operation failed")]
    Crypto,
    #[error("serialization failed")]
    Serialization(#[from] serde_json::Error),
}

impl SpikeError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::DecimalInvalid => "DECIMAL_INVALID",
            Self::DecimalScaleExceeded => "DECIMAL_SCALE_EXCEEDED",
            Self::DecimalPrecisionExceeded => "DECIMAL_PRECISION_EXCEEDED",
            Self::CurrencyPrecisionConfirmationRequired => {
                "CURRENCY_PRECISION_CONFIRMATION_REQUIRED"
            }
            Self::AmountMustBePositive => "AMOUNT_MUST_BE_POSITIVE",
            Self::EventTypeUnsupported => "EVENT_TYPE_UNSUPPORTED",
            Self::DateInvalid => "DATE_INVALID",
            Self::PageInvalid => "PAGE_INVALID",
            Self::FileAuthorizationRejected => "FILE_AUTHORIZATION_REJECTED",
            Self::AttachmentTooLarge => "ATTACHMENT_TOO_LARGE",
            Self::WorkbookContractMismatch => "WORKBOOK_CONTRACT_MISMATCH",
            Self::BackupAuthenticationFailed => "BACKUP_AUTHENTICATION_FAILED",
            Self::BackupVersionUnsupported => "BACKUP_KDF_OR_VERSION_UNSUPPORTED",
            Self::BackupIntegrityFailed => "BACKUP_INTEGRITY_FAILED",
            Self::BackupAuthorizationRejected => "BACKUP_AUTHORIZATION_REJECTED",
            Self::Cancelled => "OPERATION_CANCELLED",
            Self::SyntheticFailpoint => "SYNTHETIC_FAILPOINT",
            Self::InternalState => "INTERNAL_STATE_UNAVAILABLE",
            Self::Database(_) => "DATABASE_OPERATION_FAILED",
            Self::Io(_) => "FILE_OPERATION_FAILED",
            Self::Workbook(_) => "WORKBOOK_OPERATION_FAILED",
            Self::Crypto => "CRYPTOGRAPHIC_OPERATION_FAILED",
            Self::Serialization(_) => "SERIALIZATION_FAILED",
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl From<SpikeError> for CommandError {
    fn from(value: SpikeError) -> Self {
        Self {
            code: value.code().to_owned(),
            message: value.to_string(),
        }
    }
}
