using System.Collections.ObjectModel;
using System.Text;

namespace ReadOS.Msp.Hosting.Native;

/// <summary>
/// The workspace authority made available to a verified external runtime.
/// A provider must choose an explicit mode; an omitted mode is never promoted
/// to host filesystem access.
/// </summary>
public enum MspVerifiedRuntimeWorkspaceMode
{
    Unspecified = 0,
    Disabled,
    DedicatedVirtualWorkspace,
    ProjectedReadOnly
}

/// <summary>Platform-neutral machine architecture for a provider bundle.</summary>
public enum MspVerifiedRuntimeArchitecture
{
    Unspecified = 0,
    X86,
    X64,
    Arm32,
    Arm64,
    Wasm32
}

/// <summary>Verification state supplied by any platform bundle verifier.</summary>
public enum MspVerifiedRuntimeBundleVerification
{
    Unverified = 0,
    Verified
}

/// <summary>
/// Platform-neutral scratch authority for a provider launch. Platform
/// backends map this to their own temporary/workspace isolation primitive.
/// </summary>
public enum MspVerifiedRuntimeScratchMode
{
    Unspecified = 0,
    Disabled,
    DedicatedVirtualWorkspace
}

/// <summary>
/// Platform-neutral bundle identity. It deliberately contains no PE-only
/// fields, host paths, executable paths, or platform handles.
/// </summary>
public sealed record MspVerifiedRuntimeBundleEvidence
{
    public required MspNativeRuntimeKind Runtime { get; init; }

    public required string BundleId { get; init; }

    public required string ManifestSha256 { get; init; }

    public required string Rid { get; init; }

    public required MspVerifiedRuntimeArchitecture Architecture { get; init; }

    public required MspVerifiedRuntimeBundleVerification Verification { get; init; }
}

public enum MspVerifiedRuntimePathResolutionPolicy
{
    Unspecified = 0,
    BundleOnly
}

public enum MspVerifiedRuntimeNetworkPolicy
{
    Unspecified = 0,
    Disabled
}

public sealed record MspVerifiedRuntimeSecurityPolicy
{
    public MspVerifiedRuntimeScratchMode Scratch { get; init; }

    public MspVerifiedRuntimePathResolutionPolicy PathResolution { get; init; }

    public MspVerifiedRuntimeNetworkPolicy Network { get; init; }
}

/// <summary>How a registered profile interprets its explicit argv values.</summary>
public enum MspVerifiedRuntimeArgumentPolicy
{
    Opaque = 0,
    VirtualWorkspaceOnly
}

/// <summary>Lifecycle guarantees required before a launch plan is usable.</summary>
public sealed record MspVerifiedRuntimeLifecyclePolicy
{
    public bool CancellationRequired { get; init; }

    public bool ProcessTreeCleanupRequired { get; init; }
}

/// <summary>
/// Bounded provider launch limits. Values are deliberately transport-neutral;
/// platform backends translate them to their own process/sandbox primitives.
/// </summary>
public sealed record MspVerifiedRuntimeLimits
{
    public const int DefaultMaximumArguments = 64;
    public const int DefaultMaximumArgumentBytes = 64 * 1024;
    public const int DefaultMaximumEnvironmentEntries = 64;
    public const int DefaultMaximumEnvironmentValueBytes = 16 * 1024;
    public const int DefaultMaximumStdoutBytes = 4 * 1024 * 1024;
    public const int DefaultMaximumStderrBytes = 4 * 1024 * 1024;
    public const int DefaultMaximumWallClockMilliseconds = 120_000;
    public const int DefaultMaximumMemoryBytes = 512 * 1024 * 1024;
    public const int DefaultMaximumChildProcesses = 8;
    public const int DefaultMaximumWorkspaceFiles = 65_536;

    public int MaximumArguments { get; init; } = DefaultMaximumArguments;

    public int MaximumArgumentBytes { get; init; } = DefaultMaximumArgumentBytes;

    public int MaximumEnvironmentEntries { get; init; } = DefaultMaximumEnvironmentEntries;

    public int MaximumEnvironmentValueBytes { get; init; } = DefaultMaximumEnvironmentValueBytes;

    public int MaximumStdoutBytes { get; init; } = DefaultMaximumStdoutBytes;

    public int MaximumStderrBytes { get; init; } = DefaultMaximumStderrBytes;

    public int MaximumWallClockMilliseconds { get; init; } = DefaultMaximumWallClockMilliseconds;

    public int MaximumMemoryBytes { get; init; } = DefaultMaximumMemoryBytes;

    public int MaximumChildProcesses { get; init; } = DefaultMaximumChildProcesses;

    public int MaximumWorkspaceFiles { get; init; } = DefaultMaximumWorkspaceFiles;
}

/// <summary>
/// License and notice identifiers for one provider package. These are
/// identifiers, never host paths or arbitrary document text.
/// </summary>
public sealed record MspVerifiedRuntimeProvenance
{
    public required string LicenseId { get; init; }

    public required string NoticeId { get; init; }
}

