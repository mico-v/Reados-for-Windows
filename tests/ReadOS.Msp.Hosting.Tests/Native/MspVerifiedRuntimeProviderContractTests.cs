using System.Text.Json;
using ReadOS.Msp.Hosting.Native;

namespace ReadOS.Msp.Hosting.Tests.Native;

public sealed class MspVerifiedRuntimeProviderContractTests
{
    private const string ManifestSha256 =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    [Fact]
    public void Creates_virtual_launch_plan_from_verified_profile_without_host_authority()
    {
        var result = MspVerifiedRuntimeProviderContractEvaluator.CreateLaunchPlan(
            Provider(),
            new MspVerifiedRuntimeLaunchRequest
            {
                ProfileId = "inspect",
                VirtualCwd = "/workspace/repo",
                Arguments = ["--format", "porcelain"],
                Environment = new Dictionary<string, string>
                {
                    ["LC_ALL"] = "C"
                }
            });

        Assert.True(result.IsReady);
        Assert.Equal("msp.provider.plan.ready", result.StatusCode);
        Assert.NotNull(result.Plan);
        Assert.Equal("bin/git.exe", result.Plan.BundleRelativeEntryPoint);
        Assert.Equal("/workspace/repo", result.Plan.VirtualCwd);
        Assert.Equal(MspVerifiedRuntimeNetworkPolicy.Disabled, result.Plan.Network);
        Assert.Equal("C", result.Plan.Environment["LC_ALL"]);
    }

    [Fact]
    public void Shell_entry_points_are_not_a_provider_profile()
    {
        var result = MspVerifiedRuntimeProviderContractEvaluator.CreateLaunchPlan(
            Provider() with
            {
                Profiles =
                [
                    new MspVerifiedRuntimeCommandProfile
                    {
                        Id = "inspect",
                        BundleRelativeEntryPoint = "bin/pwsh.exe",
                        ExecutableSha256 = new string('a', 64)
                    }
                ]
            },
            Request());

        AssertBlocked(result, MspVerifiedRuntimeBlockReason.ShellEntryPointForbidden);
    }

    [Fact]
    public void Host_paths_and_non_virtual_cwd_are_blocked()
    {
        var argumentResult = MspVerifiedRuntimeProviderContractEvaluator.CreateLaunchPlan(
            Provider(),
            Request() with { Arguments = [@"C:\\private\\repo"] });
        var cwdResult = MspVerifiedRuntimeProviderContractEvaluator.CreateLaunchPlan(
            Provider(),
            Request() with { VirtualCwd = @"C:\\private\\repo" });

        AssertBlocked(argumentResult, MspVerifiedRuntimeBlockReason.HostPathArgument);
        AssertBlocked(cwdResult, MspVerifiedRuntimeBlockReason.VirtualCwdInvalid);
    }

    [Fact]
    public void Host_loader_environment_is_blocked_and_values_are_bounded()
    {
        var keyResult = MspVerifiedRuntimeProviderContractEvaluator.CreateLaunchPlan(
            Provider(),
            Request() with
            {
                Environment = new Dictionary<string, string>
                {
                    ["PATH"] = "bundle-only"
                }
            });
        var valueResult = MspVerifiedRuntimeProviderContractEvaluator.CreateLaunchPlan(
            Provider(),
            Request() with
            {
                Environment = new Dictionary<string, string>
                {
                    ["LC_ALL"] = @"C:\\private\\secret"
                }
            });

        AssertBlocked(keyResult, MspVerifiedRuntimeBlockReason.HostEnvironmentKey);
        AssertBlocked(valueResult, MspVerifiedRuntimeBlockReason.EnvironmentValueInvalid);
    }

