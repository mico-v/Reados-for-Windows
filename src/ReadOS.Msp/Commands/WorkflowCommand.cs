using System.Text;
using System.Text.Json;
using ReadOS.Msp.Models;
using ReadOS.Msp.Runtime;

namespace ReadOS.Msp.Commands;

public sealed class WorkflowCommand : IMspCommand
{
    private const string ArtifactOption = "--artifact";

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true
    };

    public string Name => "workflow";

    public string Summary => "Summarize MSP workflow sessions and run named workflows.";

    public MspCommandMetadata Metadata => MspCommandMetadata.Create(
        Name,
        Summary,
        Usage(),
        MspCommandEffects.ReadWorkspace,
        new[] { "msp.workflow.read" });

    public MspCommandMetadata GetMetadata(IReadOnlyList<string> arguments)
    {
        var writesArtifact = HasArtifactOption(arguments);
        return MspCommandMetadata.Create(
            Name,
            Summary,
            Usage(),
            writesArtifact
                ? MspCommandEffects.ReadWorkspace | MspCommandEffects.WriteWorkspace | MspCommandEffects.CreateArtifact
                : MspCommandEffects.ReadWorkspace,
            writesArtifact
                ? new[] { "msp.workflow.read", "msp.artifact.write" }
                : new[] { "msp.workflow.read" });
    }

    public MspCommandPreview GetPreview(IReadOnlyList<string> arguments)
    {
        if (!TryParseWorkflow(arguments, out var spec, out var error))
        {
            return MspCommandPreview.Create(
                "Summarize MSP workflow session state or run a named workflow.",
                details: string.IsNullOrWhiteSpace(error) ? Array.Empty<string>() : new[] { error });
        }

        if (spec.IsNamedRun && !IsKnownNamedWorkflow(spec.WorkflowName))
        {
            return MspCommandPreview.Create(
                "Unknown named workflow.",
                new[] { spec.WorkflowName ?? string.Empty },
                new[] { "Use workflow run summarize-current or workflow run review-failures." });
        }

        var targets = new List<string>
        {
            string.Equals(spec.SessionSelector, "current", StringComparison.OrdinalIgnoreCase)
                ? "current session"
                : GetSessionPath(spec.SessionSelector)
        };
        if (!string.IsNullOrWhiteSpace(spec.ArtifactPath))
        {
            targets.Add(spec.ArtifactPath);
        }

        return MspCommandPreview.Create(
            spec.IsNamedRun
                ? $"Run named workflow {spec.WorkflowName} over the current MSP session."
                : "Summarize workflow commands, artifacts, diagnostics, and failures.",
            targets,
            spec.IsNamedRun
                ? new[] { $"workflow: {spec.WorkflowName}", $"session: {spec.SessionSelector}" }
                : new[] { $"session: {spec.SessionSelector}" });
    }

    public async ValueTask<MspCommandResult> ExecuteAsync(
        MspCommandContext context,
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken = default)
    {
        if (!TryParseWorkflow(arguments, out var spec, out var error))
        {
            return MspCommandResult.Failure(error, exitCode: 2);
        }

        if (spec.IsNamedRun && !IsKnownNamedWorkflow(spec.WorkflowName))
        {
            return MspCommandResult.Failure(
                $"Unknown workflow: {spec.WorkflowName}",
                exitCode: 2,
                code: "msp.workflow.unknown_workflow",
                target: spec.WorkflowName,
                recoveryHint: "Use workflow run summarize-current, workflow run review-failures, or run help to list available commands.");
        }

        var sessionId = ResolveSessionId(spec.SessionSelector, context);
        if (!IsSafeSessionId(sessionId))
        {
            return MspCommandResult.Failure(
                "Workflow session id must contain only letters, digits, '.', '-', or '_'.",
                exitCode: 2,
                code: "msp.workflow.invalid_session_id",
                target: spec.SessionSelector,
                recoveryHint: "Use workflow summary current, workflow run summarize-current, workflow run review-failures, or pass a session id from /sessions.");
        }

        string? artifactPath = null;
        if (!string.IsNullOrWhiteSpace(spec.ArtifactPath) &&
            !TryNormalizeArtifactPath(context, spec.ArtifactPath, out artifactPath, out error))
        {
            return MspCommandResult.Failure(
                error,
                exitCode: 2,
                code: "msp.workflow.invalid_artifact_path",
                target: spec.ArtifactPath,
                recoveryHint: "Use a file path such as /artifacts/workflows/current.md.");
        }

        await context.ReportProgressAsync(
            spec.IsNamedRun ? $"Running workflow {spec.WorkflowName}." : "Reading workflow session.",
            10,
            cancellationToken);
        var sessionPath = GetSessionPath(sessionId);
        var sessionContent = await context.Workspace.TryReadTextAsync(sessionPath, cancellationToken);
        if (sessionContent is null)
        {
            return MspCommandResult.Failure(
                $"Workflow session not found: {sessionId}",
                code: "msp.workflow.session_not_found",
                target: sessionPath,
                recoveryHint: "List /sessions or use workflow summary current after executing MSP commands.");
        }

        var session = DeserializeSession(sessionContent, sessionId);
        if (session is null)
        {
            return MspCommandResult.Failure(
                $"Workflow session is not valid JSON: {sessionPath}",
                code: "msp.workflow.invalid_session",
                target: sessionPath,
                recoveryHint: "Inspect the session projection before generating a workflow summary.");
        }

        await context.ReportProgressAsync("Reading workflow transcripts.", 35, cancellationToken);
        var (transcripts, sourcePaths, diagnostics) = await ReadTranscriptsAsync(
            context,
            session,
            sessionPath,
            cancellationToken);
        IReadOnlyList<string> sourceDocuments = Array.Empty<string>();
        IReadOnlyList<string> sourcePages = Array.Empty<string>();
        if (spec.IsNamedRun)
        {
            var artifactProvenance = await ReadSessionArtifactProvenanceAsync(context, session, cancellationToken);
            sourcePaths = sourcePaths
                .Concat(artifactProvenance.SourcePaths)
                .Where(path => !string.IsNullOrWhiteSpace(path))
                .Distinct(StringComparer.OrdinalIgnoreCase)
                .ToArray();
            sourceDocuments = artifactProvenance.SourceDocuments;
            sourcePages = artifactProvenance.SourcePages;
            diagnostics = diagnostics
                .Concat(artifactProvenance.Diagnostics)
                .ToArray();
        }

        await context.ReportProgressAsync("Building workflow report.", 65, cancellationToken);
        var summary = BuildSummary(session, transcripts, spec.WorkflowName);
        var report = BuildWorkflowReport(session, transcripts, spec.WorkflowName);
        var output = report.EndsWith(Environment.NewLine, StringComparison.Ordinal)
            ? report
            : report + Environment.NewLine;

        MspArtifact[] artifacts = Array.Empty<MspArtifact>();
        if (!string.IsNullOrWhiteSpace(artifactPath))
        {
            await context.ReportProgressAsync("Writing workflow report artifact.", 85, cancellationToken);
            var now = DateTimeOffset.UtcNow;
            var artifact = new MspArtifact
            {
                Path = artifactPath,
                MediaType = "text/markdown",
                SizeBytes = report.Length,
                Description = spec.IsNamedRun
                    ? $"Named workflow {spec.WorkflowName} created by MSP."
                    : "Workflow summary created by MSP.",
                SourceCommand = context.Invocation.CommandText,
                Actor = context.Invocation.Actor,
                SessionId = context.Invocation.SessionId,
                SourcePaths = sourcePaths
                    .Where(path => !string.IsNullOrWhiteSpace(path))
                    .Distinct(StringComparer.OrdinalIgnoreCase)
                    .ToArray(),
                SourceDocuments = sourceDocuments,
                SourcePages = sourcePages,
                CreatedAt = now,
                UpdatedAt = now,
                Preview = $"session: {session.Id}; commands: {session.CommandCount}; failures: {session.FailureCount}"
            };
            await context.Workspace.WriteTextAsync(artifactPath, report, artifact, cancellationToken);
            artifacts = new[] { artifact };
            output += $"artifact\t{artifactPath}\t{report.Length}{Environment.NewLine}";
        }

        await context.ReportProgressAsync("Workflow report complete.", 100, cancellationToken);
        return MspCommandResult.Success(output, artifacts, diagnostics);
    }

    private static async ValueTask<(
        IReadOnlyList<MspCommandTranscriptRecord> Transcripts,
        IReadOnlyList<string> SourcePaths,
        IReadOnlyList<MspCommandDiagnostic> Diagnostics)> ReadTranscriptsAsync(
        MspCommandContext context,
        MspSessionRecord session,
        string sessionPath,
        CancellationToken cancellationToken)
    {
        var transcripts = new List<MspCommandTranscriptRecord>();
        var sourcePaths = new List<string> { sessionPath };
        var diagnostics = new List<MspCommandDiagnostic>();
        if (session.TranscriptIds.Count > 0)
        {
            foreach (var transcriptId in session.TranscriptIds)
            {
                var transcriptPath = GetTranscriptPath(transcriptId);
                sourcePaths.Add(transcriptPath);
                var transcript = await ReadTranscriptAsync(context, transcriptPath, diagnostics, cancellationToken);
                if (transcript is not null)
                {
                    transcripts.Add(transcript);
                }
            }
        }
        else
        {
            var entries = await context.Workspace.ListAsync("/transcripts", cancellationToken);
            foreach (var entry in entries.Where(item => !item.IsDirectory && item.Path.EndsWith(".json", StringComparison.OrdinalIgnoreCase)))
            {
                var transcript = await ReadTranscriptAsync(context, entry.Path, diagnostics, cancellationToken);
                if (transcript is not null &&
                    string.Equals(transcript.SessionId, session.Id, StringComparison.OrdinalIgnoreCase))
                {
                    sourcePaths.Add(entry.Path);
                    transcripts.Add(transcript);
                }
            }
        }

        return (
            transcripts
                .OrderBy(item => item.StartedAt)
                .ThenBy(item => item.CompletedAt)
                .ToArray(),
            sourcePaths,
            diagnostics);
    }

    private static async ValueTask<WorkflowSourceProvenance> ReadSessionArtifactProvenanceAsync(
        MspCommandContext context,
        MspSessionRecord session,
        CancellationToken cancellationToken)
    {
        var sourcePaths = new List<string>();
        var sourceDocuments = new List<string>();
        var sourcePages = new List<string>();
        var diagnostics = new List<MspCommandDiagnostic>();

        foreach (var path in session.ArtifactPaths.Where(path => !string.IsNullOrWhiteSpace(path)))
        {
            var artifactPath = context.Workspace.NormalizePath(path);
            var manifestPath = GetArtifactManifestPath(artifactPath);
            sourcePaths.Add(artifactPath);
            sourcePaths.Add(manifestPath);

            var content = await context.Workspace.TryReadTextAsync(manifestPath, cancellationToken);
            if (content is null)
            {
                diagnostics.Add(new MspCommandDiagnostic
                {
                    Severity = MspDiagnosticSeverity.Warning,
                    Code = "msp.workflow.artifact_manifest_missing",
                    Target = manifestPath,
                    Message = "Workflow session references an artifact manifest that is not available.",
                    RecoveryHint = "Refresh the artifact projection before rerunning the named workflow."
                });
                continue;
            }

            MspArtifact? artifact;
            try
            {
                artifact = JsonSerializer.Deserialize<MspArtifact>(content, JsonOptions);
            }
            catch (JsonException)
            {
                diagnostics.Add(new MspCommandDiagnostic
                {
                    Severity = MspDiagnosticSeverity.Warning,
                    Code = "msp.workflow.artifact_manifest_invalid",
                    Target = manifestPath,
                    Message = "Workflow artifact manifest JSON could not be parsed.",
                    RecoveryHint = "Inspect the artifact manifest and rerun the named workflow."
                });
                continue;
            }

            if (artifact is null)
            {
                diagnostics.Add(new MspCommandDiagnostic
                {
                    Severity = MspDiagnosticSeverity.Warning,
                    Code = "msp.workflow.artifact_manifest_invalid",
                    Target = manifestPath,
                    Message = "Workflow artifact manifest JSON did not contain an artifact record.",
                    RecoveryHint = "Inspect the artifact manifest and rerun the named workflow."
                });
                continue;
            }

            sourcePaths.AddRange(artifact.SourcePaths);
            sourceDocuments.AddRange(artifact.SourceDocuments);
            sourcePages.AddRange(artifact.SourcePages);
        }

        return new WorkflowSourceProvenance(
            sourcePaths
                .Where(path => !string.IsNullOrWhiteSpace(path))
                .Distinct(StringComparer.OrdinalIgnoreCase)
                .ToArray(),
            sourceDocuments
                .Where(document => !string.IsNullOrWhiteSpace(document))
                .Distinct(StringComparer.OrdinalIgnoreCase)
                .ToArray(),
            sourcePages
                .Where(page => !string.IsNullOrWhiteSpace(page))
                .Distinct(StringComparer.OrdinalIgnoreCase)
                .ToArray(),
            diagnostics);
    }

    private static async ValueTask<MspCommandTranscriptRecord?> ReadTranscriptAsync(
        MspCommandContext context,
        string transcriptPath,
        ICollection<MspCommandDiagnostic> diagnostics,
        CancellationToken cancellationToken)
    {
        var content = await context.Workspace.TryReadTextAsync(transcriptPath, cancellationToken);
        if (content is null)
        {
            diagnostics.Add(new MspCommandDiagnostic
            {
                Severity = MspDiagnosticSeverity.Warning,
                Code = "msp.workflow.transcript_missing",
                Target = transcriptPath,
                Message = "Workflow session references a transcript that is not available.",
                RecoveryHint = "Refresh the workbench transcript projection before rerunning the summary."
            });
            return null;
        }

        try
        {
            return JsonSerializer.Deserialize<MspCommandTranscriptRecord>(content, JsonOptions);
        }
        catch (JsonException)
        {
            diagnostics.Add(new MspCommandDiagnostic
            {
                Severity = MspDiagnosticSeverity.Warning,
                Code = "msp.workflow.transcript_invalid",
                Target = transcriptPath,
                Message = "Workflow transcript JSON could not be parsed.",
                RecoveryHint = "Inspect the transcript projection and rerun the command."
            });
            return null;
        }
    }

    private static string BuildWorkflowReport(
        MspSessionRecord session,
        IReadOnlyList<MspCommandTranscriptRecord> transcripts,
        string? workflowName)
    {
        if (string.Equals(workflowName, "review-failures", StringComparison.OrdinalIgnoreCase))
        {
            return BuildFailureReview(session, transcripts, workflowName);
        }

        return BuildSummary(session, transcripts, workflowName);
    }

    private static string BuildSummary(
        MspSessionRecord session,
        IReadOnlyList<MspCommandTranscriptRecord> transcripts,
        string? workflowName)
    {
        var failures = transcripts.Where(item => item.ExitCode != 0).ToArray();
        var builder = new StringBuilder();
        builder.AppendLine("# MSP Workflow Summary");
        builder.AppendLine();
        if (!string.IsNullOrWhiteSpace(workflowName))
        {
            builder.AppendLine($"- workflow: {workflowName}");
        }

        builder.AppendLine($"- session: {session.Id}");
        builder.AppendLine($"- title: {session.Title}");
        builder.AppendLine($"- actor: {session.Actor}");
        builder.AppendLine($"- started: {FormatTimestamp(session.StartedAt)}");
        builder.AppendLine($"- updated: {FormatTimestamp(session.UpdatedAt)}");
        builder.AppendLine($"- commands: {session.CommandCount}");
        builder.AppendLine($"- approvals: {session.ApprovalCount}");
        builder.AppendLine($"- failures: {session.FailureCount}");
        builder.AppendLine($"- artifacts: {session.ArtifactPaths.Count}");
        builder.AppendLine();
        builder.AppendLine("## Commands");
        builder.AppendLine();
        if (transcripts.Count == 0)
        {
            builder.AppendLine("No transcript records are available for this workflow.");
        }
        else
        {
            builder.AppendLine("| # | exit | decision | command |");
            builder.AppendLine("| --- | ---: | --- | --- |");
            for (var i = 0; i < transcripts.Count; i++)
            {
                var entry = transcripts[i];
                builder.Append("| ");
                builder.Append(i + 1);
                builder.Append(" | ");
                builder.Append(entry.ExitCode);
                builder.Append(" | ");
                builder.Append(EscapeTableText(entry.Decision));
                builder.Append(" | `");
                builder.Append(EscapeTableText(TrimSingleLine(entry.CommandText, 140)));
                builder.AppendLine("` |");
            }
        }

        builder.AppendLine();
        builder.AppendLine("## Failures");
        builder.AppendLine();
        if (failures.Length == 0)
        {
            builder.AppendLine("No failures recorded.");
        }
        else
        {
            for (var i = 0; i < failures.Length; i++)
            {
                var failure = failures[i];
                builder.Append(i + 1);
                builder.Append(". `");
                builder.Append(TrimSingleLine(failure.CommandText, 160));
                builder.Append("` exited ");
                builder.AppendLine(failure.ExitCode.ToString());
                if (!string.IsNullOrWhiteSpace(failure.DiagnosticsSummary))
                {
                    builder.Append("   - diagnostics: ");
                    builder.AppendLine(TrimSingleLine(failure.DiagnosticsSummary, 220));
                }

                if (!string.IsNullOrWhiteSpace(failure.RecoveryHint))
                {
                    builder.Append("   - recovery: ");
                    builder.AppendLine(TrimSingleLine(failure.RecoveryHint, 220));
                }
            }
        }

        if (session.ArtifactPaths.Count > 0)
        {
            builder.AppendLine();
            builder.AppendLine("## Artifacts");
            builder.AppendLine();
            foreach (var path in session.ArtifactPaths)
            {
                builder.Append("- ");
                builder.AppendLine(path);
            }
        }

        return builder.ToString().TrimEnd();
    }

    private static string BuildFailureReview(
        MspSessionRecord session,
        IReadOnlyList<MspCommandTranscriptRecord> transcripts,
        string? workflowName)
    {
        var failures = transcripts
            .Where(item => item.ExitCode != 0)
            .OrderBy(item => item.CompletedAt)
            .ThenBy(item => item.CommandText, StringComparer.OrdinalIgnoreCase)
            .ToArray();
        var builder = new StringBuilder();
        builder.AppendLine("# MSP Failure Review");
        builder.AppendLine();
        if (!string.IsNullOrWhiteSpace(workflowName))
        {
            builder.AppendLine($"- workflow: {workflowName}");
        }

        builder.AppendLine($"- session: {session.Id}");
        builder.AppendLine($"- title: {session.Title}");
        builder.AppendLine($"- updated: {FormatTimestamp(session.UpdatedAt)}");
        builder.AppendLine($"- failures: {failures.Length}");
        builder.AppendLine($"- transcripts: {transcripts.Count}");
        builder.AppendLine();
        builder.AppendLine("## Recovery Queue");
        builder.AppendLine();
        if (failures.Length == 0)
        {
            builder.AppendLine("No failed transcript records are available for review.");
        }
        else
        {
            for (var i = 0; i < failures.Length; i++)
            {
                var failure = failures[i];
                builder.Append(i + 1);
                builder.Append(". `");
                builder.Append(TrimSingleLine(failure.CommandText, 180));
                builder.Append("`");
                builder.Append(" exited ");
                builder.AppendLine(failure.ExitCode.ToString());
                builder.Append("   - decision: ");
                builder.AppendLine(TrimSingleLine(failure.Decision, 120));
                if (!string.IsNullOrWhiteSpace(failure.DiagnosticsSummary))
                {
                    builder.Append("   - diagnostics: ");
                    builder.AppendLine(TrimSingleLine(failure.DiagnosticsSummary, 260));
                }
                else if (!string.IsNullOrWhiteSpace(failure.Stderr))
                {
                    builder.Append("   - stderr: ");
                    builder.AppendLine(TrimSingleLine(failure.Stderr, 260));
                }

                if (!string.IsNullOrWhiteSpace(failure.RecoveryHint))
                {
                    builder.Append("   - recovery: ");
                    builder.AppendLine(TrimSingleLine(failure.RecoveryHint, 260));
                }

                if (!string.IsNullOrWhiteSpace(failure.ArtifactsSummary))
                {
                    builder.Append("   - artifacts: ");
                    builder.AppendLine(TrimSingleLine(failure.ArtifactsSummary, 220));
                }
            }
        }

        if (session.ArtifactPaths.Count > 0)
        {
            builder.AppendLine();
            builder.AppendLine("## Session Artifacts");
            builder.AppendLine();
            foreach (var path in session.ArtifactPaths)
            {
                builder.Append("- ");
                builder.AppendLine(path);
            }
        }

        return builder.ToString().TrimEnd();
    }

    private static MspSessionRecord? DeserializeSession(string content, string fallbackSessionId)
    {
        try
        {
            var session = JsonSerializer.Deserialize<MspSessionRecord>(content, JsonOptions);
            if (session is null)
            {
                return null;
            }

            var id = string.IsNullOrWhiteSpace(session.Id) ? fallbackSessionId : session.Id;
            var title = string.IsNullOrWhiteSpace(session.Title) ? $"MSP session {id}" : session.Title;
            return session with
            {
                Id = id,
                Title = title
            };
        }
        catch (JsonException)
        {
            return null;
        }
        catch (InvalidOperationException)
        {
            return null;
        }
    }

    private static bool TryParseWorkflow(
        IReadOnlyList<string> arguments,
        out WorkflowSpec spec,
        out string error)
    {
        spec = new WorkflowSpec(string.Empty, null, null);
        error = string.Empty;
        if (arguments.Count == 0)
        {
            error = Usage();
            return false;
        }

        if (string.Equals(arguments[0], "summary", StringComparison.OrdinalIgnoreCase))
        {
            if (arguments.Count < 2 || string.IsNullOrWhiteSpace(arguments[1]))
            {
                error = Usage();
                return false;
            }

            if (!TryParseArtifactOption(arguments, 2, out var artifactPath, out error))
            {
                return false;
            }

            spec = new WorkflowSpec(arguments[1], artifactPath, null);
            return true;
        }

        if (string.Equals(arguments[0], "run", StringComparison.OrdinalIgnoreCase))
        {
            if (arguments.Count < 2 || string.IsNullOrWhiteSpace(arguments[1]))
            {
                error = Usage();
                return false;
            }

            if (!TryParseArtifactOption(arguments, 2, out var artifactPath, out error))
            {
                return false;
            }

            spec = new WorkflowSpec("current", artifactPath, arguments[1]);
            return true;
        }

        error = Usage();
        return false;
    }

    private static bool TryParseArtifactOption(
        IReadOnlyList<string> arguments,
        int startIndex,
        out string? artifactPath,
        out string error)
    {
        artifactPath = null;
        error = string.Empty;
        for (var i = startIndex; i < arguments.Count; i++)
        {
            var argument = arguments[i];
            if (string.Equals(argument, ArtifactOption, StringComparison.OrdinalIgnoreCase))
            {
                if (!string.IsNullOrWhiteSpace(artifactPath))
                {
                    error = "--artifact can only be specified once.";
                    return false;
                }

                if (i + 1 >= arguments.Count || string.IsNullOrWhiteSpace(arguments[i + 1]))
                {
                    error = "--artifact requires a path.";
                    return false;
                }

                artifactPath = arguments[++i];
                continue;
            }

            if (argument.StartsWith(ArtifactOption + "=", StringComparison.OrdinalIgnoreCase))
            {
                if (!string.IsNullOrWhiteSpace(artifactPath))
                {
                    error = "--artifact can only be specified once.";
                    return false;
                }

                artifactPath = argument[(ArtifactOption.Length + 1)..];
                if (string.IsNullOrWhiteSpace(artifactPath))
                {
                    error = "--artifact requires a path.";
                    return false;
                }

                continue;
            }

            error = $"Unexpected workflow argument: {argument}";
            return false;
        }

        return true;
    }

    private static bool HasArtifactOption(IReadOnlyList<string> arguments)
    {
        return arguments.Any(argument =>
            string.Equals(argument, ArtifactOption, StringComparison.OrdinalIgnoreCase) ||
            argument.StartsWith(ArtifactOption + "=", StringComparison.OrdinalIgnoreCase));
    }

    private static string ResolveSessionId(string selector, MspCommandContext context)
    {
        return string.Equals(selector, "current", StringComparison.OrdinalIgnoreCase)
            ? string.IsNullOrWhiteSpace(context.Invocation.SessionId) ? "default" : context.Invocation.SessionId
            : selector;
    }

    private static bool IsSafeSessionId(string sessionId)
    {
        return !string.IsNullOrWhiteSpace(sessionId) &&
            sessionId.All(character =>
                char.IsLetterOrDigit(character) ||
                character is '.' or '-' or '_');
    }

    private static bool TryNormalizeArtifactPath(
        MspCommandContext context,
        string path,
        out string normalized,
        out string error)
    {
        normalized = context.Workspace.NormalizePath(path, context.WorkingDirectory);
        if (normalized == "/artifacts" ||
            !normalized.StartsWith("/artifacts/", StringComparison.Ordinal) ||
            normalized.EndsWith("/", StringComparison.Ordinal))
        {
            error = "workflow --artifact must target a file under /artifacts.";
            return false;
        }

        error = string.Empty;
        return true;
    }

    private static string GetSessionPath(string sessionId)
    {
        return $"/sessions/{sessionId}.json";
    }

    private static string GetTranscriptPath(string transcriptId)
    {
        return $"/transcripts/{transcriptId}.json";
    }

    private static string GetArtifactManifestPath(string artifactPath)
    {
        return artifactPath + ".manifest.json";
    }

    private static string EscapeTableText(string value)
    {
        return TrimSingleLine(value, 240).Replace("|", "\\|", StringComparison.Ordinal);
    }

    private static string TrimSingleLine(string value, int maxLength)
    {
        var normalized = value
            .Replace("\r", " ", StringComparison.Ordinal)
            .Replace("\n", " ", StringComparison.Ordinal)
            .Trim();
        return normalized.Length <= maxLength ? normalized : normalized[..maxLength] + "...";
    }

    private static string FormatTimestamp(DateTimeOffset timestamp)
    {
        return timestamp.UtcDateTime.ToString("u");
    }

    private static string Usage()
    {
        return "Usage: workflow summary <session-id|current> [--artifact <path>] | workflow run summarize-current|review-failures [--artifact <path>]";
    }

    private static bool IsKnownNamedWorkflow(string? workflowName)
    {
        return string.Equals(workflowName, "summarize-current", StringComparison.OrdinalIgnoreCase) ||
            string.Equals(workflowName, "review-failures", StringComparison.OrdinalIgnoreCase);
    }

    private sealed record WorkflowSpec(string SessionSelector, string? ArtifactPath, string? WorkflowName)
    {
        public bool IsNamedRun => !string.IsNullOrWhiteSpace(WorkflowName);
    }

    private sealed record WorkflowSourceProvenance(
        IReadOnlyList<string> SourcePaths,
        IReadOnlyList<string> SourceDocuments,
        IReadOnlyList<string> SourcePages,
        IReadOnlyList<MspCommandDiagnostic> Diagnostics);
}
