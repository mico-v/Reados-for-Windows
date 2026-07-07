using System.Collections.ObjectModel;
using ReadOS.Msp.Models;

namespace ReadOS.Msp.Hosting.Runtime;

public sealed class MspCommandRequestFactory
{
    public MspCommandRequestFactory(
        string defaultSessionId,
        string defaultActor = "agent",
        string defaultWorkingDirectory = "/")
    {
        DefaultSessionId = string.IsNullOrWhiteSpace(defaultSessionId)
            ? "default"
            : defaultSessionId.Trim();
        DefaultActor = string.IsNullOrWhiteSpace(defaultActor)
            ? "agent"
            : defaultActor.Trim();
        DefaultWorkingDirectory = string.IsNullOrWhiteSpace(defaultWorkingDirectory)
            ? "/"
            : defaultWorkingDirectory.Trim();
    }

    public string DefaultSessionId { get; }

    public string DefaultActor { get; }

    public string DefaultWorkingDirectory { get; }

    public MspCommandRequest Create(
        string commandText,
        string? actor = null,
        string? sessionId = null,
        string? workingDirectory = null,
        bool dryRun = false,
        IReadOnlyDictionary<string, string>? environment = null)
    {
        return new MspCommandRequest
        {
            CommandText = commandText,
            Actor = string.IsNullOrWhiteSpace(actor) ? DefaultActor : actor.Trim(),
            SessionId = string.IsNullOrWhiteSpace(sessionId) ? DefaultSessionId : sessionId.Trim(),
            WorkingDirectory = string.IsNullOrWhiteSpace(workingDirectory)
                ? DefaultWorkingDirectory
                : workingDirectory.Trim(),
            DryRun = dryRun,
            Environment = environment ?? ReadOnlyDictionary<string, string>.Empty
        };
    }
}
