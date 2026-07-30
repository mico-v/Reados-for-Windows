using System.Security.Cryptography;
using System.Text;

namespace ReadOS.App.Services;

public sealed class WindowsDpapiProviderCredentialStore : IProviderCredentialStore
{
    private const string CredentialDirectoryName = "ReadOS.Credentials";
    private const string CredentialFileExtension = ".dpapi";
    private const string EntropyPrefix = "ReadOS.ProviderApiKey.v1:";
    private static readonly UTF8Encoding StrictUtf8 = new(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);

    private readonly string credentialRoot;
    private readonly ICurrentUserDataProtector dataProtector;

    public WindowsDpapiProviderCredentialStore()
        : this(
            Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                CredentialDirectoryName),
            new DpapiCurrentUserDataProtector())
    {
    }

    internal WindowsDpapiProviderCredentialStore(
        string credentialRoot,
        ICurrentUserDataProtector dataProtector)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(credentialRoot);
        this.credentialRoot = Path.GetFullPath(credentialRoot);
        this.dataProtector = dataProtector ?? throw new ArgumentNullException(nameof(dataProtector));
    }

    public async Task<string?> GetApiKeyAsync(
        string providerBaseUrl,
        CancellationToken cancellationToken = default)
    {
        var scope = NormalizeScope(providerBaseUrl);
        var path = GetCredentialPath(scope);
        if (!File.Exists(path))
        {
            return null;
        }

        var protectedPayload = await File.ReadAllBytesAsync(path, cancellationToken);
        if (protectedPayload.Length == 0)
        {
            return null;
        }

        var entropy = BuildEntropy(scope);
        byte[]? plaintext = null;
        try
        {
            plaintext = dataProtector.Unprotect(protectedPayload, entropy);
            var apiKey = StrictUtf8.GetString(plaintext).Trim();
            return string.IsNullOrWhiteSpace(apiKey) ? null : apiKey;
        }
        finally
        {
            CryptographicOperations.ZeroMemory(protectedPayload);
            CryptographicOperations.ZeroMemory(entropy);
            if (plaintext is not null)
            {
                CryptographicOperations.ZeroMemory(plaintext);
            }
        }
    }

    public async Task SetApiKeyAsync(
        string providerBaseUrl,
        string? apiKey,
        CancellationToken cancellationToken = default)
    {
        var scope = NormalizeScope(providerBaseUrl);
        var path = GetCredentialPath(scope);
        var normalizedApiKey = apiKey?.Trim() ?? string.Empty;
        if (string.IsNullOrWhiteSpace(normalizedApiKey))
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (File.Exists(path))
            {
                File.Delete(path);
            }

            return;
        }

        Directory.CreateDirectory(credentialRoot);
        var plaintext = StrictUtf8.GetBytes(normalizedApiKey);
        var entropy = BuildEntropy(scope);
        byte[]? protectedPayload = null;
        var tempPath = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            protectedPayload = dataProtector.Protect(plaintext, entropy);
            await File.WriteAllBytesAsync(tempPath, protectedPayload, cancellationToken);
            File.Move(tempPath, path, overwrite: true);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(plaintext);
            CryptographicOperations.ZeroMemory(entropy);
            if (protectedPayload is not null)
            {
                CryptographicOperations.ZeroMemory(protectedPayload);
            }

            if (File.Exists(tempPath))
            {
                File.Delete(tempPath);
            }
        }
    }

    internal static string NormalizeScope(string? providerBaseUrl)
    {
        var value = providerBaseUrl?.Trim().TrimEnd('/') ?? string.Empty;
        if (!Uri.TryCreate(value, UriKind.Absolute, out var uri))
        {
            return value;
        }

        var builder = new UriBuilder(uri)
        {
            Scheme = uri.Scheme.ToLowerInvariant(),
            Host = uri.IdnHost.ToLowerInvariant(),
            Fragment = string.Empty
        };
        return builder.Uri.AbsoluteUri.TrimEnd('/');
    }

    private string GetCredentialPath(string scope)
    {
        var scopeBytes = StrictUtf8.GetBytes(scope);
        try
        {
            var hash = SHA256.HashData(scopeBytes);
            try
            {
                return Path.Combine(
                    credentialRoot,
                    Convert.ToHexString(hash).ToLowerInvariant() + CredentialFileExtension);
            }
            finally
            {
                CryptographicOperations.ZeroMemory(hash);
            }
        }
        finally
        {
            CryptographicOperations.ZeroMemory(scopeBytes);
        }
    }

    private static byte[] BuildEntropy(string scope)
    {
        return StrictUtf8.GetBytes(EntropyPrefix + scope);
    }
}

internal interface ICurrentUserDataProtector
{
    byte[] Protect(byte[] plaintext, byte[] entropy);

    byte[] Unprotect(byte[] protectedPayload, byte[] entropy);
}

internal sealed class DpapiCurrentUserDataProtector : ICurrentUserDataProtector
{
    public byte[] Protect(byte[] plaintext, byte[] entropy)
    {
        return ProtectedData.Protect(plaintext, entropy, DataProtectionScope.CurrentUser);
    }

    public byte[] Unprotect(byte[] protectedPayload, byte[] entropy)
    {
        return ProtectedData.Unprotect(protectedPayload, entropy, DataProtectionScope.CurrentUser);
    }
}
