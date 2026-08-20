using System.Text.Json;
using ReadOS.Msp.Hosting.Native;

namespace ReadOS.Msp.Hosting.Tests.Native;

public sealed class MspNativeRuntimeCapabilityContractsTests
{
    private const string BundleId = "reados-python-3.13.5";
    private const string ManifestSha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    private const string TargetRid = "win-x64";

    [Theory]
    [InlineData(MspNativeRuntimeKind.Python)]
    [InlineData(MspNativeRuntimeKind.Node)]
    [InlineData(MspNativeRuntimeKind.Git)]
    public void Verified_python_node_and_git_records_are_usable_without_launching(
        MspNativeRuntimeKind runtime)
    {
        var record = MspNativeRuntimeCapabilityEvaluator.Evaluate(
            Request(runtime));

        Assert.Equal(MspNativeRuntimeCapabilityStatus.Verified, record.Status);
        Assert.False(record.IsBlocked);
        Assert.True(record.IsUsable);
        Assert.Equal("msp.native.runtime.verified", record.StatusCode);
        Assert.Null(record.BlockReasonCode);
        Assert.Null(record.BlockMessage);
        Assert.Equal(runtime, record.Bundle!.Runtime);
        Assert.Equal(
            MspNativeRuntimePathResolutionPolicy.BundleOnly,
            record.SandboxPolicy.PathResolution);
        Assert.Equal(
            MspNativeRuntimeNetworkPolicy.Disabled,
            record.SandboxPolicy.Network);
    }

    [Fact]
    public void Missing_bundle_is_blocked_and_never_reported_as_verified()
    {
        var record = MspNativeRuntimeCapabilityEvaluator.Evaluate(
            Request(MspNativeRuntimeKind.Python) with { Bundle = null });

        AssertBlocked(record, MspNativeRuntimeBlockReason.BundleMissing);
        Assert.Null(record.Bundle);
    }

    [Fact]
    public void Mismatched_bundle_identity_is_blocked()
    {
        var record = MspNativeRuntimeCapabilityEvaluator.Evaluate(
            Request(MspNativeRuntimeKind.Node) with
            {
                Bundle = Bundle(MspNativeRuntimeKind.Node) with
                {
                    BundleId = "reados-node-different"
                }
            });

        AssertBlocked(record, MspNativeRuntimeBlockReason.BundleIdentityMismatch);
        Assert.Equal(
            "msp.native.runtime.bundle_identity_mismatch",
            record.BlockReasonCode);
    }

    [Fact]
    public void Mismatched_manifest_digest_is_blocked_even_when_bundle_id_matches()
    {
        var record = MspNativeRuntimeCapabilityEvaluator.Evaluate(
            Request(MspNativeRuntimeKind.Git) with
            {
                Bundle = Bundle(MspNativeRuntimeKind.Git) with
                {
                    ManifestSha256 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
                }
            });

        AssertBlocked(record, MspNativeRuntimeBlockReason.BundleIdentityMismatch);
    }

    [Fact]
    public void Path_shaped_bundle_identity_is_blocked_without_echoing_the_candidate()
    {
        const string hostPath = @"C:\private\runtime-bundle";
        var record = MspNativeRuntimeCapabilityEvaluator.Evaluate(
            Request(MspNativeRuntimeKind.Python) with
            {
                Bundle = Bundle(MspNativeRuntimeKind.Python) with
                {
                    BundleId = hostPath
                }
            });

        AssertBlocked(record, MspNativeRuntimeBlockReason.BundleIdentityMissing);
        Assert.Null(record.Bundle);
        Assert.DoesNotContain(hostPath, JsonSerializer.Serialize(record));
    }

    [Fact]
    public void Unverified_bundle_is_blocked_before_identity_or_target_matching()
    {
        var record = MspNativeRuntimeCapabilityEvaluator.Evaluate(
            Request(MspNativeRuntimeKind.Python) with
            {
                Bundle = Bundle(MspNativeRuntimeKind.Python) with
                {
                    Verification = MspNativeRuntimeBundleVerification.Unverified,
                    Rid = "wrong-rid",
                    PeMachine = MspNativePeMachine.Arm64
                }
            });

        AssertBlocked(record, MspNativeRuntimeBlockReason.BundleUnverified);
        Assert.Equal("msp.native.runtime.blocked", record.StatusCode);
    }

    [Fact]
    public void Rid_and_pe_mismatches_are_blocked()
    {
        var ridRecord = MspNativeRuntimeCapabilityEvaluator.Evaluate(
            Request(MspNativeRuntimeKind.Python) with
            {
                Bundle = Bundle(MspNativeRuntimeKind.Python) with { Rid = "win-arm64" }
            });
        var peRecord = MspNativeRuntimeCapabilityEvaluator.Evaluate(
            Request(MspNativeRuntimeKind.Python) with
            {
                Bundle = Bundle(MspNativeRuntimeKind.Python) with
                {
                    PeMachine = MspNativePeMachine.Arm64
                }
            });

        AssertBlocked(ridRecord, MspNativeRuntimeBlockReason.RidMismatch);
        AssertBlocked(peRecord, MspNativeRuntimeBlockReason.PeMachineMismatch);
    }

