using ReadOS.Msp.Hosting.Native;
using System.Text.Json;
using ReadOS.Msp.Audit;
using ReadOS.Msp.Hosting.Runtime;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Hosting.Tests.Native;

public sealed class MspNativeReleaseDllIntegrationTests
{
    [ExplicitNativeDllFact]
    public void Explicit_release_dll_enforces_parse_operation_request_cap_without_disclosure()
    {
        var libraryPath = Environment.GetEnvironmentVariable(ExplicitNativeDllFactAttribute.EnvironmentVariable)!;
        using var adapter = MspNativeAdapter.LoadWindows(new MspNativeLibraryOptions
        {
            LibraryPath = libraryPath
        });
        const string secret = @"V:\private\oversized-parse-secret";
        var commandText = $"echo {secret} {new string('x', 160 * 1024)}";
        var requestJson = JsonSerializer.SerializeToUtf8Bytes(new
        {
            contractVersion = MspNativeContract.Version,
            commandText
        });
        Assert.True(requestJson.Length > 128 * 1024);
        Assert.True(requestJson.Length < MspNativeAdapterLimits.DefaultMaximumRequestBytes);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Parse(new MspNativeShellParseRequest
            {
                CommandText = commandText
            }));

        Assert.Equal(MspNativeFailureKind.NativeRequestTooLarge, exception.FailureKind);
        Assert.Equal(MspNativeOperation.Parse, exception.Operation);
        Assert.Equal("msp.native.request_too_large", exception.Diagnostic.Code);
        Assert.DoesNotContain(secret, exception.ToString());
        Assert.Null(exception.InnerException);
    }

    [ExplicitNativeDllFact]
    public void Explicit_release_dll_matches_execute_parse_and_path_contracts()
    {
        var libraryPath = Environment.GetEnvironmentVariable(ExplicitNativeDllFactAttribute.EnvironmentVariable)!;
        Assert.True(Path.IsPathFullyQualified(libraryPath));
        Assert.True(File.Exists(libraryPath));

        using var adapter = MspNativeAdapter.LoadWindows(new MspNativeLibraryOptions
        {
            LibraryPath = libraryPath
        });

        var runtimeInfo = ((IMspNativeRuntimeInfoProvider)adapter).NativeRuntimeInfo;
        Assert.Equal(MspNativeAbiMode.LengthDelimitedV2, runtimeInfo.AbiMode);
        Assert.Equal(2U, runtimeInfo.MajorVersion);
        Assert.Equal(0U, runtimeInfo.MinorVersion);
        Assert.Equal(MspNativeContract.AbiV2ContractId, runtimeInfo.ContractId);
        Assert.Equal(
            MspNativeContract.AbiV2RequiredCapabilities,
            runtimeInfo.Capabilities & MspNativeContract.AbiV2RequiredCapabilities);

        var pwd = adapter.Execute(new MspNativeCommandRequest
        {
            CommandText = "pwd",
            WorkingDirectory = "/documents",
            Actor = "dotnet-integration",
            SessionId = "native-release"
        });
        Assert.Equal(0, pwd.ExitCode);
        Assert.Equal("/documents\n", pwd.StdoutText);

        var echo = adapter.Execute(new MspNativeCommandRequest
        {
            CommandText = "echo ''"
        });
        Assert.Equal("\n", echo.StdoutText);

        var unknown = adapter.Execute(new MspNativeCommandRequest
        {
            CommandText = "missing-command"
        });
        Assert.Equal(127, unknown.ExitCode);
        Assert.Equal("msp.command_not_found", Assert.Single(unknown.Diagnostics).Code);

        var parsed = adapter.Parse(new MspNativeShellParseRequest
        {
            CommandText = "echo ''"
        });
        Assert.True(parsed.Succeeded);
        Assert.True(Assert.Single(
            Assert.Single(parsed.Script!.Pipelines).Commands[0].ArgumentWords)
            .HasExplicitEmptyQuotedFragment);

        var normalized = adapter.NormalizeWorkspacePath(new MspNativeWorkspacePathRequest
        {
            Path = "../../reports/a.txt",
            CurrentDirectory = "/docs/current"
        });
        Assert.True(normalized.Succeeded);
        Assert.Equal("/reports/a.txt", normalized.VirtualPath);

        var workspaceRoot = Path.Combine(
            Path.GetTempPath(),
            "reados-native-integration-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(workspaceRoot);
        try
        {
            var binary = new byte[] { 0x00, 0xff, (byte)'A', (byte)'\n' };
            File.WriteAllBytes(Path.Combine(workspaceRoot, "binary.dat"), binary);

            var listed = adapter.Execute(new MspNativeCommandRequest
            {
                CommandText = "ls /",
                WorkspaceRoot = workspaceRoot
            });
            Assert.Equal(0, listed.ExitCode);
            Assert.Contains("binary.dat\n", listed.StdoutText);

            var read = adapter.Execute(new MspNativeCommandRequest
            {
                CommandText = "cat /binary.dat",
                WorkspaceRoot = workspaceRoot
            });
            Assert.Equal(0, read.ExitCode);
            Assert.Equal(binary, read.StdoutBytes.ToArray());
            Assert.DoesNotContain(workspaceRoot, listed.StdoutText);
            Assert.DoesNotContain(workspaceRoot, listed.StderrText);
            Assert.DoesNotContain(workspaceRoot, JsonSerializer.Serialize(listed.Diagnostics));
            Assert.DoesNotContain(workspaceRoot, JsonSerializer.Serialize(listed.AuditRecords));
        }
        finally
        {
            Directory.Delete(workspaceRoot, recursive: true);
        }
    }

    [ExplicitNativeDllFact]
    public async Task Explicit_release_dll_runs_through_managed_policy_and_single_audit_proxy()
    {
        var libraryPath = Environment.GetEnvironmentVariable(
            ExplicitNativeDllFactAttribute.EnvironmentVariable)!;
        using var provider = new LazyMspNativeAdapterProvider(() =>
            MspNativeAdapter.LoadWindows(new MspNativeLibraryOptions
            {
                LibraryPath = libraryPath
            }));
        var registry = MspRuntime.CreateDefaultRegistry();
        Assert.True(registry.TryGet("pwd", out var pwdDefinition));
        Assert.True(registry.TryGet("echo", out var echoDefinition));
        registry.Register(new MspNativeBackedCommand(pwdDefinition, provider));
        registry.Register(new MspNativeBackedCommand(echoDefinition, provider));
        var context = new MspCommandContext(
            new InMemoryMspWorkspace(),
            registry,
            new AllowAllMspPolicy(),
            new InMemoryMspAuditSink());
        var host = new MspRuntimeCommandHost(new MspRuntime(context));

        var pwd = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "pwd",
            WorkingDirectory = "/documents",
            Actor = "dotnet-proxy",
            SessionId = "native-proxy"
        });
        var echo = await host.ExecuteAsync(new MspCommandRequest
        {
            CommandText = "echo ''",
            Actor = "dotnet-proxy",
            SessionId = "native-proxy"
        });

        Assert.Equal("/documents\n", pwd.Stdout);
        Assert.Equal("\n", echo.Stdout);
        Assert.Single(pwd.AuditRecords);
        Assert.Single(echo.AuditRecords);
        Assert.Equal(MspPolicyDecision.Allow, pwd.AuditRecords[0].Decision);
        Assert.Equal(MspPolicyDecision.Allow, echo.AuditRecords[0].Decision);
        Assert.Equal("pwd", pwd.AuditRecords[0].CommandName);
        Assert.Equal("echo", echo.AuditRecords[0].CommandName);
    }
}

internal sealed class ExplicitNativeDllFactAttribute : FactAttribute
{
    public const string EnvironmentVariable = "READOS_MSP_NATIVE_DLL";

    public ExplicitNativeDllFactAttribute()
    {
        if (!OperatingSystem.IsWindows())
        {
            Skip = "The real native MSP transport is Windows-only.";
            return;
        }

        if (string.IsNullOrWhiteSpace(Environment.GetEnvironmentVariable(EnvironmentVariable)))
        {
            Skip = $"Set {EnvironmentVariable} to an explicit built msp_core.dll path.";
        }
    }
}
