using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using ReadOS.Msp.Hosting.Native;

namespace ReadOS.Msp.Hosting.Tests.Native;

public sealed class MspNativeAdapterTests
{
    [Fact]
    public void Adapter_exposes_transport_abi_evidence_through_optional_provider_contract()
    {
        var runtimeInfo = new MspNativeRuntimeInfo(
            MspNativeAbiMode.LengthDelimitedV2,
            2,
            0,
            MspNativeContract.AbiV2ContractId,
            MspNativeContract.AbiV2RequiredCapabilities);
        var transport = new FakeTransport(
            (_, _) => CommandResult(),
            runtimeInfo);
        using IMspNativeAdapter adapter = new MspNativeAdapter(transport);

        var provider = Assert.IsAssignableFrom<IMspNativeRuntimeInfoProvider>(adapter);
        Assert.Same(runtimeInfo, provider.NativeRuntimeInfo);
    }

    [Fact]
    public void Execute_pwd_uses_versioned_utf8_request_and_authoritative_bytes()
    {
        var transport = new FakeTransport((operation, _) =>
            CommandResult(
                stdoutBytes: Encoding.UTF8.GetBytes("/documents\n"),
                commandText: "pwd",
                commandName: "pwd",
                actor: "测试者",
                sessionId: "session-1",
                workingDirectory: "/documents"));
        using var adapter = new MspNativeAdapter(transport);

        var result = adapter.Execute(new MspNativeCommandRequest
        {
            CommandText = "pwd",
            WorkingDirectory = "/documents",
            Actor = "测试者",
            SessionId = "session-1"
        });

        Assert.Equal(MspNativeOperation.Execute, transport.LastOperation);
        using var request = JsonDocument.Parse(transport.LastRequestJson);
        Assert.Equal(MspNativeContract.Version, request.RootElement.GetProperty("contractVersion").GetString());
        Assert.Equal("测试者", request.RootElement.GetProperty("actor").GetString());
        Assert.Equal("/documents\n", result.StdoutText);
        Assert.Equal(Encoding.UTF8.GetBytes("/documents\n"), result.StdoutBytes.ToArray());
        Assert.Single(result.AuditRecords);
    }

    [Fact]
    public void Execute_echo_preserves_empty_argument_and_ignores_text_projection()
    {
        var transport = new FakeTransport((_, _) =>
            CommandResult(
                stdoutBytes: Encoding.UTF8.GetBytes("\n"),
                commandText: "echo ''",
                commandName: "echo",
                arguments: [""]));
        using var adapter = new MspNativeAdapter(transport);

        var result = adapter.Execute(new MspNativeCommandRequest
        {
            CommandText = "echo ''"
        });

        Assert.Equal("\n", result.StdoutText);
        Assert.Equal([""], result.AuditRecords[0].Arguments);
    }

    [Fact]
    public void Execute_unknown_command_maps_native_diagnostic_without_reclassifying_exit()
    {
        var diagnostic = Diagnostic(
            "msp.command_not_found",
            "missing-command: command not found",
            "missing-command",
            "Use an enabled MSP command pack command.");
        var transport = new FakeTransport((_, _) =>
            CommandResult(
                exitCode: 127,
                stderrBytes: Encoding.UTF8.GetBytes("missing-command: command not found\n"),
                commandText: "missing-command",
                commandName: "missing-command",
                diagnostics: [diagnostic],
                policyKind: "notEvaluated"));
        using var adapter = new MspNativeAdapter(transport);

        var result = adapter.Execute(new MspNativeCommandRequest
        {
            CommandText = "missing-command"
        });

        Assert.Equal(127, result.ExitCode);
        Assert.False(result.Succeeded);
        var mapped = Assert.Single(result.Diagnostics).ToManagedDiagnostic();
        Assert.Equal("msp.command_not_found", mapped.Code);
        Assert.Equal("missing-command", mapped.Target);
        Assert.Equal("Use an enabled MSP command pack command.", mapped.RecoveryHint);
        Assert.Equal(MspNativePolicyDecisionKind.NotEvaluated, result.AuditRecords[0].PolicyDecision.Kind);
    }

