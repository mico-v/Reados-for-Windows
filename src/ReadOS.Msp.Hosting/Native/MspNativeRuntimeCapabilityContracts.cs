namespace ReadOS.Msp.Hosting.Native;

/// <summary>
/// Runtime families that may be described by a Hosting capability record.
/// These records are declarations only; they do not add commands or launch a
/// runtime.
/// </summary>
public enum MspNativeRuntimeKind
{
    Unknown = 0,
    Python,
    Node,
    Git
}

/// <summary>Verification state supplied by the bundle verifier.</summary>
public enum MspNativeRuntimeBundleVerification
{
    Unverified = 0,
    Verified
}

/// <summary>PE machine values used by the verified bundle contract.</summary>
public enum MspNativePeMachine
{
    Unspecified = 0,
    I386,
    Amd64,
    Arm64
}

/// <summary>
/// Explicit scratch behavior for a future runtime invocation. An unspecified
/// value is never treated as safe by the capability evaluator.
/// </summary>
public enum MspNativeRuntimeScratchMode
{
    Unspecified = 0,
    Disabled,
    DedicatedVirtualWorkspace
}

/// <summary>
/// Restricts executable resolution to the verified bundle. Host PATH lookup is
/// intentionally not a valid capability.
/// </summary>
public enum MspNativeRuntimePathResolutionPolicy
{
    Unspecified = 0,
    BundleOnly
}

/// <summary>Network policy for a runtime capability declaration.</summary>
public enum MspNativeRuntimeNetworkPolicy
{
    Unspecified = 0,
    Disabled
}

/// <summary>Whether a capability record may be consumed by a later launcher.</summary>
public enum MspNativeRuntimeCapabilityStatus
{
    Blocked = 0,
    Verified
}

/// <summary>Stable reasons why a runtime capability remains blocked.</summary>
public enum MspNativeRuntimeBlockReason
{
    None = 0,
    UnsupportedRuntime,
    ExpectedBundleIdentityMissing,
    BundleMissing,
    BundleIdentityMissing,
    BundleUnverified,
    BundleRuntimeMismatch,
    BundleIdentityMismatch,
    BundleRidMissing,
    RidMismatch,
    BundlePeMachineMissing,
    PeMachineMismatch,
    SandboxPolicyMissing,
    ScratchPolicyMissing,
    HostPathPolicyNotBundleOnly,
    NetworkPolicyNotDisabled
}

/// <summary>
/// Identity evidence produced by a verified bundle step. The identity contains
/// no host path. A caller must not set <see cref="Verification"/> to
/// <see cref="MspNativeRuntimeBundleVerification.Verified"/> without verifier
/// evidence.
/// </summary>
public sealed record MspNativeRuntimeBundleIdentity
{
    /// <summary>Stable runtime family represented by this bundle.</summary>
    public MspNativeRuntimeKind Runtime { get; init; }

    /// <summary>Stable package/bundle identifier, not a filesystem path.</summary>
    public string? BundleId { get; init; }

    /// <summary>Verified manifest SHA-256 in hexadecimal form.</summary>
    public string? ManifestSha256 { get; init; }

    /// <summary>Runtime identifier declared by the bundle manifest.</summary>
    public string? Rid { get; init; }

    /// <summary>PE machine declared by the bundle manifest.</summary>
    public MspNativePeMachine PeMachine { get; init; }

    /// <summary>Whether the identity was produced by the verifier.</summary>
    public MspNativeRuntimeBundleVerification Verification { get; init; }
}

/// <summary>
/// Identity expected by a host before it can accept a verified bundle. This is
/// metadata only and deliberately has no bundle path or executable path.
/// </summary>
public sealed record MspNativeRuntimeBundleExpectation
{
    public string? BundleId { get; init; }

    public string? ManifestSha256 { get; init; }
}

/// <summary>
/// Explicit isolation policy required for every runtime capability. The
/// defaults are non-capabilities and are rejected by the evaluator.
/// </summary>
public sealed record MspNativeRuntimeSandboxPolicy
{
    public MspNativeRuntimeScratchMode Scratch { get; init; }

    public MspNativeRuntimePathResolutionPolicy PathResolution { get; init; }

    public MspNativeRuntimeNetworkPolicy Network { get; init; }

