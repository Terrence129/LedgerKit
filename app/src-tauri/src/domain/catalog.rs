#![forbid(unsafe_code)]

use super::error::DomainError;

const MAX_TEXT_SCALARS: usize = 160;
const MAX_BUSINESS_ID_BYTES: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogText(String);

impl CatalogText {
    /// Validates bounded user-authored catalog text without translating or normalizing it.
    ///
    /// # Errors
    ///
    /// Returns `CATALOG_TEXT_INVALID` for blank, control-containing, or overlong text.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        if value.trim().is_empty()
            || value.chars().count() > MAX_TEXT_SCALARS
            || value.chars().any(char::is_control)
        {
            return Err(DomainError::CatalogTextInvalid);
        }
        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BusinessId(String);

impl BusinessId {
    /// Parses a stable, portable business identifier.
    ///
    /// # Errors
    ///
    /// Returns `BUSINESS_ID_INVALID` unless the identifier is 1-64 safe ASCII bytes.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        if value.is_empty()
            || value.len() > MAX_BUSINESS_ID_BYTES
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            })
        {
            return Err(DomainError::BusinessIdInvalid);
        }
        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CategoryKind {
    Income,
    Expense,
}

impl CategoryKind {
    /// Parses the stable category direction.
    ///
    /// # Errors
    ///
    /// Returns `CATEGORY_KIND_INVALID` for unsupported values.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "income" => Ok(Self::Income),
            "expense" => Ok(Self::Expense),
            _ => Err(DomainError::CategoryKindInvalid),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Income => "income",
            Self::Expense => "expense",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticRole {
    Normal,
    Refund,
    Reimbursement,
}

impl SemanticRole {
    /// Parses the stable, display-name-independent semantic role.
    ///
    /// # Errors
    ///
    /// Returns `SEMANTIC_ROLE_INVALID` for unsupported values.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "normal" => Ok(Self::Normal),
            "refund" => Ok(Self::Refund),
            "reimbursement" => Ok(Self::Reimbursement),
            _ => Err(DomainError::SemanticRoleInvalid),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Refund => "refund",
            Self::Reimbursement => "reimbursement",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SortOrder(u32);

impl SortOrder {
    /// Constructs a SQLite-safe stable sort order.
    ///
    /// # Errors
    ///
    /// Returns `SORT_ORDER_INVALID` above the signed 32-bit boundary.
    pub fn new(value: u32) -> Result<Self, DomainError> {
        if value > i32::MAX as u32 {
            return Err(DomainError::SortOrderInvalid);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{BusinessId, CatalogText, CategoryKind, SemanticRole, SortOrder};
    use crate::domain::error::DomainError;

    #[test]
    fn catalog_values_preserve_business_text_and_reject_unsafe_identifiers() {
        assert_eq!(CatalogText::parse("账户 A").unwrap().as_str(), "账户 A");
        assert_eq!(
            CatalogText::parse("  "),
            Err(DomainError::CatalogTextInvalid)
        );
        assert_eq!(
            BusinessId::parse("cash:sgd-1").unwrap().as_str(),
            "cash:sgd-1"
        );
        assert_eq!(
            BusinessId::parse("cash account"),
            Err(DomainError::BusinessIdInvalid)
        );
    }

    #[test]
    fn stable_enums_and_sort_order_reject_unknown_values() {
        assert_eq!(CategoryKind::parse("expense").unwrap().as_str(), "expense");
        assert_eq!(SemanticRole::parse("refund").unwrap().as_str(), "refund");
        assert_eq!(SortOrder::new(12).unwrap().get(), 12);
        assert_eq!(SortOrder::new(u32::MAX), Err(DomainError::SortOrderInvalid));
    }
}