    [Fact]
    public void Parse_preserves_explicit_empty_quoted_fragment()
    {
        var response = Json(new Dictionary<string, object?>
        {
            ["contractVersion"] = MspNativeContract.Version,
            ["succeeded"] = true,
            ["script"] = new Dictionary<string, object?>
            {
                ["rawInput"] = "echo ''",
                ["pipelines"] = new object[]
                {
                    new Dictionary<string, object?>
                    {
                        ["leadingOperator"] = null,
                        ["isNegated"] = false,
                        ["commands"] = new object[]
                        {
                            new Dictionary<string, object?>
                            {
                                ["commandName"] = "echo",
                                ["arguments"] = new[] { "" },
                                ["assignments"] = Array.Empty<object>(),
                                ["redirections"] = Array.Empty<object>(),
                                ["isAssignmentOnly"] = false,
                                ["rawInput"] = "echo ''",
                                ["commandNameWord"] = Word("echo", isQuoted: false),
                                ["argumentWords"] = new[]
                                {
                                    new Dictionary<string, object?>
                                    {
                                        ["parts"] = new[]
                                        {
                                            new Dictionary<string, object?>
                                            {
                                                ["text"] = "",
                                                ["isExpandable"] = true,
                                                ["isQuoted"] = true
                                            }
                                        },
                                        ["hasExplicitEmptyQuotedFragment"] = true
                                    }
                                }
                            }
                        },
                        ["pipeOperators"] = Array.Empty<object>()
                    }
                }
            },
            ["error"] = null
        });
        var transport = new FakeTransport((_, _) => response);
        using var adapter = new MspNativeAdapter(transport);

        var result = adapter.Parse(new MspNativeShellParseRequest
        {
            CommandText = "echo ''"
        });

        var command = Assert.Single(Assert.Single(result.Script!.Pipelines).Commands);
        Assert.Equal([""], command.Arguments);
        Assert.True(Assert.Single(command.ArgumentWords).HasExplicitEmptyQuotedFragment);
        Assert.Equal(MspNativeOperation.Parse, transport.LastOperation);
    }

    [Fact]
    public void Normalize_workspace_path_preserves_root_clamp_contract()
    {
        var transport = new FakeTransport((_, _) => Json(new Dictionary<string, object?>
        {
            ["contractVersion"] = MspNativeContract.Version,
            ["succeeded"] = true,
            ["virtualPath"] = "/reports/a.txt",
            ["error"] = null
        }));
        using var adapter = new MspNativeAdapter(transport);

        var result = adapter.NormalizeWorkspacePath(new MspNativeWorkspacePathRequest
        {
            Path = "../../reports/a.txt",
            CurrentDirectory = "/docs/current"
        });

        Assert.Equal("/reports/a.txt", result.VirtualPath);
        using var request = JsonDocument.Parse(transport.LastRequestJson);
        Assert.Equal("../../reports/a.txt", request.RootElement.GetProperty("path").GetString());
        Assert.Equal("/docs/current", request.RootElement.GetProperty("currentDirectory").GetString());
    }

    [Fact]
    public void Execute_preserves_non_utf8_stream_bytes()
    {
        var expected = new byte[] { 0x00, 0xff, (byte)'A', (byte)'\n' };
        var transport = new FakeTransport((_, _) =>
            CommandResult(
                stdoutBytes: expected,
                stdoutProjection: "\0�A\n",
                commandText: "binary-fixture",
                commandName: "binary-fixture"));
        using var adapter = new MspNativeAdapter(transport);

        var result = adapter.Execute(new MspNativeCommandRequest
        {
            CommandText = "binary-fixture"
        });

        Assert.Equal(expected, result.StdoutBytes.ToArray());
        Assert.Contains('�', result.StdoutText);
    }

    [Fact]
    public void Execute_rejects_compatibility_text_that_disagrees_with_authoritative_bytes()
    {
        var transport = new FakeTransport((_, _) =>
            CommandResult(
                stdoutBytes: Encoding.UTF8.GetBytes("trusted\n"),
                stdoutProjection: "forged\n"));
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Execute(new MspNativeCommandRequest { CommandText = "pwd" }));