    public static MspNativeRuntimeSandboxPolicy Unspecified { get; } = new();
}

/// <summary>
/// Host declaration for one Python, Node, or Git runtime. Evaluating this
/// record never loads a bundle, resolves PATH, opens a network connection, or
/// starts a process.
/// </summary>
public sealed record MspNativeRuntimeRegistrationRequest
{
    public MspNativeRuntimeKind Runtime { get; init; }

    public MspNativeRuntimeBundleExpectation? ExpectedBundle { get; init; }

    public MspNativeRuntimeBundleIdentity? Bundle { get; init; }

    /// <summary>Host target RID selected explicitly by the caller.</summary>
    public string? TargetRid { get; init; }

    /// <summary>Host target PE selected explicitly by the caller.</summary>
    public MspNativePeMachine TargetPeMachine { get; init; }

    /// <summary>
    /// Must be present and must explicitly deny host PATH and network access.
    /// </summary>
    public MspNativeRuntimeSandboxPolicy? SandboxPolicy { get; init; }
}

/// <summary>
/// Truthful, path-free capability evidence for one runtime registration
/// attempt. A <see cref="Blocked"/> record is never usable as a registration.
/// </summary>
public sealed record MspNativeRuntimeCapabilityRecord
{
    public required MspNativeRuntimeKind Runtime { get; init; }

    public required MspNativeRuntimeCapabilityStatus Status { get; init; }

    public MspNativeRuntimeBlockReason BlockReason { get; init; }

    public MspNativeRuntimeBundleIdentity? Bundle { get; init; }

    public MspNativeRuntimeSandboxPolicy SandboxPolicy { get; init; } =
        MspNativeRuntimeSandboxPolicy.Unspecified;

    /// <summary>Stable status code suitable for diagnostics or evidence.</summary>
    public string StatusCode => Status == MspNativeRuntimeCapabilityStatus.Verified
        ? "msp.native.runtime.verified"
        : "msp.native.runtime.blocked";

    /// <summary>Stable reason code; null for a verified record.</summary>
    public string? BlockReasonCode => BlockReason == MspNativeRuntimeBlockReason.None
        ? null
        : MspNativeRuntimeCapabilityEvaluator.GetBlockReasonCode(BlockReason);

    /// <summary>Human-readable, path-free reason; null for a verified record.</summary>
    public string? BlockMessage => BlockReason == MspNativeRuntimeBlockReason.None
        ? null
        : MspNativeRuntimeCapabilityEvaluator.GetBlockReasonMessage(BlockReason);

    public bool IsBlocked => Status == MspNativeRuntimeCapabilityStatus.Blocked;

    public bool IsUsable => Status == MspNativeRuntimeCapabilityStatus.Verified &&
        BlockReason == MspNativeRuntimeBlockReason.None;
}

/// <summary>
/// Pure fail-closed evaluator for runtime capability records. It validates
/// evidence and policy only; it has no process, filesystem, PATH, network, or
/// command-registry side effects.
/// </summary>
public static class MspNativeRuntimeCapabilityEvaluator
{
    public static MspNativeRuntimeCapabilityRecord Evaluate(
        MspNativeRuntimeRegistrationRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);

        if (!IsSupportedRuntime(request.Runtime))
        {
            return Blocked(request, MspNativeRuntimeBlockReason.UnsupportedRuntime);
        }

        if (request.ExpectedBundle is null ||
            !IsSafeBundleId(request.ExpectedBundle.BundleId) ||
            !IsSha256(request.ExpectedBundle.ManifestSha256))
        {
            return Blocked(request, MspNativeRuntimeBlockReason.ExpectedBundleIdentityMissing);
        }

        if (request.Bundle is null)
        {
            return Blocked(request, MspNativeRuntimeBlockReason.BundleMissing);
        }

        var bundle = request.Bundle;
        if (bundle.Verification != MspNativeRuntimeBundleVerification.Verified)
        {
            return Blocked(request, MspNativeRuntimeBlockReason.BundleUnverified);
        }

        if (bundle.Runtime != request.Runtime)
        {
            return Blocked(request, MspNativeRuntimeBlockReason.BundleRuntimeMismatch);
        }