/// <summary>
/// One explicitly registered command profile. The entry point is relative to
/// the verified bundle and is resolved only by a platform backend at launch.
/// </summary>
public sealed record MspVerifiedRuntimeCommandProfile
{
    public required string Id { get; init; }

    public required string BundleRelativeEntryPoint { get; init; }

    /// <summary>Verifier-produced digest for the executable at the entry point.</summary>
    public required string ExecutableSha256 { get; init; }

    public MspVerifiedRuntimeArgumentPolicy ArgumentPolicy { get; init; } =
        MspVerifiedRuntimeArgumentPolicy.Opaque;

    /// <summary>
    /// Immutable argv prefix owned by this profile. This is the allowlist
    /// boundary for multi-call providers such as Toybox: the caller may only
    /// supply the bounded suffix after this prefix. An empty prefix preserves
    /// the ordinary provider-profile behavior.
    /// </summary>
    public IReadOnlyList<string> ArgumentPrefix { get; init; } = Array.Empty<string>();

    public int MaximumArguments { get; init; } = MspVerifiedRuntimeLimits.DefaultMaximumArguments;
}

/// <summary>
/// Host-neutral verified provider declaration. It describes what a later
/// platform backend may launch; it does not launch, inspect, or authorize a
/// process and does not own policy, approval, or product audit.
/// </summary>
public sealed record MspVerifiedRuntimeProviderContract
{
    public required string ProviderId { get; init; }

    public required MspVerifiedRuntimeBundleEvidence Bundle { get; init; }

    public required MspVerifiedRuntimeSecurityPolicy SecurityPolicy { get; init; }

    public required IReadOnlyList<MspVerifiedRuntimeCommandProfile> Profiles { get; init; }

    public required MspVerifiedRuntimeWorkspaceMode WorkspaceMode { get; init; }

    public required MspVerifiedRuntimeLimits Limits { get; init; }

    public required MspVerifiedRuntimeLifecyclePolicy Lifecycle { get; init; }

    public required MspVerifiedRuntimeProvenance Provenance { get; init; }
}

/// <summary>
/// Input to the host-neutral registration gate. Verified bundle evidence is
/// checked first, then provider profile and provenance rules are evaluated
/// without touching a platform API.
/// </summary>
public sealed record MspVerifiedRuntimeProviderRegistrationRequest
{
    public required string ProviderId { get; init; }

    public required MspVerifiedRuntimeBundleEvidence Bundle { get; init; }

    public required string ExpectedBundleId { get; init; }

    public required string ExpectedManifestSha256 { get; init; }

    public required string TargetRid { get; init; }

    public required MspVerifiedRuntimeArchitecture TargetArchitecture { get; init; }

    public required MspVerifiedRuntimeSecurityPolicy SecurityPolicy { get; init; }

    public required IReadOnlyList<MspVerifiedRuntimeCommandProfile> Profiles { get; init; }

    public required MspVerifiedRuntimeWorkspaceMode WorkspaceMode { get; init; }

    public required MspVerifiedRuntimeLimits Limits { get; init; }

    public required MspVerifiedRuntimeLifecyclePolicy Lifecycle { get; init; }

    public required MspVerifiedRuntimeProvenance Provenance { get; init; }
}

public enum MspVerifiedRuntimeProviderRegistrationStatus
{
    Blocked = 0,
    Verified
}

/// <summary>Truthful result of provider registration; blocked means no contract.</summary>
public sealed record MspVerifiedRuntimeProviderRegistrationResult
{
    public required MspVerifiedRuntimeProviderRegistrationStatus Status { get; init; }

    public MspVerifiedRuntimeBlockReason BlockReason { get; init; }

    public MspVerifiedRuntimeBundleEvidence? Bundle { get; init; }

    public MspVerifiedRuntimeProviderContract? Contract { get; init; }

    public bool IsUsable => Status == MspVerifiedRuntimeProviderRegistrationStatus.Verified &&
        BlockReason == MspVerifiedRuntimeBlockReason.None &&
        Bundle is not null &&
        Contract is not null;
}

/// <summary>
/// Explicit launch request. It contains only virtual cwd, registered profile
/// id, argv, and caller-supplied environment values. There is no shell string,
/// host cwd, executable path, or PATH lookup input.
/// </summary>
public sealed record MspVerifiedRuntimeLaunchRequest
{
    public required string ProfileId { get; init; }

    public string VirtualCwd { get; init; } = "/workspace";

    public IReadOnlyList<string> Arguments { get; init; } = Array.Empty<string>();

    public IReadOnlyDictionary<string, string> Environment { get; init; } =
        new ReadOnlyDictionary<string, string>(new Dictionary<string, string>(StringComparer.Ordinal));
}

