using System.IO.Compression;
using Microsoft.UI.Xaml.Media.Imaging;
using ReadOS.App.Models;
using ReadOS.App.Services;

namespace ReadOS.App.Tests.Services;

public sealed class WorkspaceStoreTests
{
    [Fact]
    public async Task Save_protects_api_key_without_serializing_it_into_workspace_json()
    {
        using var directory = new TemporaryDirectory();
        var credentialStore = new TestProviderCredentialStore();
        var workspaceRoot = Path.Combine(directory.Path, "workspace");
        var store = CreateStore(workspaceRoot, credentialStore);
        var state = new WorkspaceState
        {
            Settings = new WorkspaceSettings
            {
                ProviderBaseUrl = "https://api.example.com/v1",
                ProviderApiKey = "workspace-save-secret",
                UseOfflineResponses = false
            }
        };

        await store.SaveAsync(state);

        var serializedState = await File.ReadAllTextAsync(Path.Combine(workspaceRoot, "workspace.json"));
        Assert.False(serializedState.Contains("providerApiKey", StringComparison.OrdinalIgnoreCase));
        Assert.DoesNotContain("workspace-save-secret", serializedState, StringComparison.Ordinal);
        Assert.Equal(
            "workspace-save-secret",
            await credentialStore.GetApiKeyAsync("https://api.example.com/v1"));
    }

    [Fact]
    public async Task Load_migrates_legacy_plaintext_key_then_rewrites_sanitized_state()
    {
        using var directory = new TemporaryDirectory();
        var credentialStore = new TestProviderCredentialStore();
        var workspaceRoot = Path.Combine(directory.Path, "workspace");
        Directory.CreateDirectory(workspaceRoot);
        var statePath = Path.Combine(workspaceRoot, "workspace.json");
        await File.WriteAllTextAsync(
            statePath,
            """
            {
              "settings": {
                "providerName": "Legacy Provider",
                "providerBaseUrl": "https://legacy.example/v1",
                "providerApiKey": "legacy-workspace-secret",
                "useOfflineResponses": false
              },
              "projects": []
            }
            """);
        var store = CreateStore(workspaceRoot, credentialStore);

        var state = await store.LoadAsync();

        Assert.Equal("legacy-workspace-secret", state.Settings.ProviderApiKey);
        Assert.Equal(
            "legacy-workspace-secret",
            await credentialStore.GetApiKeyAsync("https://legacy.example/v1"));
        var sanitizedState = await File.ReadAllTextAsync(statePath);
        Assert.False(sanitizedState.Contains("providerApiKey", StringComparison.OrdinalIgnoreCase));
        Assert.DoesNotContain("legacy-workspace-secret", sanitizedState, StringComparison.Ordinal);
    }

    [Fact]
    public async Task Failed_legacy_migration_does_not_silently_delete_the_only_key_copy()
    {
        using var directory = new TemporaryDirectory();
        var workspaceRoot = Path.Combine(directory.Path, "workspace");
        Directory.CreateDirectory(workspaceRoot);
        var statePath = Path.Combine(workspaceRoot, "workspace.json");
        await File.WriteAllTextAsync(
            statePath,
            """
            {
              "settings": {
                "providerBaseUrl": "https://legacy.example/v1",
                "providerApiKey": "legacy-retry-secret"
              },
              "projects": []
            }
            """);
        var store = CreateStore(workspaceRoot, new FailingProviderCredentialStore());

        await Assert.ThrowsAsync<InvalidOperationException>(() => store.LoadAsync());

        var unchangedState = await File.ReadAllTextAsync(statePath);
        Assert.Contains("providerApiKey", unchangedState, StringComparison.OrdinalIgnoreCase);
        Assert.Contains("legacy-retry-secret", unchangedState, StringComparison.Ordinal);
    }

    [Fact]
    public async Task Load_without_a_credential_starts_with_an_empty_runtime_key()
    {
        using var directory = new TemporaryDirectory();
        var workspaceRoot = Path.Combine(directory.Path, "workspace");
        var store = CreateStore(workspaceRoot, new TestProviderCredentialStore());

        var state = await store.LoadAsync();

        Assert.Equal(string.Empty, state.Settings.ProviderApiKey);
        var serializedState = await File.ReadAllTextAsync(Path.Combine(workspaceRoot, "workspace.json"));
        Assert.False(serializedState.Contains("providerApiKey", StringComparison.OrdinalIgnoreCase));
    }

    [Fact]
    public async Task Credential_for_another_provider_is_not_attached_to_loaded_workspace()
    {
        using var directory = new TemporaryDirectory();
        var credentialStore = new TestProviderCredentialStore();
        await credentialStore.SetApiKeyAsync("https://trusted.example/v1", "trusted-provider-secret");
        var workspaceRoot = Path.Combine(directory.Path, "workspace");
        Directory.CreateDirectory(workspaceRoot);
        await File.WriteAllTextAsync(
            Path.Combine(workspaceRoot, "workspace.json"),
            """
            {
              "settings": {
                "providerBaseUrl": "https://imported.example/v1",
                "useOfflineResponses": false
              },
              "projects": []
            }
            """);
        var store = CreateStore(workspaceRoot, credentialStore);

        var state = await store.LoadAsync();

        Assert.Equal(string.Empty, state.Settings.ProviderApiKey);
        Assert.Equal(
            "trusted-provider-secret",
            await credentialStore.GetApiKeyAsync("https://trusted.example/v1"));
    }