        if (!IsSafeBundleId(bundle.BundleId) ||
            !IsSha256(bundle.ManifestSha256))
        {
            return Blocked(request, MspNativeRuntimeBlockReason.BundleIdentityMissing);
        }

        if (!string.Equals(
                bundle.BundleId,
                request.ExpectedBundle.BundleId,
                StringComparison.Ordinal))
        {
            return Blocked(request, MspNativeRuntimeBlockReason.BundleIdentityMismatch);
        }

        if (!string.Equals(
                bundle.ManifestSha256,
                request.ExpectedBundle.ManifestSha256,
                StringComparison.OrdinalIgnoreCase))
        {
            return Blocked(request, MspNativeRuntimeBlockReason.BundleIdentityMismatch);
        }

        if (!IsSafeRid(bundle.Rid))
        {
            return Blocked(request, MspNativeRuntimeBlockReason.BundleRidMissing);
        }

        if (!IsSafeRid(request.TargetRid) ||
            !string.Equals(bundle.Rid, request.TargetRid, StringComparison.OrdinalIgnoreCase))
        {
            return Blocked(request, MspNativeRuntimeBlockReason.RidMismatch);
        }

        if (bundle.PeMachine == MspNativePeMachine.Unspecified)
        {
            return Blocked(request, MspNativeRuntimeBlockReason.BundlePeMachineMissing);
        }

        if (request.TargetPeMachine == MspNativePeMachine.Unspecified ||
            bundle.PeMachine != request.TargetPeMachine)
        {
            return Blocked(request, MspNativeRuntimeBlockReason.PeMachineMismatch);
        }

        if (request.SandboxPolicy is null)
        {
            return Blocked(request, MspNativeRuntimeBlockReason.SandboxPolicyMissing);
        }

        var policy = request.SandboxPolicy;
        if (policy.Scratch == MspNativeRuntimeScratchMode.Unspecified)
        {
            return Blocked(request, MspNativeRuntimeBlockReason.ScratchPolicyMissing);
        }

        if (policy.PathResolution != MspNativeRuntimePathResolutionPolicy.BundleOnly)
        {
            return Blocked(request, MspNativeRuntimeBlockReason.HostPathPolicyNotBundleOnly);
        }

        if (policy.Network != MspNativeRuntimeNetworkPolicy.Disabled)
        {
            return Blocked(request, MspNativeRuntimeBlockReason.NetworkPolicyNotDisabled);
        }