/// <summary>Stable reasons for a blocked provider or launch-plan attempt.</summary>
public enum MspVerifiedRuntimeBlockReason
{
    None = 0,
    ProviderIdInvalid,
    CapabilityBlocked,
    CapabilityIdentityMissing,
    ProviderProfilesMissing,
    DuplicateProfile,
    ProfileIdInvalid,
    EntryPointInvalid,
    ShellEntryPointForbidden,
    ExecutableIdentityMissing,
    ProfileArgumentLimitInvalid,
    WorkspacePolicyMissing,
    LifecyclePolicyMissing,
    CancellationPolicyMissing,
    CleanupPolicyMissing,
    LimitsInvalid,
    ProvenanceMissing,
    ProvenanceInvalid,
    ProfileMissing,
    RequestProfileIdInvalid,
    ArgumentLimitExceeded,
    ArgumentPrefixInvalid,
    ArgumentPrefixMismatch,
    ArgumentInvalid,
    HostPathArgument,
    VirtualCwdInvalid,
    EnvironmentLimitExceeded,
    EnvironmentKeyInvalid,
    HostEnvironmentKey,
    EnvironmentValueInvalid,
    IdentityChanged,
    ProviderAlreadyRegistered,
    ProviderNotRegistered
}

public enum MspVerifiedRuntimePlanStatus
{
    Blocked = 0,
    Ready
}

/// <summary>Immutable, backend-ready values produced after contract checks.</summary>
public sealed record MspVerifiedRuntimeLaunchPlan
{
    public required string ProviderId { get; init; }

    public required string ProfileId { get; init; }

    public required MspNativeRuntimeKind Runtime { get; init; }

    public required string BundleId { get; init; }

    public required string ManifestSha256 { get; init; }

    public required string TargetRid { get; init; }

    public required MspVerifiedRuntimeArchitecture TargetArchitecture { get; init; }

    public required string BundleRelativeEntryPoint { get; init; }

    public required string ExecutableSha256 { get; init; }

    public required string VirtualCwd { get; init; }

    public required IReadOnlyList<string> Arguments { get; init; }

    public required IReadOnlyDictionary<string, string> Environment { get; init; }

    public required MspVerifiedRuntimeWorkspaceMode WorkspaceMode { get; init; }

    public required MspVerifiedRuntimeNetworkPolicy Network { get; init; }

    public required MspVerifiedRuntimeLimits Limits { get; init; }

    public required MspVerifiedRuntimeLifecyclePolicy Lifecycle { get; init; }

    public required MspVerifiedRuntimeProvenance Provenance { get; init; }
}

/// <summary>
/// Fresh launch-time identity observed by a platform backend. The backend owns
/// how it obtains the digest; this contract receives no host path.
/// </summary>
public sealed record MspVerifiedRuntimeLaunchIdentityObservation
{
    public required MspVerifiedRuntimeBundleEvidence Bundle { get; init; }

    public required string ExecutableSha256 { get; init; }
}

/// <summary>Path-free result of building or revalidating a launch plan.</summary>
public sealed record MspVerifiedRuntimePlanResult
{
    public required MspVerifiedRuntimePlanStatus Status { get; init; }

    public MspVerifiedRuntimeBlockReason BlockReason { get; init; }

    public MspVerifiedRuntimeLaunchPlan? Plan { get; init; }

    public bool IsReady => Status == MspVerifiedRuntimePlanStatus.Ready &&
        BlockReason == MspVerifiedRuntimeBlockReason.None &&
        Plan is not null;

    public string StatusCode => IsReady
        ? "msp.provider.plan.ready"
        : "msp.provider.plan.blocked";

    public string? BlockReasonCode => BlockReason == MspVerifiedRuntimeBlockReason.None
        ? null
        : MspVerifiedRuntimeProviderContractEvaluator.GetBlockReasonCode(BlockReason);

    public string? BlockMessage => BlockReason == MspVerifiedRuntimeBlockReason.None
        ? null
        : MspVerifiedRuntimeProviderContractEvaluator.GetBlockReasonMessage(BlockReason);
}

/// <summary>
/// Pure provider contract evaluator. It is intentionally independent of
/// Windows, Linux, Android, process APIs, filesystem APIs, and product policy.
/// </summary>
public static class MspVerifiedRuntimeProviderContractEvaluator
{
    private static readonly HashSet<string> ForbiddenEnvironmentKeys = new(StringComparer.OrdinalIgnoreCase)
    {
        "PATH",
        "PATHEXT",
        "COMSPEC",
        "SHELL",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH"
    };

    private static readonly HashSet<string> ShellEntryPoints = new(StringComparer.OrdinalIgnoreCase)
    {
        "sh",
        "bash",
        "zsh",
        "fish",
        "cmd",
        "cmd.exe",
        "powershell",
        "powershell.exe",
        "pwsh",
        "pwsh.exe"
    };

    /// <summary>
    /// Evaluates evidence and registers a provider contract without loading a
    /// bundle or invoking a platform backend. A blocked result never contains
    /// a usable contract.
    /// </summary>
    public static MspVerifiedRuntimeProviderRegistrationResult Register(
        MspVerifiedRuntimeProviderRegistrationRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);

