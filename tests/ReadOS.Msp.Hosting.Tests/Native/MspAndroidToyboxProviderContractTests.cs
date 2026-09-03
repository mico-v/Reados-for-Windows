using ReadOS.Msp.Hosting.Native;

namespace ReadOS.Msp.Hosting.Tests.Native;

public sealed class MspAndroidToyboxProviderContractTests
{
    private const string ExecutableSha256 =
        "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd";

    [Fact]
    public void Registration_exposes_fixed_toybox_profiles_without_enabling_a_shell()
    {
        var result = MspVerifiedRuntimeProviderContractEvaluator.Register(
            MspAndroidToyboxProviderContract.CreateRegistration(
                "android-arm64",
                MspVerifiedRuntimeArchitecture.Arm64,
                ExecutableSha256));

        Assert.True(result.IsUsable);
        var contract = Assert.IsType<MspVerifiedRuntimeProviderContract>(result.Contract);
        Assert.Equal(MspNativeRuntimeKind.Toybox, contract.Bundle.Runtime);
        Assert.Equal(["pwd"], contract.Profiles.Single(profile => profile.Id == "pwd").ArgumentPrefix);
        Assert.Equal(["cat"], contract.Profiles.Single(profile => profile.Id == "cat").ArgumentPrefix);
        Assert.Equal(["ls", "-1"], contract.Profiles.Single(profile => profile.Id == "ls").ArgumentPrefix);
        Assert.All(contract.Profiles, profile => Assert.Equal("toybox", profile.BundleRelativeEntryPoint));
    }

    [Fact]
    public void Catalog_selects_a_profile_and_preserves_virtual_workspace_only_suffix()
    {
        var catalog = new MspVerifiedRuntimeProviderCatalog();
        var registration = catalog.Register(
            MspAndroidToyboxProviderContract.CreateRegistration(
                "android-arm64",
                MspVerifiedRuntimeArchitecture.Arm64,
                ExecutableSha256));

        Assert.True(registration.IsUsable);

        var ready = catalog.CreateLaunchPlan(
            MspAndroidToyboxProviderContract.ProviderId,
            new MspVerifiedRuntimeLaunchRequest
            {
                ProfileId = "cat",
                VirtualCwd = "/workspace",
                Arguments = ["cat", "/workspace/input.bin"]
            });
        Assert.True(ready.IsReady);
        Assert.Equal(MspNativeRuntimeKind.Toybox, ready.Plan!.Runtime);

        var hostPath = catalog.CreateLaunchPlan(
            MspAndroidToyboxProviderContract.ProviderId,
            new MspVerifiedRuntimeLaunchRequest
            {
                ProfileId = "cat",
                Arguments = ["cat", @"C:\private\input.bin"]
            });
        Assert.Equal(MspVerifiedRuntimeBlockReason.HostPathArgument, hostPath.BlockReason);

        var wrongPrefix = catalog.CreateLaunchPlan(
            MspAndroidToyboxProviderContract.ProviderId,
            new MspVerifiedRuntimeLaunchRequest
            {
                ProfileId = "cat",
                Arguments = ["ls", "/workspace/input.bin"]
            });
        Assert.Equal(MspVerifiedRuntimeBlockReason.ArgumentPrefixMismatch, wrongPrefix.BlockReason);
    }

    [Fact]
    public void Catalog_rejects_duplicate_registration_and_unknown_provider()
    {
        var catalog = new MspVerifiedRuntimeProviderCatalog();
        var request = MspAndroidToyboxProviderContract.CreateRegistration(
            "android-x86_64",
            MspVerifiedRuntimeArchitecture.X64,
            ExecutableSha256);

        Assert.True(catalog.Register(request).IsUsable);
        var duplicate = catalog.Register(request);
        Assert.Equal(MspVerifiedRuntimeBlockReason.ProviderAlreadyRegistered, duplicate.BlockReason);

        var missing = catalog.CreateLaunchPlan(
            "not-registered",
            new MspVerifiedRuntimeLaunchRequest { ProfileId = "pwd" });
        Assert.Equal(MspVerifiedRuntimeBlockReason.ProviderNotRegistered, missing.BlockReason);
        Assert.Single(catalog.Snapshot());
    }
}
