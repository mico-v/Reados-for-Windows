namespace ReadOS.App.Services;

public interface IProviderCredentialStore
{
    Task<string?> GetApiKeyAsync(
        string providerBaseUrl,
        CancellationToken cancellationToken = default);

    Task SetApiKeyAsync(
        string providerBaseUrl,
        string? apiKey,
        CancellationToken cancellationToken = default);
}
