namespace ReadOS.Msp.Hosting.Native;

/// <summary>
/// Creates the Host-side declaration for the Android fixed-profile Toybox
/// provider. The Android platform layer supplies the verified executable
/// digest and target ABI; this helper never discovers a binary or launches it.
/// </summary>
public static class MspAndroidToyboxProviderContract
{
    public const string ProviderId = "reados-android-tool-provider";
    public const string BundleId = "reados-android-tool-provider-1";
    public const string ManifestSha256 =
        "a528dba0aee524f1a6e1a03f2b8db6c9204ceda58409219e95bbb80c2302175f";
    public const string LicenseId = "0BSD";
    public const string NoticeId = "reados-android-tool-provider-notice-1";

    public static MspVerifiedRuntimeProviderRegistrationRequest CreateRegistration(
        string targetRid,
        MspVerifiedRuntimeArchitecture targetArchitecture,
        string executableSha256)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(targetRid);
        ArgumentException.ThrowIfNullOrWhiteSpace(executableSha256);

        var profiles = new[]
        {
            Profile("pwd", ["pwd"], MspVerifiedRuntimeArgumentPolicy.Opaque, 1, executableSha256),
            Profile("cat", ["cat"], MspVerifiedRuntimeArgumentPolicy.VirtualWorkspaceOnly, 2, executableSha256),
            Profile("ls", ["ls", "-1"], MspVerifiedRuntimeArgumentPolicy.VirtualWorkspaceOnly, 3, executableSha256)
        };

        return new MspVerifiedRuntimeProviderRegistrationRequest
        {
            ProviderId = ProviderId,
            Bundle = new MspVerifiedRuntimeBundleEvidence
            {
                Runtime = MspNativeRuntimeKind.Toybox,
                BundleId = BundleId,
                ManifestSha256 = ManifestSha256,
                Rid = targetRid,
                Architecture = targetArchitecture,
                Verification = MspVerifiedRuntimeBundleVerification.Verified
            },
            ExpectedBundleId = BundleId,
            ExpectedManifestSha256 = ManifestSha256,
            TargetRid = targetRid,
            TargetArchitecture = targetArchitecture,
            SecurityPolicy = new MspVerifiedRuntimeSecurityPolicy
            {
                Scratch = MspVerifiedRuntimeScratchMode.DedicatedVirtualWorkspace,
                PathResolution = MspVerifiedRuntimePathResolutionPolicy.BundleOnly,
                Network = MspVerifiedRuntimeNetworkPolicy.Disabled
            },
            Profiles = profiles,
            WorkspaceMode = MspVerifiedRuntimeWorkspaceMode.ProjectedReadOnly,
            Limits = new MspVerifiedRuntimeLimits
            {
                MaximumArguments = 3,
                MaximumArgumentBytes = 4096,
                MaximumEnvironmentEntries = 2,
                MaximumEnvironmentValueBytes = 64,
                MaximumStdoutBytes = 2 * 1024 * 1024,
                MaximumStderrBytes = 2 * 1024 * 1024,
                MaximumWallClockMilliseconds = 30_000,
                MaximumMemoryBytes = 128 * 1024 * 1024,
                MaximumChildProcesses = 1,
                MaximumWorkspaceFiles = 65_536
            },
            Lifecycle = new MspVerifiedRuntimeLifecyclePolicy
            {
                CancellationRequired = true,
                ProcessTreeCleanupRequired = true
            },
            Provenance = new MspVerifiedRuntimeProvenance
            {
                LicenseId = LicenseId,
                NoticeId = NoticeId
            }
        };
    }

    private static MspVerifiedRuntimeCommandProfile Profile(
        string id,
        IReadOnlyList<string> prefix,
        MspVerifiedRuntimeArgumentPolicy argumentPolicy,
        int maximumArguments,
        string executableSha256)
    {
        return new MspVerifiedRuntimeCommandProfile
        {
            Id = id,
            BundleRelativeEntryPoint = "toybox",
            ExecutableSha256 = executableSha256,
            ArgumentPrefix = prefix,
            ArgumentPolicy = argumentPolicy,
            MaximumArguments = maximumArguments
        };
    }
}
