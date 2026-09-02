#![forbid(unsafe_code)]

use crate::domain::settings::UiLocale;

pub const PRIVILEGED_OPERATION_COUNT: u8 = 11;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsError {
    InvalidLocale,
    StorageUnavailable,
}

pub trait SettingsRepository: Send {
    /// Loads the shell locale cache.
    ///
    /// # Errors
    ///
    /// Returns `StorageUnavailable` when the cache cannot be read safely.
    fn load_ui_locale(&self) -> Result<Option<UiLocale>, SettingsError>;
    /// Stores the shell locale cache.
    ///
    /// # Errors
    ///
    /// Returns `StorageUnavailable` when the cache cannot be persisted.
    fn save_ui_locale(&self, locale: UiLocale) -> Result<(), SettingsError>;
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{SettingsError, SettingsRepository};
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
    fn memory_repository_round_trips_locale() {
        let repository = MemoryRepository::default();
        assert_eq!(repository.load_ui_locale(), Ok(None));
        repository.save_ui_locale(UiLocale::EnUs).unwrap();
        assert_eq!(repository.load_ui_locale(), Ok(Some(UiLocale::EnUs)));
    }
}
