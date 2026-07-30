using ReadOS.Msp.Hosting.Native;

namespace ReadOS.Msp.Hosting.Tests.Native;

public sealed class MspNativeCommandRouteClassifierTests
{
    private readonly MspNativeCommandRouteClassifier classifier = new();

    [Fact]
    public void Classify_accepts_one_canonical_simple_command_with_empty_quoted_argument()
    {
        var command = SimpleCommand("echo") with
        {
            Arguments = [""],
            ArgumentWords = [new MspNativeParsedWord
            {
                Parts = [new MspNativeParsedWordPart
                {
                    Text = "",
                    IsExpandable = true,
                    IsQuoted = true
                }],
                HasExplicitEmptyQuotedFragment = true
            }]
        };

        var decision = classifier.Classify("echo", "fixture", [""], Success(command));

        Assert.True(decision.ShouldExecuteNative);
        Assert.Equal(MspNativeCommandRouteDecisionKind.ExecuteNative, decision.Kind);
        Assert.Empty(decision.DiagnosticCode);
    }

    [Fact]
    public void Classify_rejects_native_parse_failure_as_parser_disagreement()
    {
        var decision = classifier.Classify("echo", "echo", [], new MspNativeShellParseResult
        {
            ContractVersion = MspNativeContract.Version,
            Succeeded = false,
            Error = new MspNativeShellParseError
            {
                Kind = MspNativeShellParseErrorKind.Syntax,
                ExitCode = 2,
                Message = "syntax error"
            }
        });

        Assert.False(decision.ShouldExecuteNative);
        Assert.Equal(MspNativeCommandRouteDecisionKind.ParserDisagreement, decision.Kind);
        Assert.Equal("msp.native.route.parser_disagreement", decision.DiagnosticCode);
    }

    [Fact]
    public void Classify_rejects_ast_for_different_raw_command()
    {
        var decision = classifier.Classify(
            "echo",
            "echo expected",
            [],
            Success(SimpleCommand("echo")));

        Assert.Equal(MspNativeCommandRouteDecisionKind.ParserDisagreement, decision.Kind);
    }

    [Fact]
    public void Classify_rejects_command_name_mismatch()
    {
        var decision = classifier.Classify(
            "echo",
            "fixture",
            [],
            Success(SimpleCommand("ECHO")));

        Assert.Equal(MspNativeCommandRouteDecisionKind.CommandMismatch, decision.Kind);
        Assert.Equal("msp.native.route.command_mismatch", decision.DiagnosticCode);
    }

    [Fact]
    public void Classify_rejects_multiple_pipelines_and_list_operator()
    {
        var first = SimplePipeline(SimpleCommand("echo"));
        var second = SimplePipeline(SimpleCommand("pwd")) with
        {
            LeadingOperator = MspNativeParsedListOperator.And
        };
        var result = Success(first, second);

        AssertUnsupported(result);
    }

    [Fact]
    public void Classify_rejects_managed_native_argument_disagreement_including_empty_quotes()
    {
        var command = SimpleCommand("echo") with
        {
            Arguments = [""],
            ArgumentWords = [new MspNativeParsedWord
            {
                Parts = [new MspNativeParsedWordPart
                {
                    Text = "",
                    IsExpandable = true,
                    IsQuoted = true
                }],
                HasExplicitEmptyQuotedFragment = true
            }]
        };

        var decision = classifier.Classify("echo", "fixture", [], Success(command));

        Assert.Equal(MspNativeCommandRouteDecisionKind.ParserDisagreement, decision.Kind);
        Assert.Equal("msp.native.route.parser_disagreement", decision.DiagnosticCode);
    }

    [Theory]
    [InlineData("alpha\fbeta", "alpha", "beta")]
    [InlineData("alpha\u2003beta", "alpha", "beta")]
    public void Classify_rejects_whitespace_tokenization_disagreement(
        string nativeArgument,
        string managedFirst,
        string managedSecond)
    {
        var command = SimpleCommand("echo") with
        {
            Arguments = [nativeArgument],
            ArgumentWords = [Word(nativeArgument)]
        };

        var decision = classifier.Classify(
            "echo",
            "fixture",
            [managedFirst, managedSecond],
            Success(command));

        Assert.Equal(MspNativeCommandRouteDecisionKind.ParserDisagreement, decision.Kind);
    }

    [Fact]
    public void Classify_rejects_pipe()
    {
        var pipeline = SimplePipeline(SimpleCommand("echo")) with
        {
            Commands = [SimpleCommand("echo"), SimpleCommand("pwd")],
            PipeOperators = [MspNativeParsedPipeOperator.Stdout]
        };

        AssertUnsupported(Success(pipeline));
    }