    [Fact]
    public void Missing_scratch_policy_is_blocked_instead_of_defaulting_to_host_behavior()
    {
        var record = MspNativeRuntimeCapabilityEvaluator.Evaluate(
            Request(MspNativeRuntimeKind.Python) with
            {
                SandboxPolicy = new MspNativeRuntimeSandboxPolicy
                {
                    PathResolution = MspNativeRuntimePathResolutionPolicy.BundleOnly,
                    Network = MspNativeRuntimeNetworkPolicy.Disabled
                }
            });

        AssertBlocked(record, MspNativeRuntimeBlockReason.ScratchPolicyMissing);
    }

    [Fact]
    public void Host_path_lookup_is_blocked_even_when_other_policy_fields_are_safe()
    {
        var record = MspNativeRuntimeCapabilityEvaluator.Evaluate(
            Request(MspNativeRuntimeKind.Node) with
            {
                SandboxPolicy = new MspNativeRuntimeSandboxPolicy
                {
                    Scratch = MspNativeRuntimeScratchMode.Disabled,
                    PathResolution = MspNativeRuntimePathResolutionPolicy.Unspecified,
                    Network = MspNativeRuntimeNetworkPolicy.Disabled
                }
            });

        AssertBlocked(record, MspNativeRuntimeBlockReason.HostPathPolicyNotBundleOnly);
    }

    [Fact]
    public void Network_access_is_blocked_when_not_explicitly_disabled()
    {
        var record = MspNativeRuntimeCapabilityEvaluator.Evaluate(
            Request(MspNativeRuntimeKind.Git) with
            {
                SandboxPolicy = new MspNativeRuntimeSandboxPolicy
                {
                    Scratch = MspNativeRuntimeScratchMode.Disabled,
                    PathResolution = MspNativeRuntimePathResolutionPolicy.BundleOnly,
                    Network = MspNativeRuntimeNetworkPolicy.Unspecified
                }
            });

        AssertBlocked(record, MspNativeRuntimeBlockReason.NetworkPolicyNotDisabled);
    }

    [Fact]
    public void Capability_contract_contains_identity_and_policy_only_not_launch_inputs()
    {
        var request = Request(MspNativeRuntimeKind.Python);
        var json = JsonSerializer.Serialize(request);

        Assert.DoesNotContain("program", json, StringComparison.OrdinalIgnoreCase);
        Assert.DoesNotContain("executable", json, StringComparison.OrdinalIgnoreCase);
        Assert.DoesNotContain("hostPath", json, StringComparison.OrdinalIgnoreCase);
        Assert.DoesNotContain("networkUrl", json, StringComparison.OrdinalIgnoreCase);
    }

    private static MspNativeRuntimeRegistrationRequest Request(
        MspNativeRuntimeKind runtime)
    {
        return new MspNativeRuntimeRegistrationRequest
        {
            Runtime = runtime,
            ExpectedBundle = new MspNativeRuntimeBundleExpectation
            {
                BundleId = runtime switch
                {
                    MspNativeRuntimeKind.Python => BundleId,
                    MspNativeRuntimeKind.Node => "reados-node-22.14.0",
                    MspNativeRuntimeKind.Git => "reados-git-2.49.0",
                    _ => BundleId
                },
                ManifestSha256 = ManifestSha256
            },
            Bundle = Bundle(runtime),
            TargetRid = TargetRid,
            TargetPeMachine = MspNativePeMachine.Amd64,
            SandboxPolicy = new MspNativeRuntimeSandboxPolicy
            {
                Scratch = MspNativeRuntimeScratchMode.DedicatedVirtualWorkspace,
                PathResolution = MspNativeRuntimePathResolutionPolicy.BundleOnly,
                Network = MspNativeRuntimeNetworkPolicy.Disabled
            }
        };
    }

    private static MspNativeRuntimeBundleIdentity Bundle(
        MspNativeRuntimeKind runtime)
    {
        return new MspNativeRuntimeBundleIdentity
        {
            Runtime = runtime,
            BundleId = runtime switch
            {
                MspNativeRuntimeKind.Python => BundleId,
                MspNativeRuntimeKind.Node => "reados-node-22.14.0",
                MspNativeRuntimeKind.Git => "reados-git-2.49.0",
                _ => BundleId
            },
            ManifestSha256 = ManifestSha256,
            Rid = TargetRid,
            PeMachine = MspNativePeMachine.Amd64,
            Verification = MspNativeRuntimeBundleVerification.Verified
        };
    }

    private static void AssertBlocked(
        MspNativeRuntimeCapabilityRecord record,
        MspNativeRuntimeBlockReason reason)
    {
        Assert.Equal(MspNativeRuntimeCapabilityStatus.Blocked, record.Status);
        Assert.True(record.IsBlocked);
        Assert.False(record.IsUsable);
        Assert.Equal(reason, record.BlockReason);
        Assert.Equal("msp.native.runtime.blocked", record.StatusCode);
        Assert.NotNull(record.BlockReasonCode);
        Assert.NotNull(record.BlockMessage);
    }
}
