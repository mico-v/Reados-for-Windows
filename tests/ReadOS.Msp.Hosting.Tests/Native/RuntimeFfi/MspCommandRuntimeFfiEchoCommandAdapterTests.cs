using System.Text;
using System.Text.Json;
using ReadOS.Msp.Audit;
using ReadOS.Msp.Commands;
using ReadOS.Msp.Hosting.Native.RuntimeFfi;
using ReadOS.Msp.Models;
using ReadOS.Msp.Policy;
using ReadOS.Msp.Runtime;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Hosting.Tests.Native.RuntimeFfi;

public sealed partial class MspCommandRuntimeFfiAdapterTests
{
    [Theory]
    [InlineData("echo ''", "\n")]
    [InlineData("echo one two", "one two\n")]
    [InlineData("echo -n bytes", "bytes")]
    public async Task Echo_adapter_preserves_managed_parser_shapes_and_native_echo_bytes(
        string commandText,
        string expectedStdout)
    {
        using var state = new FakeNativeState
        {
            Stdout = Encoding.UTF8.GetBytes(expectedStdout)
        };
        using var library = MspCommandRuntimeFfiLibrary.CreateForTests(state.Library, state.Exports);
        var adapter = new MspCommandRuntimeFfiEchoCommandAdapter(library);
        var context = CreateContext(commandText);
        var parsed = ReadOS.Msp.Parsing.MspCommandLineParser.Parse(commandText);

        var managed = await new EchoCommand().ExecuteAsync(context, parsed.Arguments);
        var native = await adapter.ExecuteAsync(context, parsed.Arguments);

        Assert.Equal(managed.ExitCode, native.ExitCode);
        Assert.Equal(expectedStdout, native.Stdout);
        Assert.DoesNotContain(native.AuditRecords, record => record.CommandName == "echo");
        Assert.Equal(1, state.ExecuteCalls);

        using var request = JsonDocument.Parse(state.LastRequest);
        Assert.Equal(commandText, request.RootElement.GetProperty("command").GetString());
        Assert.Equal("/workspace", request.RootElement.GetProperty("cwd").GetString());
        Assert.DoesNotContain("actor", request.RootElement.EnumerateObject().Select(property => property.Name));
        Assert.DoesNotContain("environment", request.RootElement.EnumerateObject().Select(property => property.Name));
        Assert.DoesNotContain("policy", request.RootElement.EnumerateObject().Select(property => property.Name));
        Assert.DoesNotContain("audit", request.RootElement.EnumerateObject().Select(property => property.Name));
    }

    [Theory]
    [InlineData("Echo hello")]
    [InlineData("echo a | cat")]
    [InlineData("echo a > output.txt")]
    [InlineData("echo $HOME")]
    [InlineData("echo C:\\Users\\agent\\secret.txt")]
    public async Task Echo_adapter_rejects_unsupported_shapes_before_native_call(string commandText)
    {
        using var state = new FakeNativeState();
        using var library = MspCommandRuntimeFfiLibrary.CreateForTests(state.Library, state.Exports);
        var adapter = new MspCommandRuntimeFfiEchoCommandAdapter(library);
        var result = await adapter.ExecuteAsync(
            CreateContext(commandText),
            Array.Empty<string>());

        Assert.Equal(2, result.ExitCode);
        Assert.Equal(MspCommandRuntimeFfiEchoCommandAdapter.InvalidCommandDiagnosticCode, Assert.Single(result.Diagnostics).Code);
        Assert.Equal(0, state.ExecuteCalls);
    }

    [Fact]
    public async Task Echo_adapter_returns_typed_unavailable_result_without_library()
    {
        var adapter = new MspCommandRuntimeFfiEchoCommandAdapter(null);
        var result = await adapter.ExecuteAsync(
            CreateContext("echo available"),
            ["available"]);

        Assert.Equal(MspCommandRuntimeFfiEchoCommandAdapter.AdapterUnavailableDiagnosticCode, Assert.Single(result.Diagnostics).Code);
        Assert.Equal(1, result.ExitCode);
    }

    [Fact]
    public async Task Runtime_policy_denial_does_not_call_native_ffi()
    {
        using var state = new FakeNativeState
        {
            Stdout = Encoding.UTF8.GetBytes("must-not-run\n")
        };
        using var library = MspCommandRuntimeFfiLibrary.CreateForTests(state.Library, state.Exports);
        var audit = new InMemoryMspAuditSink();
        var registry = new MspCommandRegistry()
            .Register(new MspCommandRuntimeFfiEchoCommandAdapter(library));
        var runtime = new MspRuntime(new MspCommandContext(
            new InMemoryMspWorkspace(),
            registry,
            new DenyPolicy(),
            audit));

        var result = await runtime.ExecuteAsync(new MspCommandRequest
        {
            Actor = "denied-agent",
            CommandText = "echo must-not-run"
        });

        Assert.False(result.Succeeded);
        Assert.Equal(0, state.ExecuteCalls);
        Assert.Equal(MspPolicyDecision.Deny, Assert.Single(result.AuditRecords).Decision);
        Assert.Single(audit.Records);
    }

    [Fact]
    public async Task Runtime_owns_exactly_one_outer_audit_for_opt_in_echo_adapter()
    {
        using var state = new FakeNativeState
        {
            Stdout = Encoding.UTF8.GetBytes("audited\n")
        };
        using var library = MspCommandRuntimeFfiLibrary.CreateForTests(state.Library, state.Exports);
        var audit = new InMemoryMspAuditSink();
        var registry = new MspCommandRegistry()
            .Register(new MspCommandRuntimeFfiEchoCommandAdapter(library));
        var runtime = new MspRuntime(new MspCommandContext(
            new InMemoryMspWorkspace(),
            registry,
            new AllowAllMspPolicy(),
            audit));

        var result = await runtime.ExecuteAsync(new MspCommandRequest
        {
            Actor = "outer-owner",
            CommandText = "echo audited"
        });

        Assert.True(result.Succeeded, result.Stderr);
        Assert.DoesNotContain(result.AuditRecords, record => record.CommandName != "echo");
        Assert.Single(result.AuditRecords);
        Assert.Single(audit.Records);
        Assert.Equal("outer-owner", Assert.Single(audit.Records).Actor);
    }

    private static MspCommandContext CreateContext(string commandText)
    {
        return new MspCommandContext(
            new InMemoryMspWorkspace(),
            MspRuntime.CreateDefaultRegistry(),
            new AllowAllMspPolicy(),
            new InMemoryMspAuditSink(),
            workingDirectory: "C:\\host\\root",
            invocation: new MspCommandInvocation
            {
                Actor = "secret-actor",
                CommandText = commandText,
                CommandName = "echo"
            });
    }

    private sealed class DenyPolicy : IMspPolicy
    {
        public ValueTask<MspPolicyDecision> AuthorizeAsync(
            MspPolicyRequest request,
            CancellationToken cancellationToken = default)
        {
            return ValueTask.FromResult(MspPolicyDecision.Deny);
        }
    }
}