    [Fact]
    public void Cancellation_and_process_tree_cleanup_are_mandatory()
    {
        var cancellationResult = MspVerifiedRuntimeProviderContractEvaluator.CreateLaunchPlan(
            Provider() with
            {
                Lifecycle = new MspVerifiedRuntimeLifecyclePolicy
                {
                    ProcessTreeCleanupRequired = true
                }
            },
            Request());
        var cleanupResult = MspVerifiedRuntimeProviderContractEvaluator.CreateLaunchPlan(
            Provider() with
            {
                Lifecycle = new MspVerifiedRuntimeLifecyclePolicy
                {
                    CancellationRequired = true
                }
            },
            Request());

        AssertBlocked(cancellationResult, MspVerifiedRuntimeBlockReason.CancellationPolicyMissing);
        AssertBlocked(cleanupResult, MspVerifiedRuntimeBlockReason.CleanupPolicyMissing);
    }

    [Fact]
    public void Launch_identity_is_revalidated_before_backend_execution()
    {
        var ready = MspVerifiedRuntimeProviderContractEvaluator.CreateLaunchPlan(
            Provider(),
            Request());
        var plan = Assert.IsType<MspVerifiedRuntimeLaunchPlan>(ready.Plan);
        var observed = BundleEvidence();

        var stillValid = MspVerifiedRuntimeProviderContractEvaluator.RevalidateIdentity(
            plan,
            new MspVerifiedRuntimeLaunchIdentityObservation
            {
                Bundle = observed,
                ExecutableSha256 = plan.ExecutableSha256
            });
        var changed = MspVerifiedRuntimeProviderContractEvaluator.RevalidateIdentity(
            plan,
            new MspVerifiedRuntimeLaunchIdentityObservation
            {
                Bundle = observed with { ManifestSha256 = new string('f', 64) },
                ExecutableSha256 = plan.ExecutableSha256
            });
        var unverified = MspVerifiedRuntimeProviderContractEvaluator.RevalidateIdentity(
            plan,
            new MspVerifiedRuntimeLaunchIdentityObservation
            {
                Bundle = observed with { Verification = MspVerifiedRuntimeBundleVerification.Unverified },
                ExecutableSha256 = plan.ExecutableSha256
            });
        var executableChanged = MspVerifiedRuntimeProviderContractEvaluator.RevalidateIdentity(
            plan,
            new MspVerifiedRuntimeLaunchIdentityObservation
            {
                Bundle = observed,
                ExecutableSha256 = new string('b', 64)
            });

        Assert.True(stillValid.IsReady);
        AssertBlocked(changed, MspVerifiedRuntimeBlockReason.IdentityChanged);
        AssertBlocked(unverified, MspVerifiedRuntimeBlockReason.IdentityChanged);
        AssertBlocked(executableChanged, MspVerifiedRuntimeBlockReason.IdentityChanged);
    }

    [Fact]
    public void Missing_executable_digest_blocks_the_profile_before_launch()
    {
        var result = MspVerifiedRuntimeProviderContractEvaluator.CreateLaunchPlan(
            Provider() with
            {
                Profiles =
                [
                    new MspVerifiedRuntimeCommandProfile
                    {
                        Id = "inspect",
                        BundleRelativeEntryPoint = "bin/git.exe",
                        ExecutableSha256 = string.Empty
                    }
                ]
            },
            Request());

        AssertBlocked(result, MspVerifiedRuntimeBlockReason.ExecutableIdentityMissing);
    }

    [Fact]
    public void Registration_combines_bundle_evidence_and_provider_contract()
    {
        var result = MspVerifiedRuntimeProviderContractEvaluator.Register(
            new MspVerifiedRuntimeProviderRegistrationRequest
            {
                ProviderId = "reados-git-provider",
                Bundle = BundleEvidence(),
                ExpectedBundleId = "reados-git-2.49.0",
                ExpectedManifestSha256 = ManifestSha256,
                TargetRid = "win-x64",
                TargetArchitecture = MspVerifiedRuntimeArchitecture.X64,
                SecurityPolicy = SecurityPolicy(),
                Profiles = Provider().Profiles,
                WorkspaceMode = Provider().WorkspaceMode,
                Limits = Provider().Limits,
                Lifecycle = Provider().Lifecycle,
                Provenance = Provider().Provenance
            });

        Assert.True(result.IsUsable);
        Assert.Equal(MspVerifiedRuntimeProviderRegistrationStatus.Verified, result.Status);
        Assert.NotNull(result.Contract);
        Assert.NotNull(result.Bundle);
    }

