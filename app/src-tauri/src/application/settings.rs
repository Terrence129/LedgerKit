#![forbid(unsafe_code)]

use crate::domain::settings::UiLocale;

pub const PRIVILEGED_OPERATION_COUNT: u8 = 2;

#[derive(Debug, Eq, PartialEq)]
pub enum SettingsError {
    InvalidLocale,
    StorageUnavailable,
}

pub trait SettingsRepository: Send {
    fn load_ui_locale(&self) -> Result<Option<UiLocale>, SettingsError>;
    fn save_ui_locale(&self, locale: UiLocale) -> Result<(), SettingsError>;
}

#[derive(Debug, Eq, PartialEq)]
pub struct LedgerStatus {
    pub ui_locale: UiLocale,
}

pub struct SettingsService<R: SettingsRepository> {
    repository: R,
}

impl<R: SettingsRepository> SettingsService<R> {
    pub const fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn get_ledger_status(
        &self,
        system_locale_hint: Option<&str>,
    ) -> Result<LedgerStatus, SettingsError> {
        let ui_locale = self
            .repository
            .load_ui_locale()?
            .unwrap_or_else(|| UiLocale::from_system_hint(system_locale_hint));
        Ok(LedgerStatus { ui_locale })
    }

    pub fn update_ui_locale(&self, requested: &str) -> Result<UiLocale, SettingsError> {
        let locale = UiLocale::parse(requested).ok_or(SettingsError::InvalidLocale)?;
        self.repository.save_ui_locale(locale)?;
        Ok(locale)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{SettingsError, SettingsRepository, SettingsService};
    use crate::domain::settings::UiLocale;

    #[derive(Clone, Default)]
    struct MemoryRepository(Arc<Mutex<Option<UiLocale>>>);

    impl SettingsRepository for MemoryRepository {
        fn load_ui_locale(&self) -> Result<Option<UiLocale>, SettingsError> {
            self.0
                .lock()
                .map(|value| *value)
                .map_err(|_| SettingsError::StorageUnavailable)
        }

        fn save_ui_locale(&self, locale: UiLocale) -> Result<(), SettingsError> {
            *self
                .0
                .lock()
                .map_err(|_| SettingsError::StorageUnavailable)? = Some(locale);
            Ok(())
        }
    }

    #[test]
    fn persisted_choice_overrides_system_language_after_restart() {
        let repository = MemoryRepository::default();
        let service = SettingsService::new(repository.clone());
        assert_eq!(
            service.get_ledger_status(Some("zh-CN")).unwrap().ui_locale,
            UiLocale::ZhCn
        );
        assert_eq!(service.update_ui_locale("en-US"), Ok(UiLocale::EnUs));

        let restarted = SettingsService::new(repository);
        assert_eq!(
            restarted
                .get_ledger_status(Some("zh-CN"))
                .unwrap()
                .ui_locale,
            UiLocale::EnUs
        );
    }

    #[test]
    fn unsupported_locale_is_rejected_without_persistence() {
        let repository = MemoryRepository::default();
        let service = SettingsService::new(repository.clone());
        assert_eq!(
            service.update_ui_locale("fr-FR"),
            Err(SettingsError::InvalidLocale)
        );
        assert_eq!(repository.load_ui_locale(), Ok(None));
    }
}