    [Fact]
    public void Classify_rejects_negation()
    {
        var pipeline = SimplePipeline(SimpleCommand("echo")) with { IsNegated = true };

        AssertUnsupported(Success(pipeline));
    }

    [Fact]
    public void Classify_rejects_redirection()
    {
        var target = Word("output.txt");
        var command = SimpleCommand("echo") with
        {
            Redirections = [new MspNativeParsedRedirection
            {
                Operation = MspNativeParsedRedirectionOperator.Output,
                Target = "output.txt",
                TargetWord = target
            }]
        };

        AssertUnsupported(Success(command));
    }

    [Fact]
    public void Classify_rejects_assignment_and_assignment_only_forms()
    {
        var assignedCommand = SimpleCommand("echo") with
        {
            Assignments = [new MspNativeParsedAssignment
            {
                Name = "TOKEN",
                Value = "secret"
            }]
        };
        var assignmentOnly = SimpleCommand(string.Empty) with
        {
            CommandNameWord = null,
            IsAssignmentOnly = true,
            Assignments = [new MspNativeParsedAssignment
            {
                Name = "TOKEN",
                Value = "secret"
            }]
        };

        AssertUnsupported(Success(assignedCommand));
        AssertUnsupported(Success(assignmentOnly));
    }

    [Fact]
    public void Classify_rejects_leading_operator_even_for_one_pipeline()
    {
        var pipeline = SimplePipeline(SimpleCommand("echo")) with
        {
            LeadingOperator = MspNativeParsedListOperator.Semicolon
        };

        AssertUnsupported(Success(pipeline));
    }

    [Fact]
    public void Classify_accepts_escaped_ampersand_literal()
    {
        var command = SimpleCommand("echo") with
        {
            Arguments = ["&"],
            ArgumentWords = [new MspNativeParsedWord
            {
                Parts = [new MspNativeParsedWordPart
                {
                    Text = "&",
                    IsExpandable = false,
                    IsQuoted = false
                }],
                HasExplicitEmptyQuotedFragment = false
            }]
        };

        var decision = classifier.Classify("echo", "fixture", ["&"], Success(command));

        Assert.True(decision.ShouldExecuteNative);
    }

    [Fact]
    public void Classify_rejects_raw_newline_even_when_flattened_ast_looks_simple()
    {
        var result = Success(SimpleCommand("echo") with
        {
            Arguments = ["x", "pwd"],
            ArgumentWords = [Word("x"), Word("pwd")]
        });

        var decision = classifier.Classify(
            "echo",
            "echo x\npwd",
            ["x", "pwd"],
            result with
            {
                Script = result.Script! with { RawInput = "echo x\npwd" }
            });

        Assert.Equal(MspNativeCommandRouteDecisionKind.UnsupportedShellForm, decision.Kind);
    }

    private void AssertUnsupported(MspNativeShellParseResult result)
    {
        var managedArguments = result.Script?.Pipelines
            .FirstOrDefault()?.Commands.FirstOrDefault()?.Arguments ?? [];
        var decision = classifier.Classify("echo", "fixture", managedArguments, result);

        Assert.False(decision.ShouldExecuteNative);
        Assert.Equal(MspNativeCommandRouteDecisionKind.UnsupportedShellForm, decision.Kind);
        Assert.Equal("msp.native.route.unsupported_shell_form", decision.DiagnosticCode);
    }

    private static MspNativeShellParseResult Success(MspNativeParsedCommandLine command)
    {
        return Success(SimplePipeline(command));
    }

    private static MspNativeShellParseResult Success(params MspNativeParsedCommandPipeline[] pipelines)
    {
        return new MspNativeShellParseResult
        {
            ContractVersion = MspNativeContract.Version,
            Succeeded = true,
            Script = new MspNativeParsedShellScript
            {
                RawInput = "fixture",
                Pipelines = pipelines
            }
        };
    }

    private static MspNativeParsedCommandPipeline SimplePipeline(
        MspNativeParsedCommandLine command)
    {
        return new MspNativeParsedCommandPipeline
        {
            IsNegated = false,
            Commands = [command],
            PipeOperators = []
        };
    }

    private static MspNativeParsedCommandLine SimpleCommand(string name)
    {
        return new MspNativeParsedCommandLine
        {
            CommandName = name,
            Arguments = [],
            Assignments = [],
            Redirections = [],
            IsAssignmentOnly = false,
            RawInput = name,
            CommandNameWord = Word(name),
            ArgumentWords = []
        };
    }

    private static MspNativeParsedWord Word(string text)
    {
        return new MspNativeParsedWord
        {
            Parts = [new MspNativeParsedWordPart
            {
                Text = text,
                IsExpandable = true,
                IsQuoted = false
            }],
            HasExplicitEmptyQuotedFragment = false
        };
    }
}