    [Fact]
    public void Registration_keeps_unverified_bundle_blocked_without_a_contract()
    {
        var result = MspVerifiedRuntimeProviderContractEvaluator.Register(
            new MspVerifiedRuntimeProviderRegistrationRequest
            {
                ProviderId = "reados-git-provider",
                Bundle = BundleEvidence() with
                {
                    Verification = MspVerifiedRuntimeBundleVerification.Unverified
                },
                ExpectedBundleId = "reados-git-2.49.0",
                ExpectedManifestSha256 = ManifestSha256,
                TargetRid = "win-x64",
                TargetArchitecture = MspVerifiedRuntimeArchitecture.X64,
                SecurityPolicy = SecurityPolicy(),
                Profiles = Provider().Profiles,
                WorkspaceMode = Provider().WorkspaceMode,
                Limits = Provider().Limits,
                Lifecycle = Provider().Lifecycle,
                Provenance = Provider().Provenance
            });

        Assert.False(result.IsUsable);
        Assert.Equal(MspVerifiedRuntimeProviderRegistrationStatus.Blocked, result.Status);
        Assert.Equal(MspVerifiedRuntimeBlockReason.CapabilityBlocked, result.BlockReason);
        Assert.Null(result.Contract);
    }

    [Fact]
    public void Registration_blocks_target_architecture_drift_without_a_contract()
    {
        var result = MspVerifiedRuntimeProviderContractEvaluator.Register(
            new MspVerifiedRuntimeProviderRegistrationRequest
            {
                ProviderId = "reados-git-provider",
                Bundle = BundleEvidence(),
                ExpectedBundleId = "reados-git-2.49.0",
                ExpectedManifestSha256 = ManifestSha256,
                TargetRid = "win-x64",
                TargetArchitecture = MspVerifiedRuntimeArchitecture.Arm64,
                SecurityPolicy = SecurityPolicy(),
                Profiles = Provider().Profiles,
                WorkspaceMode = Provider().WorkspaceMode,
                Limits = Provider().Limits,
                Lifecycle = Provider().Lifecycle,
                Provenance = Provider().Provenance
            });

        Assert.False(result.IsUsable);
        Assert.Equal(MspVerifiedRuntimeProviderRegistrationStatus.Blocked, result.Status);
        Assert.Equal(MspVerifiedRuntimeBlockReason.CapabilityBlocked, result.BlockReason);
        Assert.Null(result.Contract);
    }

    [Fact]
    public void Provider_contract_has_no_shell_or_host_path_fields()
    {
        var propertyNames = typeof(MspVerifiedRuntimeLaunchRequest)
            .GetProperties()
            .Select(property => property.Name)
            .ToArray();

        Assert.DoesNotContain("Shell", propertyNames);
        Assert.DoesNotContain("HostPath", propertyNames);
        Assert.DoesNotContain("ExecutablePath", propertyNames);
        Assert.Contains("VirtualCwd", propertyNames);
    }

    [Fact]
    public void Shell_metacharacters_are_just_argv_data_not_a_command_line()
    {
        var result = MspVerifiedRuntimeProviderContractEvaluator.CreateLaunchPlan(
            Provider(),
            Request() with { Arguments = ["&&", "whoami"] });

        Assert.True(result.IsReady);
        Assert.Equal(["&&", "whoami"], result.Plan!.Arguments);
    }

