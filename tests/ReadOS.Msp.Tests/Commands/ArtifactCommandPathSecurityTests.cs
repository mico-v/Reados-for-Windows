using ReadOS.Msp.Audit;
using ReadOS.Msp.Commands;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Tests.Commands;

public sealed class ArtifactCommandPathSecurityTests
{
    [Theory]
    [InlineData("../escaped.md")]
    [InlineData("..\\escaped.md")]
    [InlineData("/sessions/escaped.md")]
    [InlineData("C:\\temp\\escaped.md")]
    [InlineData("/artifacts/../escaped.md")]
    [InlineData("../escaped.md.manifest.json")]
    public async Task Write_rejects_paths_outside_artifact_namespace(string path)
    {
        var workspace = new InMemoryMspWorkspace();
        var command = new ArtifactCommand();

        var result = await command.ExecuteAsync(
            CreateContext(workspace),
            new[] { "write", path, "blocked" });

        Assert.False(result.Succeeded);
        Assert.Equal(2, result.ExitCode);
        Assert.Equal("msp.artifact.invalid_path", Assert.Single(result.Diagnostics).Code);
        Assert.Empty(await workspace.ListAsync("/"));
    }

    [Fact]
    public async Task Relative_artifact_path_is_resolved_inside_artifact_namespace()
    {
        var workspace = new InMemoryMspWorkspace();
        var command = new ArtifactCommand();

        var result = await command.ExecuteAsync(
            CreateContext(workspace),
            new[] { "write", "reports/summary.md", "safe" });

        Assert.True(result.Succeeded, result.Stderr);
        Assert.Equal("safe", await workspace.TryReadTextAsync("/artifacts/reports/summary.md"));
        Assert.NotNull(await workspace.TryReadTextAsync("/artifacts/reports/summary.md.manifest.json"));
    }

    [Fact]
    public async Task Manifest_aliases_keep_rename_and_delete_inside_artifact_namespace()
    {
        var workspace = new InMemoryMspWorkspace();
        var command = new ArtifactCommand();
        var context = CreateContext(workspace);
        await command.ExecuteAsync(
            context,
            new[] { "write", "/artifacts/source.md", "content" });

        var rename = await command.ExecuteAsync(
            context,
            new[]
            {
                "rename",
                "/artifacts/source.md.manifest.json",
                "/artifacts/archive/renamed.md.manifest.json"
            });

        Assert.True(rename.Succeeded, rename.Stderr);
        Assert.Null(await workspace.TryReadTextAsync("/artifacts/source.md"));
        Assert.Null(await workspace.TryReadTextAsync("/artifacts/source.md.manifest.json"));
        Assert.Equal("content", await workspace.TryReadTextAsync("/artifacts/archive/renamed.md"));
        Assert.NotNull(await workspace.TryReadTextAsync("/artifacts/archive/renamed.md.manifest.json"));

        var delete = await command.ExecuteAsync(
            context,
            new[] { "delete", "/artifacts/archive/renamed.md.manifest.json" });

        Assert.True(delete.Succeeded, delete.Stderr);
        Assert.Null(await workspace.TryReadTextAsync("/artifacts/archive/renamed.md"));
        Assert.Null(await workspace.TryReadTextAsync("/artifacts/archive/renamed.md.manifest.json"));
    }

    [Fact]
    public async Task Rename_and_delete_cannot_read_or_remove_files_outside_artifact_namespace()
    {
        var workspace = new InMemoryMspWorkspace();
        var command = new ArtifactCommand();
        var context = CreateContext(workspace);
        await workspace.WriteTextAsync("/sessions/protected.md", "secret");
        await command.ExecuteAsync(
            context,
            new[] { "write", "/artifacts/source.md", "content" });

        var renameSource = await command.ExecuteAsync(
            context,
            new[] { "rename", "../sessions/protected.md", "/artifacts/stolen.md" });
        var renameTarget = await command.ExecuteAsync(
            context,
            new[] { "rename", "/artifacts/source.md", "../escaped.md" });
        var delete = await command.ExecuteAsync(
            context,
            new[] { "delete", "..\\sessions\\protected.md" });

        Assert.False(renameSource.Succeeded);
        Assert.False(renameTarget.Succeeded);
        Assert.False(delete.Succeeded);
        Assert.All(
            new[] { renameSource, renameTarget, delete },
            result => Assert.Equal("msp.artifact.invalid_path", Assert.Single(result.Diagnostics).Code));
        Assert.Equal("secret", await workspace.TryReadTextAsync("/sessions/protected.md"));
        Assert.Equal("content", await workspace.TryReadTextAsync("/artifacts/source.md"));
        Assert.Null(await workspace.TryReadTextAsync("/artifacts/stolen.md"));
        Assert.Null(await workspace.TryReadTextAsync("/escaped.md"));
    }

    [Fact]
    public async Task List_rejects_artifact_prefix_siblings()
    {
        var workspace = new InMemoryMspWorkspace();
        await workspace.WriteTextAsync("/artifacts-archive/secret.md", "secret");

        var result = await new ArtifactCommand().ExecuteAsync(
            CreateContext(workspace),
            new[] { "list", "/artifacts-archive" });

        Assert.False(result.Succeeded);
        Assert.Equal(2, result.ExitCode);
        Assert.DoesNotContain("secret.md", result.Stdout, StringComparison.Ordinal);
    }

    private static MspCommandContext CreateContext(InMemoryMspWorkspace workspace)
    {
        return new MspCommandContext(
            workspace,
            new MspCommandRegistry(),
            new AllowAllMspPolicy(),
            new InMemoryMspAuditSink());
    }
}
