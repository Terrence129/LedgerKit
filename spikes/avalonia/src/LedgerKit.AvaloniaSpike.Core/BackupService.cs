using System.Security.Cryptography;
using System.Text;
using System.Text.Json.Nodes;

namespace LedgerKit.AvaloniaSpike.Core;

internal sealed class BackupService
{
    private const int FormatVersion = 1;
    private const int Pbkdf2Iterations = 600_000;
    private const string KdfName = "pbkdf2-hmac-sha256-v1";

    private readonly LedgerStore ledger;
    private readonly string workingDirectory;

    public BackupService(LedgerStore ledger, string workingDirectory)
    {
        this.ledger = ledger;
        this.workingDirectory = Path.GetFullPath(workingDirectory);
        Directory.CreateDirectory(this.workingDirectory);
    }

    public BackupSummary Create(string backupId, string packagePath, string password)
    {
        if (password.Length < 8)
        {
            throw new SpikeException("CRYPTOGRAPHIC_OPERATION_FAILED", "Backup password is too short.");
        }

        var fullPackagePath = Path.GetFullPath(packagePath);
        Directory.CreateDirectory(Path.GetDirectoryName(fullPackagePath)!);
        if (File.Exists(fullPackagePath))
        {
            throw new SpikeException("FILE_OPERATION_FAILED", "Backup target already exists.");
        }

        var snapshotPath = Path.Combine(workingDirectory, $"{backupId}.snapshot.sqlite");
        var temporaryPackage = Path.Combine(workingDirectory, $"{backupId}.package.tmp");
        try
        {
            ledger.BackupDatabase(snapshotPath);
            LedgerStore.VerifyDatabaseFile(snapshotPath);
            var databaseBytes = File.ReadAllBytes(snapshotPath);
            var databaseHash = CanonicalJson.Sha256Prefixed(databaseBytes);
            var manifest = new JsonObject
            {
                ["format_version"] = FormatVersion,
                ["schema_version"] = LedgerStore.SchemaVersion,
                ["created_at_unix_seconds"] = checked((ulong)DateTimeOffset.UtcNow.ToUnixTimeSeconds()),
                ["database_sha256"] = databaseHash,
                ["calculation_version"] = LedgerStore.CalculationVersion,
                ["kdf"] = KdfName,
                ["kdf_iterations"] = Pbkdf2Iterations,
            };
            var envelope = Encrypt(databaseBytes, password, manifest);
            var packageBytes = CanonicalJson.Bytes(envelope);
            using (var stream = new FileStream(
                       temporaryPackage,
                       FileMode.CreateNew,
                       FileAccess.Write,
                       FileShare.None,
                       4096,
                       FileOptions.WriteThrough))
            {
                stream.Write(packageBytes);
                stream.Flush(true);
            }

            File.Move(temporaryPackage, fullPackagePath);
            var verified = Decrypt(fullPackagePath, password);
            CryptographicOperations.ZeroMemory(verified.DatabaseBytes);
            return new BackupSummary(
                backupId,
                FormatVersion,
                LedgerStore.SchemaVersion,
                databaseHash,
                packageBytes.LongLength,
                true);
        }
        finally
        {
            DeleteIfExists(snapshotPath);
            DeleteIfExists(temporaryPackage);
        }
    }

    public RestoreSummary Restore(string backupId, string packagePath, string password)
    {
        var payload = Decrypt(packagePath, password);
        if (payload.Manifest["schema_version"]!.GetValue<long>() != LedgerStore.SchemaVersion)
        {
            CryptographicOperations.ZeroMemory(payload.DatabaseBytes);
            throw new SpikeException("BACKUP_KDF_OR_VERSION_UNSUPPORTED", "Backup schema is unsupported.");
        }

        var candidatePath = Path.Combine(workingDirectory, $"{backupId}.restore-candidate.sqlite");
        var previousPath = Path.Combine(workingDirectory, $"{backupId}.pre-restore.sqlite");
        try
        {
            File.WriteAllBytes(candidatePath, payload.DatabaseBytes);
            LedgerStore.VerifyDatabaseFile(candidatePath);
            ledger.BackupDatabase(previousPath);
            LedgerStore.VerifyDatabaseFile(previousPath);
            try
            {
                ledger.RestoreDatabase(candidatePath);
            }
            catch
            {
                ledger.RestoreDatabase(previousPath);
                throw;
            }

            return new RestoreSummary(backupId, LedgerStore.SchemaVersion, true, true);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(payload.DatabaseBytes);
            DeleteIfExists(candidatePath);
            DeleteIfExists(previousPath);
        }
    }

