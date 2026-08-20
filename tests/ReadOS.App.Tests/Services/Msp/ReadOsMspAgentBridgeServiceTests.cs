using ReadOS.App.Models;
using ReadOS.App.Services.Msp;
using ReadOS.Msp.Hosting.Native;
using ReadOS.Msp.Hosting.Runtime;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsMspAgentBridgeServiceTests
{
    [Fact]
    public void BuildInstruction_names_supported_msp_command_surface()
    {
        var service = new ReadOsMspAgentBridgeService();

        var instruction = service.BuildInstruction();

        Assert.Contains("```msp", instruction);
        Assert.Contains("workspace info", instruction);
        Assert.Contains("pdf search current", instruction);
        Assert.Contains("workflow summary current", instruction);
        Assert.Contains("windows info", instruction);
        Assert.Contains("不要调用 PowerShell、cmd、bash", instruction);
    }

    [Fact]
    public void ExtractRequestedCommands_reads_code_blocks_and_xml_blocks()
    {
        var service = new ReadOsMspAgentBridgeService();
        const string content = """
            需要检查工作区：
            ```reados-msp-sh
            # comment
            $ workspace info
            msp> library list
            // ignored

            pdf inspect current
            ```

            <msp>
            pdf search current "risk"
            </msp>

            <reados-msp>
            artifact list /artifacts
            </reados-msp>
            """;

        var commands = service.ExtractRequestedCommands(content);

        Assert.Equal(new[]
        {
            "workspace info",
            "library list",
            "pdf inspect current",
            "pdf search current \"risk\"",
            "artifact list /artifacts"
        }, commands);
    }

    [Fact]
    public void ExtractRequestedCommands_ignores_unmarked_text()
    {
        var service = new ReadOsMspAgentBridgeService();

        var commands = service.ExtractRequestedCommands("Please run workspace info.");

        Assert.Empty(commands);
    }

    [Fact]
    public void BuildExecutionReport_includes_command_result_context()
    {
        var service = new ReadOsMspAgentBridgeService();
        var entries = new[]
        {
            new MspTranscriptEntry
            {
                CommandText = "pdf inspect current",
                ExitCode = 0,
                Decision = "Allow",
                Effects = "ReadWorkspace",
                ArtifactsSummary = "/artifacts/report.md",
                Stdout = "  ok  ",
                Stderr = "warning",
                DiagnosticsSummary = "diagnostic",
                RecoveryHint = "retry"
            }
        };

        var report = service.BuildExecutionReport(entries).Replace("\r\n", "\n");

        Assert.Contains("MSP command results:", report);
        Assert.Contains("$ pdf inspect current", report);
        Assert.Contains("exitCode: 0", report);
        Assert.Contains("decision: Allow", report);
        Assert.Contains("effects: ReadWorkspace", report);
        Assert.Contains("artifacts: /artifacts/report.md", report);
        Assert.Contains("stdout:\nok", report);
        Assert.Contains("stderr:\nwarning", report);
        Assert.Contains("diagnostics:\ndiagnostic", report);
        Assert.Contains("recovery: retry", report);
    }

    [Fact]
    public void BuildExecutionReport_trims_large_outputs()
    {
        var service = new ReadOsMspAgentBridgeService();
        var longStdout = new string('x', 5005);
        var longStderr = new string('y', 2005);
        var entries = new[]
        {
            new MspTranscriptEntry
            {
                CommandText = "cat /documents/doc/pages/1.txt",
                Stdout = longStdout,
                Stderr = longStderr
            }
        };

        var report = service.BuildExecutionReport(entries);

        Assert.Contains(new string('x', 5000) + "...", report);
        Assert.DoesNotContain(new string('x', 5001), report);
        Assert.Contains(new string('y', 2000) + "...", report);
        Assert.DoesNotContain(new string('y', 2001), report);
    }

    [Fact]
    public void ParseJsonDispatch_maps_shell_exec_to_hosting_request()
    {
        var service = new ReadOsMspAgentBridgeService();

        var dispatch = service.ParseJsonDispatch(
            "exec_command",
            "{\"cmd\":\"echo json\",\"workdir\":\"/documents\",\"yield_time_ms\":250,\"max_output_tokens\":64}");

        Assert.Equal("exec_command", dispatch.Operation);
        Assert.True(dispatch.IsExec);
        Assert.False(dispatch.IsStdin);
        Assert.NotNull(dispatch.ExecRequest);
        Assert.Null(dispatch.StdinRequest);
        Assert.Equal(MspExecSessionMode.Shell, dispatch.ExecRequest!.Mode);
        Assert.Equal("echo json", dispatch.ExecRequest.CommandText);
        Assert.Equal("/documents", dispatch.ExecRequest.WorkingDirectory);
        Assert.Equal(250, dispatch.ExecRequest.YieldTimeMs);
        Assert.Equal(64, dispatch.ExecRequest.MaxOutputTokens);
    }

    [Fact]
    public void ParseJsonDispatch_maps_process_exec_and_environment_to_hosting_request()
    {
        var service = new ReadOsMspAgentBridgeService();

        var dispatch = service.ParseJsonDispatch(
            "exec_command",
            "{\"mode\":\"process\",\"program\":\"C:\\\\tools\\\\runner.exe\",\"arguments\":[\"--flag\",\"value\"],\"workspaceRoot\":\"C:\\\\workspace\",\"environment\":{\"MSP_TEST_VALUE\":\"bounded\"}}");

        var request = Assert.IsType<MspExecSessionRequest>(dispatch.ExecRequest);
        Assert.Equal(MspExecSessionMode.Process, request.Mode);
        Assert.Equal(@"C:\tools\runner.exe", request.Program);
        Assert.Equal(new[] { "--flag", "value" }, request.Arguments);
        Assert.Equal(@"C:\workspace", request.WorkspaceRoot);
        Assert.Equal("bounded", request.Environment!["MSP_TEST_VALUE"]);
    }

    [Fact]
    public void ParseJsonDispatch_maps_stdin_poll_and_continuation_to_typed_requests()
    {
        var service = new ReadOsMspAgentBridgeService();

        var poll = service.ParseJsonDispatch(
            "write_stdin",
            "{\"session_id\":42,\"yield_time_ms\":500}");
        var continuation = service.ParseJsonDispatch(
            "write_stdin",
            "{\"sessionId\":42,\"chars\":\"next\\n\",\"maxOutputTokens\":32}");

        Assert.True(poll.IsStdin);
        Assert.Null(poll.ExecRequest);
        Assert.Equal(42UL, poll.StdinRequest!.SessionId);
        Assert.Null(poll.StdinRequest.Chars);
        Assert.True(poll.StdinRequest.IsPoll);
        Assert.Equal(500, poll.StdinRequest.YieldTimeMs);

        Assert.Equal(42UL, continuation.Stdin!.SessionId);
        Assert.Equal("next\n", continuation.Stdin.Chars);
        Assert.False(continuation.Stdin.IsPoll);
        Assert.Equal(32, continuation.Stdin.MaxOutputTokens);
    }

    [Fact]
    public void ParseJsonDispatch_rejects_unknown_duplicate_and_out_of_range_arguments()
    {
        var service = new ReadOsMspAgentBridgeService();

        var unknown = Assert.Throws<ArgumentException>(() =>
            service.ParseJsonDispatch(
                "exec_command",
                "{\"cmd\":\"echo\",\"secret\":\"private\"}"));
        Assert.Contains("unsupported", unknown.Message, StringComparison.OrdinalIgnoreCase);
        Assert.DoesNotContain("private", unknown.ToString(), StringComparison.Ordinal);

        var duplicate = Assert.Throws<ArgumentException>(() =>
            service.ParseJsonDispatch(
                "exec_command",
                "{\"cmd\":\"echo one\",\"command\":\"echo two\"}"));
        Assert.Contains("duplicate", duplicate.Message, StringComparison.OrdinalIgnoreCase);

        var negative = Assert.Throws<ArgumentException>(() =>
            service.ParseJsonDispatch(
                "write_stdin",
                "{\"session_id\":42,\"max_output_tokens\":-1}"));
        Assert.Contains("non-negative", negative.Message, StringComparison.OrdinalIgnoreCase);

        var oversizedChars = new string('x', MspNativeExecSessionLimits.MaximumWriteStdinChars + 1);
        var oversized = Assert.Throws<ArgumentException>(() =>
            service.ParseJsonDispatch(
                "write_stdin",
                "{\"session_id\":42,\"chars\":\"" + oversizedChars + "\"}"));
        Assert.Contains("maximum", oversized.Message, StringComparison.OrdinalIgnoreCase);
        Assert.DoesNotContain("xxx", oversized.ToString(), StringComparison.Ordinal);
    }

    [Fact]
    public void ParseJsonDispatch_rejects_non_object_json_and_unknown_operation()
    {
        var service = new ReadOsMspAgentBridgeService();

        var nonObject = Assert.Throws<ArgumentException>(() =>
            service.ParseJsonDispatch("exec_command", "[]"));
        Assert.Contains("JSON object", nonObject.Message, StringComparison.OrdinalIgnoreCase);

        var unknownOperation = Assert.Throws<ArgumentException>(() =>
            service.ParseJsonDispatch("other", "{}"));
        Assert.Contains("not supported", unknownOperation.Message, StringComparison.OrdinalIgnoreCase);
    }
}