    [Fact]
    public async Task Export_contains_sanitized_workspace_and_excludes_external_credential_store()
    {
        using var directory = new TemporaryDirectory();
        var workspaceRoot = Path.Combine(directory.Path, "workspace");
        var credentialRoot = Path.Combine(directory.Path, "credentials");
        var credentialStore = new WindowsDpapiProviderCredentialStore(
            credentialRoot,
            new PassthroughTestDataProtector());
        var store = CreateStore(workspaceRoot, credentialStore);
        var state = new WorkspaceState
        {
            Settings = new WorkspaceSettings
            {
                ProviderBaseUrl = "https://api.example.com/v1",
                ProviderApiKey = "workspace-export-secret",
                UseOfflineResponses = false
            }
        };
        var archivePath = Path.Combine(directory.Path, "workspace.zip");

        await store.ExportWorkspaceAsync(state, archivePath);

        Assert.NotEmpty(Directory.GetFiles(credentialRoot, "*.dpapi"));
        using var archive = ZipFile.OpenRead(archivePath);
        var stateEntry = Assert.Single(archive.Entries, entry => entry.FullName == "workspace.json");
        await using var stream = stateEntry.Open();
        using var reader = new StreamReader(stream);
        var serializedState = await reader.ReadToEndAsync();
        Assert.False(serializedState.Contains("providerApiKey", StringComparison.OrdinalIgnoreCase));
        Assert.DoesNotContain("workspace-export-secret", serializedState, StringComparison.Ordinal);
        Assert.DoesNotContain(
            archive.Entries,
            entry => entry.FullName.Contains("credential", StringComparison.OrdinalIgnoreCase) ||
                entry.FullName.EndsWith(".dpapi", StringComparison.OrdinalIgnoreCase));
    }

    private static WorkspaceStore CreateStore(
        string workspaceRoot,
        IProviderCredentialStore credentialStore)
    {
        return new WorkspaceStore(new TestPdfDocumentService(), credentialStore, workspaceRoot);
    }

    private sealed class TestProviderCredentialStore : IProviderCredentialStore
    {
        private readonly Dictionary<string, string> credentials = new(StringComparer.Ordinal);

        public Task<string?> GetApiKeyAsync(
            string providerBaseUrl,
            CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            credentials.TryGetValue(Normalize(providerBaseUrl), out var apiKey);
            return Task.FromResult<string?>(apiKey);
        }

        public Task SetApiKeyAsync(
            string providerBaseUrl,
            string? apiKey,
            CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var scope = Normalize(providerBaseUrl);
            if (string.IsNullOrWhiteSpace(apiKey))
            {
                credentials.Remove(scope);
            }
            else
            {
                credentials[scope] = apiKey.Trim();
            }

            return Task.CompletedTask;
        }

        private static string Normalize(string providerBaseUrl)
        {
            return WindowsDpapiProviderCredentialStore.NormalizeScope(providerBaseUrl);
        }
    }

    private sealed class FailingProviderCredentialStore : IProviderCredentialStore
    {
        public Task<string?> GetApiKeyAsync(
            string providerBaseUrl,
            CancellationToken cancellationToken = default)
        {
            return Task.FromResult<string?>(null);
        }

        public Task SetApiKeyAsync(
            string providerBaseUrl,
            string? apiKey,
            CancellationToken cancellationToken = default)
        {
            throw new InvalidOperationException("Credential protection is unavailable.");
        }
    }

    private sealed class PassthroughTestDataProtector : ICurrentUserDataProtector
    {
        public byte[] Protect(byte[] plaintext, byte[] entropy)
        {
            return plaintext.Reverse().Select(value => (byte)(value ^ 0x5A)).ToArray();
        }

        public byte[] Unprotect(byte[] protectedPayload, byte[] entropy)
        {
            return protectedPayload.Select(value => (byte)(value ^ 0x5A)).Reverse().ToArray();
        }
    }

    private sealed class TestPdfDocumentService : IPdfDocumentService
    {
        public Task<PdfDocumentInfo> InspectAsync(string path, CancellationToken cancellationToken = default)
        {
            return Task.FromResult(new PdfDocumentInfo(0, [], []));
        }

        public Task<BitmapImage> RenderPageAsync(
            string path,
            int pageNumber,
            double width,
            CancellationToken cancellationToken = default)
        {
            throw new NotSupportedException();
        }

        public Task<IReadOnlyList<PageImageItem>> RenderThumbnailsAsync(
            string path,
            int pageCount,
            int maxPages,
            CancellationToken cancellationToken = default)
        {
            return Task.FromResult<IReadOnlyList<PageImageItem>>([]);
        }

        public Task<IReadOnlyList<PdfTextHit>> SearchAsync(
            string path,
            string query,
            CancellationToken cancellationToken = default)
        {
            return Task.FromResult<IReadOnlyList<PdfTextHit>>([]);
        }

        public Task<string> ExtractPageTextAsync(
            string path,
            int startPage,
            int endPage,
            CancellationToken cancellationToken = default)
        {
            return Task.FromResult(string.Empty);
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