    private static JsonObject Encrypt(byte[] plaintext, string password, JsonObject manifest)
    {
        var salt = RandomNumberGenerator.GetBytes(16);
        var nonce = RandomNumberGenerator.GetBytes(12);
        var key = DeriveKey(password, salt);
        var ciphertext = new byte[plaintext.Length];
        var tag = new byte[16];
        try
        {
            using var cipher = new AesGcm(key, tag.Length);
            cipher.Encrypt(nonce, plaintext, ciphertext, tag, CanonicalJson.Bytes(manifest));
            return new JsonObject
            {
                ["manifest"] = manifest,
                ["salt_base64"] = Convert.ToBase64String(salt),
                ["nonce_base64"] = Convert.ToBase64String(nonce),
                ["tag_base64"] = Convert.ToBase64String(tag),
                ["ciphertext_base64"] = Convert.ToBase64String(ciphertext),
            };
        }
        finally
        {
            CryptographicOperations.ZeroMemory(key);
        }
    }

    private static DecryptedPayload Decrypt(string path, string password)
    {
        JsonObject envelope;
        try
        {
            envelope = JsonNode.Parse(File.ReadAllBytes(path))?.AsObject() ??
                       throw new SpikeException("BACKUP_INTEGRITY_FAILED", "Backup envelope is empty.");
        }
        catch (SpikeException)
        {
            throw;
        }
        catch (Exception exception) when (exception is IOException or System.Text.Json.JsonException)
        {
            throw SpikeException.Wrap("BACKUP_INTEGRITY_FAILED", "Backup envelope is invalid.", exception);
        }

        var manifest = envelope["manifest"]?.AsObject() ??
                       throw new SpikeException("BACKUP_INTEGRITY_FAILED", "Backup manifest is absent.");
        if (manifest["format_version"]?.GetValue<int>() != FormatVersion ||
            manifest["kdf"]?.GetValue<string>() != KdfName ||
            manifest["kdf_iterations"]?.GetValue<int>() != Pbkdf2Iterations)
        {
            throw new SpikeException("BACKUP_KDF_OR_VERSION_UNSUPPORTED", "Backup KDF or format is unsupported.");
        }

        byte[] salt;
        byte[] nonce;
        byte[] tag;
        byte[] ciphertext;
        try
        {
            salt = Convert.FromBase64String(envelope["salt_base64"]!.GetValue<string>());
            nonce = Convert.FromBase64String(envelope["nonce_base64"]!.GetValue<string>());
            tag = Convert.FromBase64String(envelope["tag_base64"]!.GetValue<string>());
            ciphertext = Convert.FromBase64String(envelope["ciphertext_base64"]!.GetValue<string>());
        }
        catch (Exception exception) when (exception is FormatException or InvalidOperationException)
        {
            throw SpikeException.Wrap("BACKUP_INTEGRITY_FAILED", "Backup encoding is invalid.", exception);
        }

        if (salt.Length != 16 || nonce.Length != 12 || tag.Length != 16)
        {
            throw new SpikeException("BACKUP_INTEGRITY_FAILED", "Backup cryptographic fields are invalid.");
        }

        var key = DeriveKey(password, salt);
        var plaintext = new byte[ciphertext.Length];
        try
        {
            using var cipher = new AesGcm(key, tag.Length);
            cipher.Decrypt(nonce, ciphertext, tag, plaintext, CanonicalJson.Bytes(manifest));
        }
        catch (AuthenticationTagMismatchException exception)
        {
            CryptographicOperations.ZeroMemory(plaintext);
            throw SpikeException.Wrap(
                "BACKUP_AUTHENTICATION_FAILED",
                "Backup password or authentication tag is invalid.",
                exception);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(key);
        }

        var expectedHash = manifest["database_sha256"]!.GetValue<string>();
        if (CanonicalJson.Sha256Prefixed(plaintext) != expectedHash)
        {
            CryptographicOperations.ZeroMemory(plaintext);
            throw new SpikeException("BACKUP_INTEGRITY_FAILED", "Backup payload hash is invalid.");
        }

        return new DecryptedPayload(plaintext, manifest);
    }

    private static byte[] DeriveKey(string password, byte[] salt)
    {
        var passwordBytes = Encoding.UTF8.GetBytes(password);
        try
        {
            return Rfc2898DeriveBytes.Pbkdf2(
                passwordBytes,
                salt,
                Pbkdf2Iterations,
                HashAlgorithmName.SHA256,
                32);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(passwordBytes);
        }
    }

    private static void DeleteIfExists(string path)
    {
        if (File.Exists(path))
        {
            File.Delete(path);
        }
    }

    private sealed record DecryptedPayload(byte[] DatabaseBytes, JsonObject Manifest);
}
