using ReadOS.App.Models;
using ReadOS.App.Services.Msp;

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
}
