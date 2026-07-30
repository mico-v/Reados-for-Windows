using System.Security.Cryptography;
using System.Text;
using ReadOS.App.Services;

namespace ReadOS.App.Tests.Services;

public sealed class WindowsDpapiProviderCredentialStoreTests
{
    [Fact]
    public async Task Credential_is_protected_and_scoped_to_normalized_provider_endpoint()
    {
        using var directory = new TemporaryDirectory();
        var store = new WindowsDpapiProviderCredentialStore(directory.Path, new TestDataProtector());
        const string apiKey = "test-provider-secret";

        await store.SetApiKeyAsync("HTTPS://API.EXAMPLE.COM/v1/", apiKey);

        Assert.Equal(apiKey, await store.GetApiKeyAsync("https://api.example.com/v1"));
        Assert.Null(await store.GetApiKeyAsync("https://other.example.com/v1"));

        var credentialFile = Assert.Single(Directory.GetFiles(directory.Path, "*.dpapi"));
        var protectedPayload = await File.ReadAllBytesAsync(credentialFile);
        var plaintextHex = Convert.ToHexString(Encoding.UTF8.GetBytes(apiKey));
        Assert.DoesNotContain(plaintextHex, Convert.ToHexString(protectedPayload), StringComparison.Ordinal);
    }

    [Fact]
    public async Task Empty_api_key_removes_only_the_active_provider_credential()
    {
        using var directory = new TemporaryDirectory();
        var store = new WindowsDpapiProviderCredentialStore(directory.Path, new TestDataProtector());

        await store.SetApiKeyAsync("https://one.example/v1", "one-secret");
        await store.SetApiKeyAsync("https://two.example/v1", "two-secret");
        await store.SetApiKeyAsync("https://one.example/v1", string.Empty);

        Assert.Null(await store.GetApiKeyAsync("https://one.example/v1"));
        Assert.Equal("two-secret", await store.GetApiKeyAsync("https://two.example/v1"));
        Assert.Single(Directory.GetFiles(directory.Path, "*.dpapi"));
    }

    [Fact]
    public void Dpapi_protector_round_trips_for_the_current_windows_user()
    {
        var protector = new DpapiCurrentUserDataProtector();
        var plaintext = Encoding.UTF8.GetBytes("dpapi-round-trip-secret");
        var entropy = Encoding.UTF8.GetBytes("reados-test-entropy");
        byte[]? protectedPayload = null;
        byte[]? unprotectedPayload = null;
        try
        {
            protectedPayload = protector.Protect(plaintext, entropy);
            unprotectedPayload = protector.Unprotect(protectedPayload, entropy);

            Assert.Equal(plaintext, unprotectedPayload);
            Assert.NotEqual(Convert.ToHexString(plaintext), Convert.ToHexString(protectedPayload));
        }
        finally
        {
            CryptographicOperations.ZeroMemory(plaintext);
            CryptographicOperations.ZeroMemory(entropy);
            if (protectedPayload is not null)
            {
                CryptographicOperations.ZeroMemory(protectedPayload);
            }

            if (unprotectedPayload is not null)
            {
                CryptographicOperations.ZeroMemory(unprotectedPayload);
            }
        }
    }

    private sealed class TestDataProtector : ICurrentUserDataProtector
    {
        public byte[] Protect(byte[] plaintext, byte[] entropy)
        {
            return Transform(plaintext, entropy);
        }

        public byte[] Unprotect(byte[] protectedPayload, byte[] entropy)
        {
            return Transform(protectedPayload, entropy);
        }

        private static byte[] Transform(byte[] value, byte[] entropy)
        {
            var result = new byte[value.Length];
            for (var index = 0; index < value.Length; index++)
            {
                result[index] = (byte)(value[index] ^ entropy[index % entropy.Length] ^ 0xA5);
            }

            return result;
        }
    }

    private sealed class TemporaryDirectory : IDisposable
    {
        public TemporaryDirectory()
        {
            Path = System.IO.Path.Combine(
                System.IO.Path.GetTempPath(),
                "ReadOS.Tests",
                Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(Path);
        }

        public string Path { get; }

        public void Dispose()
        {
            if (Directory.Exists(Path))
            {
                Directory.Delete(Path, recursive: true);
            }
        }
    }
}