        if (!IsValidBundleEvidence(request.Bundle) ||
            !IsSafeIdentifier(request.ExpectedBundleId) ||
            !IsSha256(request.ExpectedManifestSha256) ||
            !string.Equals(request.Bundle.BundleId, request.ExpectedBundleId, StringComparison.Ordinal) ||
            !string.Equals(request.Bundle.ManifestSha256, request.ExpectedManifestSha256, StringComparison.OrdinalIgnoreCase) ||
            !string.Equals(request.Bundle.Rid, request.TargetRid, StringComparison.OrdinalIgnoreCase) ||
            request.Bundle.Architecture != request.TargetArchitecture ||
            !IsValidSecurityPolicy(request.SecurityPolicy))
        {
            return new MspVerifiedRuntimeProviderRegistrationResult
            {
                Status = MspVerifiedRuntimeProviderRegistrationStatus.Blocked,
                BlockReason = MspVerifiedRuntimeBlockReason.CapabilityBlocked,
                Bundle = null,
                Contract = null
            };
        }

        var contract = new MspVerifiedRuntimeProviderContract
        {
            ProviderId = request.ProviderId,
            Bundle = request.Bundle,
            SecurityPolicy = request.SecurityPolicy,
            Profiles = request.Profiles,
            WorkspaceMode = request.WorkspaceMode,
            Limits = request.Limits,
            Lifecycle = request.Lifecycle,
            Provenance = request.Provenance
        };

        var validation = CreateLaunchPlan(
            contract,
            new MspVerifiedRuntimeLaunchRequest
            {
                ProfileId = request.Profiles?.FirstOrDefault()?.Id ?? string.Empty,
                Arguments = request.Profiles?.FirstOrDefault()?.ArgumentPrefix ??
                    Array.Empty<string>()
            });
        if (!validation.IsReady)
        {
            return new MspVerifiedRuntimeProviderRegistrationResult
            {
                Status = MspVerifiedRuntimeProviderRegistrationStatus.Blocked,
                BlockReason = validation.BlockReason,
                Bundle = request.Bundle,
                Contract = null
            };
        }