        return new MspNativeRuntimeCapabilityRecord
        {
            Runtime = request.Runtime,
            Status = MspNativeRuntimeCapabilityStatus.Verified,
            BlockReason = MspNativeRuntimeBlockReason.None,
            Bundle = bundle,
            SandboxPolicy = policy
        };
    }

    public static bool IsSupportedRuntime(MspNativeRuntimeKind runtime)
    {
        return runtime is MspNativeRuntimeKind.Python or
            MspNativeRuntimeKind.Node or
            MspNativeRuntimeKind.Git;
    }

    internal static string GetBlockReasonCode(MspNativeRuntimeBlockReason reason)
    {
        return reason switch
        {
            MspNativeRuntimeBlockReason.UnsupportedRuntime => "msp.native.runtime.unsupported",
            MspNativeRuntimeBlockReason.ExpectedBundleIdentityMissing => "msp.native.runtime.expected_bundle_identity_missing",
            MspNativeRuntimeBlockReason.BundleMissing => "msp.native.runtime.bundle_missing",
            MspNativeRuntimeBlockReason.BundleIdentityMissing => "msp.native.runtime.bundle_identity_missing",
            MspNativeRuntimeBlockReason.BundleUnverified => "msp.native.runtime.bundle_unverified",
            MspNativeRuntimeBlockReason.BundleRuntimeMismatch => "msp.native.runtime.bundle_runtime_mismatch",
            MspNativeRuntimeBlockReason.BundleIdentityMismatch => "msp.native.runtime.bundle_identity_mismatch",
            MspNativeRuntimeBlockReason.BundleRidMissing => "msp.native.runtime.bundle_rid_missing",
            MspNativeRuntimeBlockReason.RidMismatch => "msp.native.runtime.rid_mismatch",
            MspNativeRuntimeBlockReason.BundlePeMachineMissing => "msp.native.runtime.bundle_pe_machine_missing",
            MspNativeRuntimeBlockReason.PeMachineMismatch => "msp.native.runtime.pe_machine_mismatch",
            MspNativeRuntimeBlockReason.SandboxPolicyMissing => "msp.native.runtime.sandbox_policy_missing",
            MspNativeRuntimeBlockReason.ScratchPolicyMissing => "msp.native.runtime.scratch_policy_missing",
            MspNativeRuntimeBlockReason.HostPathPolicyNotBundleOnly => "msp.native.runtime.host_path_policy_not_bundle_only",
            MspNativeRuntimeBlockReason.NetworkPolicyNotDisabled => "msp.native.runtime.network_policy_not_disabled",
            _ => "msp.native.runtime.blocked"
        };
    }

    internal static string GetBlockReasonMessage(MspNativeRuntimeBlockReason reason)
    {
        return reason switch
        {
            MspNativeRuntimeBlockReason.UnsupportedRuntime => "The runtime family is not enabled for native registration.",
            MspNativeRuntimeBlockReason.ExpectedBundleIdentityMissing => "The expected verified bundle identity is missing.",
            MspNativeRuntimeBlockReason.BundleMissing => "The runtime bundle is missing.",
            MspNativeRuntimeBlockReason.BundleIdentityMissing => "The runtime bundle identity is incomplete.",
            MspNativeRuntimeBlockReason.BundleUnverified => "The runtime bundle was not verified.",
            MspNativeRuntimeBlockReason.BundleRuntimeMismatch => "The verified bundle belongs to a different runtime family.",
            MspNativeRuntimeBlockReason.BundleIdentityMismatch => "The verified bundle identity does not match the expected identity.",
            MspNativeRuntimeBlockReason.BundleRidMissing => "The verified bundle has no runtime identifier.",
            MspNativeRuntimeBlockReason.RidMismatch => "The verified bundle runtime identifier does not match the host target.",
            MspNativeRuntimeBlockReason.BundlePeMachineMissing => "The verified bundle has no PE machine identity.",
            MspNativeRuntimeBlockReason.PeMachineMismatch => "The verified bundle PE machine does not match the host target.",
            MspNativeRuntimeBlockReason.SandboxPolicyMissing => "The runtime scratch, PATH, and network policy is missing.",
            MspNativeRuntimeBlockReason.ScratchPolicyMissing => "The runtime scratch policy is not explicit.",
            MspNativeRuntimeBlockReason.HostPathPolicyNotBundleOnly => "Host PATH lookup is not permitted for runtime registration.",
            MspNativeRuntimeBlockReason.NetworkPolicyNotDisabled => "Network access is not explicitly disabled for runtime registration.",
            _ => "The runtime capability is blocked."
        };
    }

    private static MspNativeRuntimeCapabilityRecord Blocked(
        MspNativeRuntimeRegistrationRequest request,
        MspNativeRuntimeBlockReason reason)
    {
        return new MspNativeRuntimeCapabilityRecord
        {
            Runtime = request.Runtime,
            Status = MspNativeRuntimeCapabilityStatus.Blocked,
            BlockReason = reason,
            Bundle = null,
            SandboxPolicy = request.SandboxPolicy ?? MspNativeRuntimeSandboxPolicy.Unspecified
        };
    }

    private static bool IsSafeBundleId(string? value)
    {
        if (value is null || value.Length == 0 || value.Length > 128)
        {
            return false;
        }

        foreach (var character in value)
        {
            if (!(character is >= 'A' and <= 'Z' or
                >= 'a' and <= 'z' or
                >= '0' and <= '9' or '-' or '_' or '.'))
            {
                return false;
            }
        }

        return true;
    }

    private static bool IsSafeRid(string? value)
    {
        if (value is null || value.Length == 0 || value.Length > 64)
        {
            return false;
        }

        foreach (var character in value)
        {
            if (!(character is >= 'A' and <= 'Z' or
                >= 'a' and <= 'z' or
                >= '0' and <= '9' or '-' or '_' or '.'))
            {
                return false;
            }
        }

        return true;
    }

    private static bool IsSha256(string? value)
    {
        if (value is null || value.Length != 64)
        {
            return false;
        }

        foreach (var character in value)
        {
            if (!Uri.IsHexDigit(character))
            {
                return false;
            }
        }

        return true;
    }
}