        Assert.Equal(MspNativeFailureKind.InvalidResponse, exception.FailureKind);
    }

    [Fact]
    public void Execute_sends_workspace_root_only_in_request()
    {
        const string workspaceRoot = @"V:\private\reados-workspace";
        var transport = new FakeTransport((_, _) =>
            CommandResult(commandText: "ls /", commandName: "ls"));
        using var adapter = new MspNativeAdapter(transport);

        var result = adapter.Execute(new MspNativeCommandRequest
        {
            CommandText = "ls /",
            WorkspaceRoot = workspaceRoot
        });

        using var request = JsonDocument.Parse(transport.LastRequestJson);
        Assert.Equal(workspaceRoot, request.RootElement.GetProperty("workspaceRoot").GetString());
        Assert.DoesNotContain(workspaceRoot, result.StdoutText);
        Assert.DoesNotContain(workspaceRoot, result.StderrText);
        Assert.DoesNotContain(workspaceRoot, JsonSerializer.Serialize(result.Diagnostics));
        Assert.DoesNotContain(workspaceRoot, JsonSerializer.Serialize(result.AuditRecords));
    }

    [Fact]
    public void Execute_rejects_workspace_root_disclosure_and_clears_failure_text()
    {
        const string workspaceRoot = @"V:\private\reados-workspace";
        var transport = new FakeTransport((_, _) =>
            CommandResult(
                stdoutBytes: Encoding.UTF8.GetBytes(workspaceRoot + "\n"),
                commandText: "ls /",
                commandName: "ls"));
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Execute(new MspNativeCommandRequest
            {
                CommandText = "ls /",
                WorkspaceRoot = workspaceRoot
            }));

        Assert.Equal(MspNativeFailureKind.HostPathDisclosure, exception.FailureKind);
        Assert.Equal("msp.native.host_path_disclosure", exception.Diagnostic.Code);
        Assert.False(exception.ToString().Contains(workspaceRoot, StringComparison.OrdinalIgnoreCase));
    }

    [Theory]
    [InlineData("utf16")]
    [InlineData("verbatim")]
    [InlineData("file-uri")]
    [InlineData("percent-encoded")]
    [InlineData("slash-root")]
    [InlineData("verbatim-root")]
    [InlineData("trailing-root")]
    public void Execute_rejects_encoded_workspace_root_variants(string variant)
    {
        var workspaceRoot = variant switch
        {
            "slash-root" => "V:/private root/reados",
            "verbatim-root" => @"\\?\V:\private root\reados",
            "trailing-root" => @"V:\private root\reados\",
            _ => @"V:\private root\reados"
        };
        var disclosed = variant switch
        {
            "verbatim" => @"\\?\V:\private root\reados",
            "file-uri" => "file:///V:/private%20root/reados",
            "percent-encoded" => Uri.EscapeDataString(workspaceRoot),
            "slash-root" or "verbatim-root" or "trailing-root" => @"V:\private root\reados",
            _ => workspaceRoot
        };
        var bytes = variant == "utf16"
            ? Encoding.Unicode.GetBytes(disclosed)
            : Encoding.UTF8.GetBytes(disclosed);
        var transport = new FakeTransport((_, _) =>
            CommandResult(
                stdoutBytes: bytes,
                commandText: "cat /fixture.bin",
                commandName: "cat",
                arguments: ["/fixture.bin"]));
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Execute(new MspNativeCommandRequest
            {
                CommandText = "cat /fixture.bin",
                WorkspaceRoot = workspaceRoot
            }));

        Assert.Equal(MspNativeFailureKind.HostPathDisclosure, exception.FailureKind);
        Assert.False(exception.ToString().Contains(workspaceRoot, StringComparison.OrdinalIgnoreCase));
    }

    [Theory]
    [InlineData("workspace")]
    [InlineData("..\\workspace")]
    [InlineData("V:workspace")]
    public void Execute_rejects_non_fully_qualified_workspace_root(string workspaceRoot)
    {
        var transport = new FakeTransport((_, _) => CommandResult());
        using var adapter = new MspNativeAdapter(transport);

        Assert.Throws<ArgumentException>(() =>
            adapter.Execute(new MspNativeCommandRequest
            {
                CommandText = "ls /",
                WorkspaceRoot = workspaceRoot
            }));
        Assert.Null(transport.LastOperation);
    }

    [Theory]
    [InlineData("{not-json", MspNativeFailureKind.InvalidJson)]
    [InlineData("\u00ff", MspNativeFailureKind.InvalidJson)]
    public void Invalid_response_text_fails_closed_without_echoing_payload(
        string response,
        MspNativeFailureKind expectedFailure)
    {
        const string secret = "Secret-Value-Should-Not-Leak";
        var transport = new FakeTransport((_, _) => Encoding.UTF8.GetBytes(response + secret));
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Execute(new MspNativeCommandRequest
            {
                CommandText = "pwd",
                Environment = new Dictionary<string, string> { ["TOKEN"] = secret }
            }));

        Assert.Equal(expectedFailure, exception.FailureKind);
        Assert.False(exception.ToString().Contains(secret, StringComparison.Ordinal));
    }

    [Fact]
    public void Invalid_utf8_response_fails_closed()
    {
        var transport = new FakeTransport((_, _) => [0xff, 0x00]);
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Execute(new MspNativeCommandRequest { CommandText = "pwd" }));

        Assert.Equal(MspNativeFailureKind.InvalidUtf8, exception.FailureKind);
    }

    [Fact]
    public void Empty_response_fails_as_invalid_json()
    {
        var transport = new FakeTransport((_, _) => Array.Empty<byte>());
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Execute(new MspNativeCommandRequest { CommandText = "pwd" }));

        Assert.Equal(MspNativeFailureKind.InvalidJson, exception.FailureKind);
    }

    [Fact]
    public void Decoded_stream_limit_is_enforced_separately_from_base64_validity()
    {
        var transport = new FakeTransport((_, _) =>
            CommandResult(stdoutBytes: [1, 2, 3, 4, 5]));
        using var adapter = new MspNativeAdapter(
            transport,
            new MspNativeAdapterLimits
            {
                MaximumRequestBytes = 1024,
                MaximumResponseBytes = 4096,
                MaximumDecodedStreamBytes = 4
            });

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Execute(new MspNativeCommandRequest { CommandText = "pwd" }));

        Assert.Equal(MspNativeFailureKind.ResponseTooLarge, exception.FailureKind);
    }

    [Fact]
    public void Unsupported_contract_version_fails_closed()
    {
        var transport = new FakeTransport((_, _) =>
            CommandResult(contractVersion: "reados-msp-native/999"));
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Execute(new MspNativeCommandRequest { CommandText = "pwd" }));

        Assert.Equal(MspNativeFailureKind.UnsupportedContractVersion, exception.FailureKind);
        Assert.DoesNotContain("999", exception.Message);
    }

    [Fact]
    public void Invalid_authoritative_base64_fails_closed_even_with_valid_text_projection()
    {
        var response = CommandResult(stdoutProjection: "looks valid");
        using var document = JsonDocument.Parse(response);
        var values = document.RootElement.EnumerateObject()
            .ToDictionary(property => property.Name, property => (object?)property.Value.Clone());
        values["stdoutBytesBase64"] = "%%%not-base64%%%";
        var transport = new FakeTransport((_, _) => Json(values));
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Execute(new MspNativeCommandRequest { CommandText = "pwd" }));

        Assert.Equal(MspNativeFailureKind.InvalidBase64, exception.FailureKind);
    }

    [Fact]
    public void Missing_exit_code_does_not_silently_become_success()
    {
        var response = JsonNode.Parse(CommandResult())!.AsObject();
        response.Remove("exitCode");
        var transport = new FakeTransport((_, _) => Encoding.UTF8.GetBytes(response.ToJsonString()));
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Execute(new MspNativeCommandRequest { CommandText = "pwd" }));

        Assert.Equal(MspNativeFailureKind.InvalidResponse, exception.FailureKind);
    }

    [Fact]
    public void Missing_policy_kind_does_not_silently_become_allow()
    {
        var response = JsonNode.Parse(CommandResult())!.AsObject();
        response["auditRecords"]![0]!["policyDecision"]!.AsObject().Remove("kind");
        var transport = new FakeTransport((_, _) => Encoding.UTF8.GetBytes(response.ToJsonString()));
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Execute(new MspNativeCommandRequest { CommandText = "pwd" }));

        Assert.Equal(MspNativeFailureKind.InvalidJson, exception.FailureKind);
    }

    [Theory]
    [InlineData("exitCode", "7")]
    [InlineData("actor", "different-actor")]
    [InlineData("sessionId", "different-session")]
    [InlineData("workingDirectory", "/different")]
    public void Contradictory_audit_evidence_is_rejected(string propertyName, string value)
    {
        var response = JsonNode.Parse(CommandResult())!.AsObject();
        var audit = response["auditRecords"]![0]!.AsObject();
        audit[propertyName] = propertyName == "exitCode"
            ? JsonValue.Create(int.Parse(value, System.Globalization.CultureInfo.InvariantCulture))
            : JsonValue.Create(value);
        var transport = new FakeTransport((_, _) => Encoding.UTF8.GetBytes(response.ToJsonString()));
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Execute(new MspNativeCommandRequest { CommandText = "pwd" }));

        Assert.Equal(MspNativeFailureKind.InvalidResponse, exception.FailureKind);
    }

    [Theory]
    [InlineData("documents")]
    [InlineData("/documents/")]
    [InlineData("/documents//reports")]
    [InlineData("/documents/../reports")]
    public void Execute_requires_normalized_virtual_working_directory(string workingDirectory)
    {
        var transport = new FakeTransport((_, _) => CommandResult());
        using var adapter = new MspNativeAdapter(transport);

        Assert.Throws<ArgumentException>(() =>
            adapter.Execute(new MspNativeCommandRequest
            {
                CommandText = "pwd",
                WorkingDirectory = workingDirectory
            }));
        Assert.Null(transport.LastOperation);
    }

    [Theory]
    [InlineData("")]
    [InlineData("   ")]
    [InlineData("actor\nforged")]
    public void Execute_rejects_unattributable_actor_identity(string actor)
    {
        var transport = new FakeTransport((_, _) => CommandResult());
        using var adapter = new MspNativeAdapter(transport);

        Assert.Throws<ArgumentException>(() =>
            adapter.Execute(new MspNativeCommandRequest
            {
                CommandText = "pwd",
                Actor = actor
            }));
        Assert.Null(transport.LastOperation);
    }

    [Fact]
    public void Null_transport_response_fails_closed()
    {
        var transport = new FakeTransport((_, _) => null!);
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Execute(new MspNativeCommandRequest { CommandText = "pwd" }));

        Assert.Equal(MspNativeFailureKind.NullResponse, exception.FailureKind);
    }

    [Fact]
    public void Transport_exception_is_sanitized_without_inner_exception()
    {
        const string secret = @"secret V:\private\workspace";
        var transport = new ThrowingTransport(new InvalidOperationException(secret));
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Execute(new MspNativeCommandRequest
            {
                CommandText = "pwd",
                WorkspaceRoot = @"V:\private\workspace"
            }));

        Assert.Equal(MspNativeFailureKind.InvocationFailed, exception.FailureKind);
        Assert.Null(exception.InnerException);
        Assert.False(exception.ToString().Contains(secret, StringComparison.OrdinalIgnoreCase));
    }

    [Fact]
    public void Dispose_disposes_transport_and_prevents_future_invocation()
    {
        var transport = new FakeTransport((_, _) => CommandResult());
        var adapter = new MspNativeAdapter(transport);

        adapter.Dispose();

        Assert.True(transport.IsDisposed);
        Assert.Throws<ObjectDisposedException>(() =>
            adapter.Execute(new MspNativeCommandRequest { CommandText = "pwd" }));
    }

    [Fact]
    public void Adapter_rejects_unbounded_native_size_limits()
    {
        var transport = new FakeTransport((_, _) => CommandResult());

        Assert.Throws<ArgumentOutOfRangeException>(() =>
            new MspNativeAdapter(
                transport,
                new MspNativeAdapterLimits
                {
                    MaximumRequestBytes = 1024,
                    MaximumResponseBytes = int.MaxValue,
                    MaximumDecodedStreamBytes = 1024
                }));
        Assert.Null(transport.LastOperation);
    }

    [Fact]
    public void Command_result_evidence_collections_are_frozen()
    {
        var diagnostic = Diagnostic("msp.fixture", "fixture");
        var transport = new FakeTransport((_, _) => CommandResult(diagnostics: [diagnostic]));
        using var adapter = new MspNativeAdapter(transport);

        var result = adapter.Execute(new MspNativeCommandRequest { CommandText = "pwd" });

        Assert.Throws<NotSupportedException>(() =>
            ((IList<MspNativeAuditRecord>)result.AuditRecords).Clear());
        Assert.Throws<NotSupportedException>(() =>
            ((IList<MspNativeDiagnostic>)result.Diagnostics).Clear());
        Assert.Throws<NotSupportedException>(() =>
            ((IList<string>)result.AuditRecords[0].Arguments).Clear());
        Assert.Throws<NotSupportedException>(() =>
            ((IList<MspNativeDiagnostic>)result.AuditRecords[0].Diagnostics).Clear());
    }

    [Fact]
    public void Parsed_ast_collections_are_frozen()
    {
        var response = Json(new Dictionary<string, object?>
        {
            ["contractVersion"] = MspNativeContract.Version,
            ["succeeded"] = true,
            ["script"] = new Dictionary<string, object?>
            {
                ["rawInput"] = "pwd",
                ["pipelines"] = new object[]
                {
                    new Dictionary<string, object?>
                    {
                        ["leadingOperator"] = null,
                        ["isNegated"] = false,
                        ["commands"] = new object[]
                        {
                            new Dictionary<string, object?>
                            {
                                ["commandName"] = "pwd",
                                ["arguments"] = Array.Empty<string>(),
                                ["assignments"] = Array.Empty<object>(),
                                ["redirections"] = Array.Empty<object>(),
                                ["isAssignmentOnly"] = false,
                                ["rawInput"] = "pwd",
                                ["commandNameWord"] = Word("pwd", isQuoted: false),
                                ["argumentWords"] = Array.Empty<object>()
                            }
                        },
                        ["pipeOperators"] = Array.Empty<object>()
                    }
                }
            }
        });
        var transport = new FakeTransport((_, _) => response);
        using var adapter = new MspNativeAdapter(transport);

        var result = adapter.Parse(new MspNativeShellParseRequest { CommandText = "pwd" });
        var script = result.Script!;

        Assert.Throws<NotSupportedException>(() =>
            ((IList<MspNativeParsedCommandPipeline>)script.Pipelines).Clear());
        Assert.Throws<NotSupportedException>(() =>
            ((IList<MspNativeParsedCommandLine>)script.Pipelines[0].Commands).Clear());
    }

    [Fact]
    public void Parse_rejects_ast_for_a_different_command()
    {
        var response = Json(new Dictionary<string, object?>
        {
            ["contractVersion"] = MspNativeContract.Version,
            ["succeeded"] = true,
            ["script"] = new Dictionary<string, object?>
            {
                ["rawInput"] = "echo forged",
                ["pipelines"] = new object[]
                {
                    new Dictionary<string, object?>
                    {
                        ["leadingOperator"] = null,
                        ["isNegated"] = false,
                        ["commands"] = new object[]
                        {
                            new Dictionary<string, object?>
                            {
                                ["commandName"] = "echo",
                                ["arguments"] = new[] { "forged" },
                                ["assignments"] = Array.Empty<object>(),
                                ["redirections"] = Array.Empty<object>(),
                                ["isAssignmentOnly"] = false,
                                ["rawInput"] = "echo forged",
                                ["commandNameWord"] = Word("echo", isQuoted: false),
                                ["argumentWords"] = new[] { Word("forged", isQuoted: false) }
                            }
                        },
                        ["pipeOperators"] = Array.Empty<object>()
                    }
                }
            }
        });
        var transport = new FakeTransport((_, _) => response);
        using var adapter = new MspNativeAdapter(transport);

        var exception = Assert.Throws<MspNativeAdapterException>(() =>
            adapter.Parse(new MspNativeShellParseRequest { CommandText = "pwd" }));

        Assert.Equal(MspNativeFailureKind.InvalidResponse, exception.FailureKind);
    }

    private static Dictionary<string, object?> Diagnostic(
        string code,
        string message,
        string? target = null,
        string? recoveryHint = null)
    {
        return new Dictionary<string, object?>
        {
            ["severity"] = "error",
            ["code"] = code,
            ["message"] = message,
            ["target"] = target,
            ["recoveryHint"] = recoveryHint
        };
    }

    private static Dictionary<string, object?> Word(string text, bool isQuoted)
    {
        return new Dictionary<string, object?>
        {
            ["parts"] = new[]
            {
                new Dictionary<string, object?>
                {
                    ["text"] = text,
                    ["isExpandable"] = true,
                    ["isQuoted"] = isQuoted
                }
            },
            ["hasExplicitEmptyQuotedFragment"] = false
        };
    }

    private static byte[] CommandResult(
        int exitCode = 0,
        byte[]? stdoutBytes = null,
        byte[]? stderrBytes = null,
        string? stdoutProjection = null,
        string? stderrProjection = null,
        string commandText = "pwd",
        string? commandName = "pwd",
        IReadOnlyList<string>? arguments = null,
        IReadOnlyList<Dictionary<string, object?>>? diagnostics = null,
        string policyKind = "allow",
        string contractVersion = MspNativeContract.Version,
        string actor = "agent",
        string sessionId = "default",
        string workingDirectory = "/")
    {
        stdoutBytes ??= Array.Empty<byte>();
        stderrBytes ??= Array.Empty<byte>();
        stdoutProjection ??= Encoding.UTF8.GetString(stdoutBytes);
        stderrProjection ??= Encoding.UTF8.GetString(stderrBytes);
        diagnostics ??= Array.Empty<Dictionary<string, object?>>();
        return Json(new Dictionary<string, object?>
        {
            ["contractVersion"] = contractVersion,
            ["stdout"] = stdoutProjection,
            ["stderr"] = stderrProjection,
            ["stdoutBytesBase64"] = Convert.ToBase64String(stdoutBytes),
            ["stderrBytesBase64"] = Convert.ToBase64String(stderrBytes),
            ["exitCode"] = exitCode,
            ["stateChange"] = null,
            ["auditRecords"] = new object[]
            {
                new Dictionary<string, object?>
                {
                    ["runId"] = "1-1",
                    ["commandLine"] = commandText,
                    ["commandName"] = commandName,
                    ["arguments"] = arguments ?? Array.Empty<string>(),
                    ["exitCode"] = exitCode,
                    ["startedAtUnixMs"] = 1UL,
                    ["endedAtUnixMs"] = 2UL,
                    ["actor"] = actor,
                    ["sessionId"] = sessionId,
                    ["workingDirectory"] = workingDirectory,
                    ["policyDecision"] = new Dictionary<string, object?>
                    {
                        ["kind"] = policyKind,
                        ["reason"] = null,
                        ["prompt"] = null
                    },
                    ["diagnostics"] = diagnostics
                }
            },
            ["diagnostics"] = diagnostics
        });
    }

    private static byte[] Json(object value)
    {
        return JsonSerializer.SerializeToUtf8Bytes(value);
    }

    private sealed class FakeTransport(
        Func<MspNativeOperation, ReadOnlyMemory<byte>, byte[]> handler,
        MspNativeRuntimeInfo? runtimeInfo = null) :
        IMspNativeTransport,
        IMspNativeRuntimeInfoProvider
    {
        public MspNativeRuntimeInfo NativeRuntimeInfo { get; } =
            runtimeInfo ?? MspNativeRuntimeInfo.Unknown;

        public MspNativeOperation? LastOperation { get; private set; }

        public byte[] LastRequestJson { get; private set; } = Array.Empty<byte>();

        public bool IsDisposed { get; private set; }

        public byte[] Invoke(MspNativeOperation operation, ReadOnlyMemory<byte> requestJsonUtf8)
        {
            LastOperation = operation;
            LastRequestJson = requestJsonUtf8.ToArray();
            var response = handler(operation, requestJsonUtf8);
            return response?.ToArray()!;
        }

        public void Dispose()
        {
            IsDisposed = true;
        }
    }

    private sealed class ThrowingTransport(Exception exception) : IMspNativeTransport
    {
        public byte[] Invoke(MspNativeOperation operation, ReadOnlyMemory<byte> requestJsonUtf8)
        {
            throw exception;
        }

        public void Dispose()
        {
        }
    }
}
