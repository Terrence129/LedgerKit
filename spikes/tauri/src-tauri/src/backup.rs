use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aes_gcm::aead::{Aead, KeyInit, Payload, consts::U12};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rusqlite::backup::Backup;
use rusqlite::{Connection, MAIN_DB};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::canonical::sha256_prefixed;
use crate::error::{SpikeError, SpikeResult};
use crate::ledger::{SCHEMA_VERSION, verify_connection, verify_database_file};

const FORMAT_VERSION: u32 = 1;
const KDF_NAME: &str = "argon2id-v1";
const ARGON_MEMORY_KIB: u32 = 19_456;
const ARGON_ITERATIONS: u32 = 2;
const ARGON_PARALLELISM: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub format_version: u32,
    pub schema_version: i64,
    pub created_at_unix_seconds: u64,
    pub database_sha256: String,
    pub calculation_version: String,
    pub kdf: String,
    pub kdf_memory_kib: u32,
    pub kdf_iterations: u32,
    pub kdf_parallelism: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupEnvelope {
    manifest: BackupManifest,
    salt_base64: String,
    nonce_base64: String,
    ciphertext_base64: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub backup_id: String,
    pub format_version: u32,
    pub schema_version: i64,
    pub database_sha256: String,
    pub package_bytes: u64,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSummary {
    pub backup_id: String,
    pub schema_version: i64,
    pub integrity_verified: bool,
    pub live_ledger_replaced: bool,
}

pub fn create_encrypted_backup(
    connection: &Connection,
    backup_path: &Path,
    password: &str,
) -> SpikeResult<BackupSummary> {
    if password.chars().count() < 8 {
        return Err(SpikeError::Crypto);
    }
    if let Some(parent) = backup_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let snapshot_path = backup_path.with_extension("snapshot.tmp.sqlite");
    if snapshot_path.exists() || backup_path.exists() {
        return Err(SpikeError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "backup target already exists",
        )));
    }
    connection.backup(MAIN_DB, &snapshot_path, None)?;
    let snapshot_result = (|| {
        verify_database_file(&snapshot_path)?;
        let database_bytes = std::fs::read(&snapshot_path)?;
        let database_sha256 = sha256_prefixed(&database_bytes);
        let manifest = BackupManifest {
            format_version: FORMAT_VERSION,
            schema_version: SCHEMA_VERSION,
            created_at_unix_seconds: unix_seconds(),
            database_sha256: database_sha256.clone(),
            calculation_version: "ledger-calculation-v1".to_owned(),
            kdf: KDF_NAME.to_owned(),
            kdf_memory_kib: ARGON_MEMORY_KIB,
            kdf_iterations: ARGON_ITERATIONS,
            kdf_parallelism: ARGON_PARALLELISM,
        };
        let envelope = encrypt_payload(&database_bytes, password, manifest)?;
        let package_bytes = serde_json::to_vec(&envelope)?;
        let temporary_path = backup_path.with_extension("package.tmp");
        {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary_path)?;
            file.write_all(&package_bytes)?;
            file.sync_all()?;
        }
        std::fs::rename(&temporary_path, backup_path)?;
        let (_, verified_manifest) = decrypt_payload(backup_path, password)?;
        Ok(BackupSummary {
            backup_id: backup_path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("ledgerkit-backup")
                .to_owned(),
            format_version: verified_manifest.format_version,
            schema_version: verified_manifest.schema_version,
            database_sha256,
            package_bytes: package_bytes.len() as u64,
            verified: true,
        })
    })();
    let _ = std::fs::remove_file(snapshot_path);
    snapshot_result
}

pub fn restore_encrypted_backup(
    live_connection: &mut Connection,
    backup_path: &Path,
    password: &str,
    working_directory: &Path,
) -> SpikeResult<RestoreSummary> {
    let (database_bytes, manifest) = decrypt_payload(backup_path, password)?;
    if manifest.schema_version != SCHEMA_VERSION {
        return Err(SpikeError::BackupVersionUnsupported);
    }
    std::fs::create_dir_all(working_directory)?;
    let nonce = unix_seconds();
    let candidate_path = working_directory.join(format!("restore-candidate-{nonce}.sqlite"));
    let pre_restore_path = working_directory.join(format!("pre-restore-{nonce}.sqlite"));
    std::fs::write(&candidate_path, &database_bytes)?;
    verify_database_file(&candidate_path)?;
    live_connection.backup(MAIN_DB, &pre_restore_path, None)?;
    verify_database_file(&pre_restore_path)?;

    let restore_result = (|| {
        let candidate = Connection::open(&candidate_path)?;
        {
            let backup = Backup::new(&candidate, live_connection)?;
            backup.run_to_completion(128, Duration::from_millis(1), None)?;
        }
        verify_connection(live_connection)?;
        let restored_schema: i64 =
            live_connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if restored_schema != SCHEMA_VERSION {
            return Err(SpikeError::BackupIntegrityFailed);
        }
        Ok(())
    })();

    if let Err(error) = restore_result {
        let previous = Connection::open(&pre_restore_path)?;
        {
            let rollback = Backup::new(&previous, live_connection)?;
            rollback.run_to_completion(128, Duration::from_millis(1), None)?;
        }
        let _ = std::fs::remove_file(&candidate_path);
        let _ = std::fs::remove_file(&pre_restore_path);
        return Err(error);
    }
    let _ = std::fs::remove_file(candidate_path);
    let _ = std::fs::remove_file(pre_restore_path);
    Ok(RestoreSummary {
        backup_id: backup_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("ledgerkit-backup")
            .to_owned(),
        schema_version: manifest.schema_version,
        integrity_verified: true,
        live_ledger_replaced: true,
    })
}

