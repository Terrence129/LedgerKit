#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::application::settings::{SettingsError, SettingsRepository};
use crate::domain::settings::UiLocale;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredSettings {
    ui_locale: String,
}

pub struct FileSettingsRepository {
    path: PathBuf,
}

impl FileSettingsRepository {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn read(&self) -> Result<Option<StoredSettings>, SettingsError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| SettingsError::StorageUnavailable),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(SettingsError::StorageUnavailable),
        }
    }

    fn ensure_parent(path: &Path) -> Result<(), SettingsError> {
        let parent = path.parent().ok_or(SettingsError::StorageUnavailable)?;
        std::fs::create_dir_all(parent).map_err(|_| SettingsError::StorageUnavailable)
    }
}

impl SettingsRepository for FileSettingsRepository {
    fn load_ui_locale(&self) -> Result<Option<UiLocale>, SettingsError> {
        self.read()?
            .map(|settings| {
                UiLocale::parse(&settings.ui_locale).ok_or(SettingsError::StorageUnavailable)
            })
            .transpose()
    }

    fn save_ui_locale(&self, locale: UiLocale) -> Result<(), SettingsError> {
        Self::ensure_parent(&self.path)?;
        let bytes = serde_json::to_vec(&StoredSettings {
            ui_locale: locale.as_str().to_owned(),
        })
        .map_err(|_| SettingsError::StorageUnavailable)?;
        std::fs::write(&self.path, bytes).map_err(|_| SettingsError::StorageUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::FileSettingsRepository;
    use crate::application::settings::SettingsRepository;
    use crate::domain::settings::UiLocale;

    #[test]
    fn file_repository_round_trips_only_the_supported_locale() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ledgerkit-settings-{unique}.json"));
        let repository = FileSettingsRepository::new(path.clone());
        repository
            .save_ui_locale(UiLocale::ZhCn)
            .expect("locale should persist");
        assert_eq!(repository.load_ui_locale(), Ok(Some(UiLocale::ZhCn)));
        std::fs::remove_file(path).expect("test settings file should be removable");
    }
}
