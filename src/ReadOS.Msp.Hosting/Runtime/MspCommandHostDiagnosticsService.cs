namespace ReadOS.Msp.Hosting.Runtime;

public sealed class MspCommandHostDiagnosticsService
{
    public MspCommandHostDiagnostics Build(
        MspCommandHostComposition composition,
        MspCommandRequestFactory requestFactory)
    {
        ArgumentNullException.ThrowIfNull(composition);
        ArgumentNullException.ThrowIfNull(requestFactory);

        return new MspCommandHostDiagnostics
        {
            DefaultSessionId = requestFactory.DefaultSessionId,
            DefaultActor = requestFactory.DefaultActor,
            DefaultWorkingDirectory = requestFactory.DefaultWorkingDirectory,
            HostCommandPackName = composition.HostCommandPackName,
            CoreCommandCount = composition.CoreCommandNames.Count,
            HostCommandCount = composition.HostCommandNames.Count,
            CommandCount = composition.CommandNames.Count,
            HostCommandNames = composition.HostCommandNames.ToArray(),
            OverriddenCoreCommandNames = composition.OverriddenCoreCommandNames.ToArray()
        };
    }
}