fn encrypt_payload(
    database_bytes: &[u8],
    password: &str,
    manifest: BackupManifest,
) -> SpikeResult<BackupEnvelope> {
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut salt).map_err(|_| SpikeError::Crypto)?;
    getrandom::fill(&mut nonce).map_err(|_| SpikeError::Crypto)?;
    let key = derive_key(password, &salt, &manifest)?;
    let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| SpikeError::Crypto)?;
    let aad = serde_json::to_vec(&manifest)?;
    let nonce_value: Nonce<U12> = nonce
        .as_slice()
        .try_into()
        .map_err(|_| SpikeError::Crypto)?;
    let ciphertext = cipher
        .encrypt(
            &nonce_value,
            Payload {
                msg: database_bytes,
                aad: &aad,
            },
        )
        .map_err(|_| SpikeError::Crypto)?;
    Ok(BackupEnvelope {
        manifest,
        salt_base64: BASE64.encode(salt),
        nonce_base64: BASE64.encode(nonce),
        ciphertext_base64: BASE64.encode(ciphertext),
    })
}

fn decrypt_payload(path: &Path, password: &str) -> SpikeResult<(Vec<u8>, BackupManifest)> {
    let envelope: BackupEnvelope = serde_json::from_slice(&std::fs::read(path)?)?;
    if envelope.manifest.format_version != FORMAT_VERSION
        || envelope.manifest.kdf != KDF_NAME
        || envelope.manifest.kdf_memory_kib != ARGON_MEMORY_KIB
        || envelope.manifest.kdf_iterations != ARGON_ITERATIONS
        || envelope.manifest.kdf_parallelism != ARGON_PARALLELISM
    {
        return Err(SpikeError::BackupVersionUnsupported);
    }
    let salt = BASE64
        .decode(envelope.salt_base64)
        .map_err(|_| SpikeError::BackupIntegrityFailed)?;
    let nonce = BASE64
        .decode(envelope.nonce_base64)
        .map_err(|_| SpikeError::BackupIntegrityFailed)?;
    let ciphertext = BASE64
        .decode(envelope.ciphertext_base64)
        .map_err(|_| SpikeError::BackupIntegrityFailed)?;
    if salt.len() != 16 || nonce.len() != 12 {
        return Err(SpikeError::BackupIntegrityFailed);
    }
    let key = derive_key(password, &salt, &envelope.manifest)?;
    let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| SpikeError::Crypto)?;
    let aad = serde_json::to_vec(&envelope.manifest)?;
    let nonce_value: Nonce<U12> = nonce
        .as_slice()
        .try_into()
        .map_err(|_| SpikeError::Crypto)?;
    let plaintext = cipher
        .decrypt(
            &nonce_value,
            Payload {
                msg: &ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| SpikeError::BackupAuthenticationFailed)?;
    let actual_hash = sha256_prefixed(&plaintext);
    if actual_hash != envelope.manifest.database_sha256 {
        return Err(SpikeError::BackupIntegrityFailed);
    }
    Ok((plaintext, envelope.manifest))
}

fn derive_key(
    password: &str,
    salt: &[u8],
    manifest: &BackupManifest,
) -> SpikeResult<Zeroizing<[u8; 32]>> {
    let parameters = Params::new(
        manifest.kdf_memory_kib,
        manifest.kdf_iterations,
        manifest.kdf_parallelism,
        Some(32),
    )
    .map_err(|_| SpikeError::Crypto)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, parameters);
    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| SpikeError::Crypto)?;
    Ok(key)
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::ledger::{LedgerStore, PostEventRequest};

    use super::*;

    #[test]
    fn encrypted_backup_restores_and_wrong_password_never_changes_live_data() {
        let directory = tempdir().unwrap();
        let store = LedgerStore::open(directory.path().join("live.sqlite")).unwrap();
        store.initialize_demo().unwrap();
        let original_watermark = store.status().unwrap().event_watermark;
        let package = directory.path().join("portable.ledgerkit-backup");
        store
            .with_connection(|connection| {
                create_encrypted_backup(connection, &package, "synthetic-password")
            })
            .unwrap();
        let encrypted_bytes = std::fs::read(&package).unwrap();
        assert!(
            !encrypted_bytes
                .windows(16)
                .any(|window| window == b"SQLite format 3\0")
        );
        assert!(
            !encrypted_bytes
                .windows(18)
                .any(|window| window == b"synthetic-password")
        );
        store
            .post_event(&PostEventRequest {
                event_type: "Income".to_owned(),
                effective_date: "2026-02-20".to_owned(),
                account_id: "cash-cny-1".to_owned(),
                amount: "10".to_owned(),
                currency: "CNY".to_owned(),
                category_id: Some("cat-income".to_owned()),
                currency_precision_confirmed: false,
                note: None,
            })
            .unwrap();
        let changed_watermark = store.status().unwrap().event_watermark;
        let wrong = store.with_connection(|connection| {
            restore_encrypted_backup(connection, &package, "wrong-password", directory.path())
        });
        assert_eq!(wrong.unwrap_err().code(), "BACKUP_AUTHENTICATION_FAILED");
        assert_eq!(store.status().unwrap().event_watermark, changed_watermark);

        store
            .with_connection(|connection| {
                restore_encrypted_backup(
                    connection,
                    &package,
                    "synthetic-password",
                    directory.path(),
                )
            })
            .unwrap();
        assert_eq!(store.status().unwrap().event_watermark, original_watermark);
    }
}