    [Fact]
    public void Blocked_plan_is_truthful_and_does_not_echo_host_path_or_shell_text()
    {
        const string hostPath = @"C:\\private\\secret";
        var result = MspVerifiedRuntimeProviderContractEvaluator.CreateLaunchPlan(
            Provider(),
            Request() with
            {
                Arguments = [hostPath, "&&", "whoami"]
            });

        AssertBlocked(result, MspVerifiedRuntimeBlockReason.HostPathArgument);
        var json = JsonSerializer.Serialize(result);
        Assert.DoesNotContain(hostPath, json, StringComparison.Ordinal);
        Assert.DoesNotContain("whoami", json, StringComparison.Ordinal);
    }

    private static MspVerifiedRuntimeProviderContract Provider()
    {
        return new MspVerifiedRuntimeProviderContract
        {
            ProviderId = "reados-git-provider",
            Bundle = BundleEvidence(),
            SecurityPolicy = SecurityPolicy(),
            Profiles =
            [
                new MspVerifiedRuntimeCommandProfile
                {
                    Id = "inspect",
                    BundleRelativeEntryPoint = "bin/git.exe",
                    ExecutableSha256 = new string('a', 64),
                    ArgumentPolicy = MspVerifiedRuntimeArgumentPolicy.Opaque,
                    MaximumArguments = 8
                }
            ],
            WorkspaceMode = MspVerifiedRuntimeWorkspaceMode.ProjectedReadOnly,
            Limits = new MspVerifiedRuntimeLimits
            {
                MaximumArguments = 8,
                MaximumArgumentBytes = 4096,
                MaximumEnvironmentEntries = 8,
                MaximumEnvironmentValueBytes = 1024
            },
            Lifecycle = new MspVerifiedRuntimeLifecyclePolicy
            {
                CancellationRequired = true,
                ProcessTreeCleanupRequired = true
            },
            Provenance = new MspVerifiedRuntimeProvenance
            {
                LicenseId = "Apache-2.0",
                NoticeId = "reados-git-notice-1"
            }
        };
    }

    private static MspVerifiedRuntimeLaunchRequest Request()
    {
        return new MspVerifiedRuntimeLaunchRequest
        {
            ProfileId = "inspect",
            VirtualCwd = "/workspace",
            Arguments = ["status"],
            Environment = new Dictionary<string, string>
            {
                ["LC_ALL"] = "C"
            }
        };
    }

    private static MspVerifiedRuntimeBundleEvidence BundleEvidence()
    {
        return new MspVerifiedRuntimeBundleEvidence
        {
            Runtime = MspNativeRuntimeKind.Git,
            BundleId = "reados-git-2.49.0",
            ManifestSha256 = ManifestSha256,
            Rid = "win-x64",
            Architecture = MspVerifiedRuntimeArchitecture.X64,
            Verification = MspVerifiedRuntimeBundleVerification.Verified
        };
    }

    private static MspVerifiedRuntimeSecurityPolicy SecurityPolicy()
    {
        return new MspVerifiedRuntimeSecurityPolicy
        {
            Scratch = MspVerifiedRuntimeScratchMode.DedicatedVirtualWorkspace,
            PathResolution = MspVerifiedRuntimePathResolutionPolicy.BundleOnly,
            Network = MspVerifiedRuntimeNetworkPolicy.Disabled
        };
    }

    private static void AssertBlocked(
        MspVerifiedRuntimePlanResult result,
        MspVerifiedRuntimeBlockReason reason)
    {
        Assert.False(result.IsReady);
        Assert.Equal(MspVerifiedRuntimePlanStatus.Blocked, result.Status);
        Assert.Equal(reason, result.BlockReason);
        Assert.Equal("msp.provider.plan.blocked", result.StatusCode);
        Assert.Null(result.Plan);
        Assert.NotNull(result.BlockReasonCode);
        Assert.NotNull(result.BlockMessage);
    }
}
