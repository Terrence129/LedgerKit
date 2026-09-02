#![forbid(unsafe_code)]

use crate::domain::settings::UiLocale;
use crate::domain::types::Currency;

use super::error::{ApplicationError, ApplicationResult};
use super::ledger::{
    CreateLedgerCommand, LedgerPort, LedgerState, LedgerStatus, UpdateLedgerSettingsCommand,
};
use super::settings::{SettingsError, SettingsRepository};

pub struct ApplicationFacade<L: LedgerPort, S: SettingsRepository> {
    ledger: L,
    shell_settings: S,
}

impl<L: LedgerPort, S: SettingsRepository> ApplicationFacade<L, S> {
    pub const fn new(ledger: L, shell_settings: S) -> Self {
        Self {
            ledger,
            shell_settings,
        }
    }

    /// Creates the single ledger through validated application commands.
    ///
    /// # Errors
    ///
    /// Returns a stable application error when input validation, persistence,
    /// or shell-locale persistence fails.
    pub fn create_ledger(
        &mut self,
        base_currency: &str,
        ui_locale: &str,
    ) -> ApplicationResult<LedgerStatus> {
        let command = CreateLedgerCommand {
            base_currency: Currency::parse(base_currency)?,
            ui_locale: parse_locale(ui_locale)?,
        };
        let status = self.ledger.create_ledger(command)?;
        self.save_shell_locale(command.ui_locale)?;
        Ok(status)
    }

    /// Opens the fixed ledger owned by the local application-data directory.
    ///
    /// # Errors
    ///
    /// Returns a stable application error when identification, migration,
    /// verification, or opening fails.
    pub fn open_ledger(&mut self) -> ApplicationResult<LedgerStatus> {
        let status = self.ledger.open_ledger()?;
        self.save_shell_locale(status.ui_locale)?;
        Ok(status)
    }

    /// Returns status without opening or mutating a closed ledger.
    ///
    /// # Errors
    ///
    /// Returns a stable application error when shell settings cannot be read.
    pub fn get_ledger_status(
        &self,
        system_locale_hint: Option<&str>,
    ) -> ApplicationResult<LedgerStatus> {
        let fallback = self
            .shell_settings
            .load_ui_locale()
            .map_err(map_settings_error)?
            .unwrap_or_else(|| UiLocale::from_system_hint(system_locale_hint));
        self.ledger.get_ledger_status(fallback)
    }

    /// Updates mutable settings while enforcing ledger currency freezes.
    ///
    /// # Errors
    ///
    /// Returns a stable application error for invalid input, a closed ledger
    /// when ledger-owned settings are requested, or persistence failure.
    pub fn update_settings(
        &mut self,
        ui_locale: &str,
        base_currency: Option<&str>,
        valuation_defaults_json: Option<String>,
    ) -> ApplicationResult<LedgerStatus> {
        let locale = parse_locale(ui_locale)?;
        let command = UpdateLedgerSettingsCommand {
            base_currency: base_currency.map(Currency::parse).transpose()?,
            ui_locale: locale,
            valuation_defaults_json,
        };
        let mut status = self.ledger.get_ledger_status(locale)?;
        if status.state == LedgerState::Open {
            status = self.ledger.update_settings(&command)?;
        } else if command.base_currency.is_some() || command.valuation_defaults_json.is_some() {
            return Err(ApplicationError::LedgerNotOpen);
        } else {
            status.ui_locale = locale;
        }
        self.save_shell_locale(locale)?;
        Ok(status)
    }

    fn save_shell_locale(&self, locale: UiLocale) -> ApplicationResult<()> {
        self.shell_settings
            .save_ui_locale(locale)
            .map_err(map_settings_error)
    }
}

fn parse_locale(value: &str) -> ApplicationResult<UiLocale> {
    UiLocale::parse(value).ok_or(ApplicationError::InvalidLocale)
}

const fn map_settings_error(error: SettingsError) -> ApplicationError {
    match error {
        SettingsError::InvalidLocale => ApplicationError::InvalidLocale,
        SettingsError::StorageUnavailable => ApplicationError::StorageUnavailable,
    }
}
