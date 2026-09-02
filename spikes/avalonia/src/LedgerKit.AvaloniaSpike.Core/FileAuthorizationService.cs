using System.Security.Cryptography;

namespace LedgerKit.AvaloniaSpike.Core;

internal sealed class FileAuthorizationService
{
    private const long MaxAttachmentBytes = 5L * 1024 * 1024;

    private readonly string managedRoot;
    private readonly object syncRoot = new();
    private readonly Dictionary<string, AuthorizedFile> authorizations = new(StringComparer.Ordinal);

    public FileAuthorizationService(string managedRoot)
    {
        this.managedRoot = Path.GetFullPath(managedRoot);
        Directory.CreateDirectory(this.managedRoot);
    }

    public FileAuthorization AuthorizeSelectedFile(string selectedPath, string purpose)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(selectedPath);
        ArgumentException.ThrowIfNullOrWhiteSpace(purpose);
        var canonical = Path.GetFullPath(selectedPath);
        if (!File.Exists(canonical) || new FileInfo(canonical).Attributes.HasFlag(FileAttributes.ReparsePoint))
        {
            throw new SpikeException(
                "FILE_AUTHORIZATION_REJECTED",
                "Selected path is not an ordinary local file.");
        }

        var token = Convert.ToHexStringLower(RandomNumberGenerator.GetBytes(16));
        lock (syncRoot)
        {
            authorizations.Add(token, new AuthorizedFile(canonical, purpose));
        }

        return new FileAuthorization(token, Path.GetFileName(canonical));
    }

    public string Consume(string token, string expectedPurpose)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(token);
        lock (syncRoot)
        {
            if (!authorizations.Remove(token, out var authorization) ||
                authorization.Purpose != expectedPurpose ||
                !File.Exists(authorization.Path))
            {
                throw new SpikeException(
                    "FILE_AUTHORIZATION_REJECTED",
                    "File authorization is absent, invalid, or already used.");
            }

            return authorization.Path;
        }
    }

    public ManagedAttachment CopyAuthorizedAttachment(string token)
    {
        var source = Consume(token, "attachment");
        var metadata = new FileInfo(source);
        if (metadata.Length > MaxAttachmentBytes)
        {
            throw new SpikeException("ATTACHMENT_TOO_LARGE", "Selected attachment exceeds 5 MiB.");
        }

        var bytes = File.ReadAllBytes(source);
        var hash = CanonicalJson.Sha256Hex(bytes);
        var extension = SafeExtension(source);
        var managedName = extension.Length == 0 ? hash : $"{hash}.{extension}";
        var attachmentRoot = Path.Combine(managedRoot, "attachments");
        Directory.CreateDirectory(attachmentRoot);
        var destination = Path.Combine(attachmentRoot, managedName);
        if (!File.Exists(destination))
        {
            File.WriteAllBytes(destination, bytes);
        }

        return new ManagedAttachment(
            managedName,
            $"attachments/{managedName}",
            metadata.Length,
            CanonicalJson.Sha256Prefixed(bytes));
    }

    private static string SafeExtension(string path)
    {
        var extension = Path.GetExtension(path).TrimStart('.');
        return new string(extension
            .Where(char.IsAsciiLetterOrDigit)
            .Take(10)
            .Select(char.ToLowerInvariant)
            .ToArray());
    }

    private sealed record AuthorizedFile(string Path, string Purpose);
}