        return new MspVerifiedRuntimeProviderRegistrationResult
        {
            Status = MspVerifiedRuntimeProviderRegistrationStatus.Verified,
            BlockReason = MspVerifiedRuntimeBlockReason.None,
            Bundle = request.Bundle,
            Contract = contract
        };
    }

    public static MspVerifiedRuntimePlanResult CreateLaunchPlan(
        MspVerifiedRuntimeProviderContract provider,
        MspVerifiedRuntimeLaunchRequest request)
    {
        ArgumentNullException.ThrowIfNull(provider);
        ArgumentNullException.ThrowIfNull(request);

        if (!IsSafeIdentifier(provider.ProviderId))
        {
            return Blocked(MspVerifiedRuntimeBlockReason.ProviderIdInvalid);
        }

        if (!IsValidBundleEvidence(provider.Bundle) ||
            !IsValidSecurityPolicy(provider.SecurityPolicy))
        {
            return Blocked(MspVerifiedRuntimeBlockReason.CapabilityBlocked);
        }

        var bundle = provider.Bundle;
        if (!IsSafeIdentifier(bundle.BundleId) ||
            !IsSha256(bundle.ManifestSha256) ||
            !IsSafeIdentifier(bundle.Rid) ||
            bundle.Architecture == MspVerifiedRuntimeArchitecture.Unspecified)
        {
            return Blocked(MspVerifiedRuntimeBlockReason.CapabilityIdentityMissing);
        }

        if (provider.Profiles is null || provider.Profiles.Count == 0)
        {
            return Blocked(MspVerifiedRuntimeBlockReason.ProviderProfilesMissing);
        }

        if (provider.Limits is null)
        {
            return Blocked(MspVerifiedRuntimeBlockReason.LimitsInvalid);
        }

        var profileResult = ValidateProfiles(provider.Profiles, provider.Limits);
        if (profileResult != MspVerifiedRuntimeBlockReason.None)
        {
            return Blocked(profileResult);
        }

        if (provider.WorkspaceMode == MspVerifiedRuntimeWorkspaceMode.Unspecified)
        {
            return Blocked(MspVerifiedRuntimeBlockReason.WorkspacePolicyMissing);
        }

        if (provider.Lifecycle is null)
        {
            return Blocked(MspVerifiedRuntimeBlockReason.LifecyclePolicyMissing);
        }

        if (!provider.Lifecycle.CancellationRequired)
        {
            return Blocked(MspVerifiedRuntimeBlockReason.CancellationPolicyMissing);
        }

        if (!provider.Lifecycle.ProcessTreeCleanupRequired)
        {
            return Blocked(MspVerifiedRuntimeBlockReason.CleanupPolicyMissing);
        }

        if (!ValidateLimits(provider.Limits))
        {
            return Blocked(MspVerifiedRuntimeBlockReason.LimitsInvalid);
        }

        if (provider.Provenance is null)
        {
            return Blocked(MspVerifiedRuntimeBlockReason.ProvenanceMissing);
        }

        if (!ValidateProvenance(provider.Provenance))
        {
            return Blocked(MspVerifiedRuntimeBlockReason.ProvenanceInvalid);
        }

        if (!IsSafeIdentifier(request.ProfileId))
        {
            return Blocked(MspVerifiedRuntimeBlockReason.RequestProfileIdInvalid);
        }

        var profile = provider.Profiles.FirstOrDefault(
            candidate => string.Equals(candidate.Id, request.ProfileId, StringComparison.Ordinal));
        if (profile is null)
        {
            return Blocked(MspVerifiedRuntimeBlockReason.ProfileMissing);
        }

        if (!IsValidVirtualPath(request.VirtualCwd, allowRoot: true))
        {
            return Blocked(MspVerifiedRuntimeBlockReason.VirtualCwdInvalid);
        }

        var arguments = request.Arguments ?? Array.Empty<string>();
        if (profile.ArgumentPrefix is null ||
            profile.ArgumentPrefix.Count > profile.MaximumArguments ||
            profile.ArgumentPrefix.Any(argument => !IsValidArgument(
                argument,
                provider.Limits.MaximumArgumentBytes)))
        {
            return Blocked(MspVerifiedRuntimeBlockReason.ArgumentPrefixInvalid);
        }

        if (arguments.Count < profile.ArgumentPrefix.Count ||
            !arguments.Take(profile.ArgumentPrefix.Count).SequenceEqual(
                profile.ArgumentPrefix,
                StringComparer.Ordinal) ||
            arguments.Count > provider.Limits.MaximumArguments ||
            arguments.Count > profile.MaximumArguments)
        {
            return arguments.Count < profile.ArgumentPrefix.Count ||
                !arguments.Take(profile.ArgumentPrefix.Count).SequenceEqual(
                    profile.ArgumentPrefix,
                    StringComparer.Ordinal)
                ? Blocked(MspVerifiedRuntimeBlockReason.ArgumentPrefixMismatch)
                : Blocked(MspVerifiedRuntimeBlockReason.ArgumentLimitExceeded);
        }

        foreach (var argument in arguments.Skip(profile.ArgumentPrefix.Count))
        {
            if (!IsValidArgument(argument, provider.Limits.MaximumArgumentBytes))
            {
                return Blocked(MspVerifiedRuntimeBlockReason.ArgumentInvalid);
            }

            if (profile.ArgumentPolicy == MspVerifiedRuntimeArgumentPolicy.VirtualWorkspaceOnly)
            {
                if (!IsValidVirtualPath(argument, allowRoot: true))
                {
                    return Blocked(MspVerifiedRuntimeBlockReason.HostPathArgument);
                }
            }
            else if (LooksLikeHostPath(argument))
            {
                return Blocked(MspVerifiedRuntimeBlockReason.HostPathArgument);
            }
        }

        var environment = request.Environment ??
            new ReadOnlyDictionary<string, string>(new Dictionary<string, string>(StringComparer.Ordinal));
        if (environment.Count > provider.Limits.MaximumEnvironmentEntries)
        {
            return Blocked(MspVerifiedRuntimeBlockReason.EnvironmentLimitExceeded);
        }

        var copiedEnvironment = new SortedDictionary<string, string>(StringComparer.Ordinal);
        foreach (var pair in environment)
        {
            if (!IsSafeEnvironmentKey(pair.Key))
            {
                return Blocked(ForbiddenEnvironmentKeys.Contains(pair.Key ?? string.Empty)
                    ? MspVerifiedRuntimeBlockReason.HostEnvironmentKey
                    : MspVerifiedRuntimeBlockReason.EnvironmentKeyInvalid);
            }

            if (!IsValidEnvironmentValue(pair.Value, provider.Limits.MaximumEnvironmentValueBytes) ||
                LooksLikeHostPath(pair.Value))
            {
                return Blocked(MspVerifiedRuntimeBlockReason.EnvironmentValueInvalid);
            }

            copiedEnvironment[pair.Key] = pair.Value;
        }

        return new MspVerifiedRuntimePlanResult
        {
            Status = MspVerifiedRuntimePlanStatus.Ready,
            BlockReason = MspVerifiedRuntimeBlockReason.None,
            Plan = new MspVerifiedRuntimeLaunchPlan
            {
                ProviderId = provider.ProviderId,
                ProfileId = profile.Id,
                Runtime = bundle.Runtime,
                BundleId = bundle.BundleId!,
                ManifestSha256 = bundle.ManifestSha256!,
                TargetRid = bundle.Rid!,
                TargetArchitecture = bundle.Architecture,
                BundleRelativeEntryPoint = profile.BundleRelativeEntryPoint,
                ExecutableSha256 = profile.ExecutableSha256,
                VirtualCwd = request.VirtualCwd,
                Arguments = Array.AsReadOnly(arguments.ToArray()),
                Environment = new ReadOnlyDictionary<string, string>(copiedEnvironment),
                WorkspaceMode = provider.WorkspaceMode,
                Network = provider.SecurityPolicy.Network,
                Limits = provider.Limits,
                Lifecycle = provider.Lifecycle,
                Provenance = provider.Provenance
            }
        };
    }

    /// <summary>
    /// Revalidates bundle evidence immediately before a platform backend
    /// launches. A plan is never usable after identity, RID, machine, or
    /// verification drift.
    /// </summary>
    public static MspVerifiedRuntimePlanResult RevalidateIdentity(
        MspVerifiedRuntimeLaunchPlan plan,
        MspVerifiedRuntimeLaunchIdentityObservation observed)
    {
        ArgumentNullException.ThrowIfNull(plan);
        ArgumentNullException.ThrowIfNull(observed);

        var matches = observed.Bundle.Verification == MspVerifiedRuntimeBundleVerification.Verified &&
            observed.Bundle.Runtime == plan.Runtime &&
            string.Equals(observed.Bundle.BundleId, plan.BundleId, StringComparison.Ordinal) &&
            string.Equals(observed.Bundle.ManifestSha256, plan.ManifestSha256, StringComparison.OrdinalIgnoreCase) &&
            string.Equals(observed.Bundle.Rid, plan.TargetRid, StringComparison.OrdinalIgnoreCase) &&
            observed.Bundle.Architecture == plan.TargetArchitecture &&
            string.Equals(observed.ExecutableSha256, plan.ExecutableSha256, StringComparison.OrdinalIgnoreCase) &&
            IsSha256(observed.ExecutableSha256);

        return matches
            ? new MspVerifiedRuntimePlanResult
            {
                Status = MspVerifiedRuntimePlanStatus.Ready,
                BlockReason = MspVerifiedRuntimeBlockReason.None,
                Plan = plan
            }
            : Blocked(MspVerifiedRuntimeBlockReason.IdentityChanged);
    }

    internal static string GetBlockReasonCode(MspVerifiedRuntimeBlockReason reason)
    {
        return $"msp.provider.plan.{reason switch
        {
            MspVerifiedRuntimeBlockReason.ProviderIdInvalid => "provider_id_invalid",
            MspVerifiedRuntimeBlockReason.CapabilityBlocked => "capability_blocked",
            MspVerifiedRuntimeBlockReason.CapabilityIdentityMissing => "capability_identity_missing",
            MspVerifiedRuntimeBlockReason.ProviderProfilesMissing => "profiles_missing",
            MspVerifiedRuntimeBlockReason.DuplicateProfile => "duplicate_profile",
            MspVerifiedRuntimeBlockReason.ProfileIdInvalid => "profile_id_invalid",
            MspVerifiedRuntimeBlockReason.EntryPointInvalid => "entrypoint_invalid",
            MspVerifiedRuntimeBlockReason.ShellEntryPointForbidden => "shell_entrypoint_forbidden",
            MspVerifiedRuntimeBlockReason.ExecutableIdentityMissing => "executable_identity_missing",
            MspVerifiedRuntimeBlockReason.ProfileArgumentLimitInvalid => "profile_argument_limit_invalid",
            MspVerifiedRuntimeBlockReason.WorkspacePolicyMissing => "workspace_policy_missing",
            MspVerifiedRuntimeBlockReason.LifecyclePolicyMissing => "lifecycle_policy_missing",
            MspVerifiedRuntimeBlockReason.CancellationPolicyMissing => "cancellation_policy_missing",
            MspVerifiedRuntimeBlockReason.CleanupPolicyMissing => "cleanup_policy_missing",
            MspVerifiedRuntimeBlockReason.LimitsInvalid => "limits_invalid",
            MspVerifiedRuntimeBlockReason.ProvenanceMissing => "provenance_missing",
            MspVerifiedRuntimeBlockReason.ProvenanceInvalid => "provenance_invalid",
            MspVerifiedRuntimeBlockReason.ProfileMissing => "profile_missing",
            MspVerifiedRuntimeBlockReason.RequestProfileIdInvalid => "request_profile_id_invalid",
            MspVerifiedRuntimeBlockReason.ArgumentLimitExceeded => "argument_limit_exceeded",
            MspVerifiedRuntimeBlockReason.ArgumentPrefixInvalid => "argument_prefix_invalid",
            MspVerifiedRuntimeBlockReason.ArgumentPrefixMismatch => "argument_prefix_mismatch",
            MspVerifiedRuntimeBlockReason.ArgumentInvalid => "argument_invalid",
            MspVerifiedRuntimeBlockReason.HostPathArgument => "host_path_argument",
            MspVerifiedRuntimeBlockReason.VirtualCwdInvalid => "virtual_cwd_invalid",
            MspVerifiedRuntimeBlockReason.EnvironmentLimitExceeded => "environment_limit_exceeded",
            MspVerifiedRuntimeBlockReason.EnvironmentKeyInvalid => "environment_key_invalid",
            MspVerifiedRuntimeBlockReason.HostEnvironmentKey => "host_environment_key",
            MspVerifiedRuntimeBlockReason.EnvironmentValueInvalid => "environment_value_invalid",
            MspVerifiedRuntimeBlockReason.IdentityChanged => "identity_changed",
            MspVerifiedRuntimeBlockReason.ProviderAlreadyRegistered => "provider_already_registered",
            MspVerifiedRuntimeBlockReason.ProviderNotRegistered => "provider_not_registered",
            _ => "blocked"
        }}";
    }

    internal static string GetBlockReasonMessage(MspVerifiedRuntimeBlockReason reason)
    {
        return reason switch
        {
            MspVerifiedRuntimeBlockReason.CapabilityBlocked => "The verified runtime capability is not usable.",
            MspVerifiedRuntimeBlockReason.EntryPointInvalid => "The provider entry point is not a safe bundle-relative executable.",
            MspVerifiedRuntimeBlockReason.ShellEntryPointForbidden => "Shell wrapper entry points are not permitted.",
            MspVerifiedRuntimeBlockReason.ExecutableIdentityMissing => "The provider executable has no verified digest.",
            MspVerifiedRuntimeBlockReason.HostPathArgument => "Provider arguments must not contain host paths.",
            MspVerifiedRuntimeBlockReason.ArgumentPrefixInvalid => "The provider profile has an invalid fixed argument prefix.",
            MspVerifiedRuntimeBlockReason.ArgumentPrefixMismatch => "The launch arguments do not match the registered profile prefix.",
            MspVerifiedRuntimeBlockReason.HostEnvironmentKey => "Host PATH and loader environment keys are not permitted.",
            MspVerifiedRuntimeBlockReason.IdentityChanged => "The verified runtime identity changed before launch.",
            MspVerifiedRuntimeBlockReason.CancellationPolicyMissing => "Provider cancellation is not explicitly required.",
            MspVerifiedRuntimeBlockReason.CleanupPolicyMissing => "Process-tree cleanup is not explicitly required.",
            MspVerifiedRuntimeBlockReason.ProviderAlreadyRegistered => "The provider id is already registered.",
            MspVerifiedRuntimeBlockReason.ProviderNotRegistered => "The requested provider is not registered.",
            _ => "The verified runtime launch plan is blocked."
        };
    }

    private static MspVerifiedRuntimePlanResult Blocked(MspVerifiedRuntimeBlockReason reason)
    {
        return new MspVerifiedRuntimePlanResult
        {
            Status = MspVerifiedRuntimePlanStatus.Blocked,
            BlockReason = reason,
            Plan = null
        };
    }

    private static MspVerifiedRuntimeBlockReason ValidateProfiles(
        IReadOnlyList<MspVerifiedRuntimeCommandProfile> profiles,
        MspVerifiedRuntimeLimits limits)
    {
        var seen = new HashSet<string>(StringComparer.Ordinal);
        foreach (var profile in profiles)
        {
            if (profile is null || !IsSafeIdentifier(profile.Id))
            {
                return MspVerifiedRuntimeBlockReason.ProfileIdInvalid;
            }

            if (!seen.Add(profile.Id))
            {
                return MspVerifiedRuntimeBlockReason.DuplicateProfile;
            }

            if (!IsSafeBundleRelativeEntryPoint(profile.BundleRelativeEntryPoint))
            {
                return MspVerifiedRuntimeBlockReason.EntryPointInvalid;
            }

            if (ShellEntryPoints.Contains(GetLeaf(profile.BundleRelativeEntryPoint)))
            {
                return MspVerifiedRuntimeBlockReason.ShellEntryPointForbidden;
            }

            if (!IsSha256(profile.ExecutableSha256))
            {
                return MspVerifiedRuntimeBlockReason.ExecutableIdentityMissing;
            }

            if (profile.MaximumArguments <= 0 || profile.MaximumArguments > limits.MaximumArguments)
            {
                return MspVerifiedRuntimeBlockReason.ProfileArgumentLimitInvalid;
            }

            if (profile.ArgumentPrefix is null ||
                profile.ArgumentPrefix.Count > profile.MaximumArguments ||
                profile.ArgumentPrefix.Any(argument =>
                    !IsValidArgument(argument, limits.MaximumArgumentBytes) ||
                    LooksLikeHostPath(argument)))
            {
                return MspVerifiedRuntimeBlockReason.ArgumentPrefixInvalid;
            }
        }

        return MspVerifiedRuntimeBlockReason.None;
    }

    private static bool ValidateLimits(MspVerifiedRuntimeLimits? limits)
    {
        if (limits is null)
        {
            return false;
        }

        return limits.MaximumArguments is > 0 and <= MspVerifiedRuntimeLimits.DefaultMaximumArguments &&
            limits.MaximumArgumentBytes is > 0 and <= MspVerifiedRuntimeLimits.DefaultMaximumArgumentBytes &&
            limits.MaximumEnvironmentEntries is > 0 and <= MspVerifiedRuntimeLimits.DefaultMaximumEnvironmentEntries &&
            limits.MaximumEnvironmentValueBytes is > 0 and <= MspVerifiedRuntimeLimits.DefaultMaximumEnvironmentValueBytes &&
            limits.MaximumStdoutBytes is > 0 and <= MspVerifiedRuntimeLimits.DefaultMaximumStdoutBytes &&
            limits.MaximumStderrBytes is > 0 and <= MspVerifiedRuntimeLimits.DefaultMaximumStderrBytes &&
            limits.MaximumWallClockMilliseconds is > 0 and <= MspVerifiedRuntimeLimits.DefaultMaximumWallClockMilliseconds &&
            limits.MaximumMemoryBytes is > 0 and <= MspVerifiedRuntimeLimits.DefaultMaximumMemoryBytes &&
            limits.MaximumChildProcesses is > 0 and <= MspVerifiedRuntimeLimits.DefaultMaximumChildProcesses &&
            limits.MaximumWorkspaceFiles is > 0 and <= MspVerifiedRuntimeLimits.DefaultMaximumWorkspaceFiles;
    }

    private static bool ValidateProvenance(MspVerifiedRuntimeProvenance? provenance)
    {
        return provenance is not null &&
            IsSafeIdentifier(provenance.LicenseId) &&
            IsSafeIdentifier(provenance.NoticeId);
    }

    private static bool IsValidBundleEvidence(MspVerifiedRuntimeBundleEvidence? bundle)
    {
        return bundle is not null &&
            IsSupportedRuntime(bundle.Runtime) &&
            bundle.Verification == MspVerifiedRuntimeBundleVerification.Verified &&
            IsSafeIdentifier(bundle.BundleId) &&
            IsSha256(bundle.ManifestSha256) &&
            IsSafeIdentifier(bundle.Rid) &&
            bundle.Architecture != MspVerifiedRuntimeArchitecture.Unspecified;
    }

    private static bool IsValidSecurityPolicy(MspVerifiedRuntimeSecurityPolicy? policy)
    {
        return policy is not null &&
            policy.Scratch != MspVerifiedRuntimeScratchMode.Unspecified &&
            policy.PathResolution == MspVerifiedRuntimePathResolutionPolicy.BundleOnly &&
            policy.Network == MspVerifiedRuntimeNetworkPolicy.Disabled;
    }

    private static bool IsSupportedRuntime(MspNativeRuntimeKind runtime)
    {
        return runtime is MspNativeRuntimeKind.Python or
            MspNativeRuntimeKind.Node or
            MspNativeRuntimeKind.Git or
            MspNativeRuntimeKind.Toybox;
    }

    private static bool IsSafeIdentifier(string? value)
    {
        if (string.IsNullOrEmpty(value) || value.Length > 128)
        {
            return false;
        }

        return value.All(character => char.IsAsciiLetterOrDigit(character) || character is '-' or '_' or '.');
    }

    private static bool IsSha256(string? value)
    {
        return value is { Length: 64 } && value.All(Uri.IsHexDigit);
    }

    private static bool IsSafeBundleRelativeEntryPoint(string? value)
    {
        if (string.IsNullOrEmpty(value) || value.Length > 256 || value.StartsWith('/') ||
            value.Contains('\\') || value.Contains(':') || value.Contains('\0') ||
            value.Any(char.IsControl) || value.Split('/').Any(component => component is "" or "." or ".."))
        {
            return false;
        }

        return value.Split('/').All(component =>
            component.All(character => char.IsAsciiLetterOrDigit(character) || character is '-' or '_' or '.'));
    }

    private static string GetLeaf(string? value)
    {
        return value?.Split('/').LastOrDefault() ?? string.Empty;
    }

    private static bool IsValidArgument(string? value, int maximumBytes)
    {
        if (value is null || value.Any(char.IsControl) || value.Contains('\0'))
        {
            return false;
        }

        try
        {
            return Encoding.UTF8.GetByteCount(value) <= maximumBytes;
        }
        catch (ArgumentException)
        {
            return false;
        }
    }

    private static bool LooksLikeHostPath(string value)
    {
        return value.StartsWith("/", StringComparison.Ordinal) ||
            value.StartsWith("\\\\", StringComparison.Ordinal) ||
            value.StartsWith("file:", StringComparison.OrdinalIgnoreCase) ||
            value.Length >= 3 && char.IsAsciiLetter(value[0]) && value[1] == ':' && value[2] is '\\' or '/';
    }

    private static bool IsValidVirtualPath(string? value, bool allowRoot)
    {
        if (string.IsNullOrEmpty(value) || !value.StartsWith('/') || value.Contains('\\') ||
            value.Contains(':') || value.Contains("//", StringComparison.Ordinal) ||
            value.Contains('\0') || value.Any(char.IsControl) || !allowRoot && value == "/")
        {
            return false;
        }

        return value.Split('/').Skip(1).All(component => component is not ("." or ".." or "") &&
            !string.Equals(component, ".msp", StringComparison.OrdinalIgnoreCase));
    }

    private static bool IsSafeEnvironmentKey(string? value)
    {
        return value is { Length: > 0 and <= 64 } &&
            !ForbiddenEnvironmentKeys.Contains(value) &&
            char.IsAsciiLetter(value[0]) &&
            value.All(character => char.IsAsciiLetterOrDigit(character) || character == '_');
    }

    private static bool IsValidEnvironmentValue(string? value, int maximumBytes)
    {
        return value is not null && !value.Any(char.IsControl) && !value.Contains('\0') &&
            Encoding.UTF8.GetByteCount(value) <= maximumBytes;
    }
}
