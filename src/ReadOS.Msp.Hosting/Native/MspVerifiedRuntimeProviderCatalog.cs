using System.Collections.ObjectModel;

namespace ReadOS.Msp.Hosting.Native;

/// <summary>
/// Host-owned registry of verified external runtime providers. Registration is
/// still evidence-only; execution, policy/approval, process lifecycle, and
/// product audit remain owned by their existing layers.
/// </summary>
public sealed class MspVerifiedRuntimeProviderCatalog
{
    private readonly object gate = new();
    private readonly Dictionary<string, MspVerifiedRuntimeProviderContract> providers =
        new(StringComparer.Ordinal);

    public MspVerifiedRuntimeProviderRegistrationResult Register(
        MspVerifiedRuntimeProviderRegistrationRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);

        if (string.IsNullOrEmpty(request.ProviderId))
        {
            return MspVerifiedRuntimeProviderContractEvaluator.Register(request);
        }

        lock (gate)
        {
            if (providers.ContainsKey(request.ProviderId))
            {
                return new MspVerifiedRuntimeProviderRegistrationResult
                {
                    Status = MspVerifiedRuntimeProviderRegistrationStatus.Blocked,
                    BlockReason = MspVerifiedRuntimeBlockReason.ProviderAlreadyRegistered,
                    Bundle = null,
                    Contract = null
                };
            }

            var result = MspVerifiedRuntimeProviderContractEvaluator.Register(request);
            if (result.IsUsable)
            {
                var contract = Freeze(result.Contract!);
                providers.Add(request.ProviderId, contract);
                return result with { Contract = contract };
            }

            return result;
        }
    }

    public bool TryGet(
        string providerId,
        out MspVerifiedRuntimeProviderContract? contract)
    {
        if (string.IsNullOrWhiteSpace(providerId))
        {
            contract = null;
            return false;
        }

        lock (gate)
        {
            return providers.TryGetValue(providerId, out contract);
        }
    }

    public MspVerifiedRuntimePlanResult CreateLaunchPlan(
        string providerId,
        MspVerifiedRuntimeLaunchRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);

        if (!TryGet(providerId, out var provider) || provider is null)
        {
            return new MspVerifiedRuntimePlanResult
            {
                Status = MspVerifiedRuntimePlanStatus.Blocked,
                BlockReason = MspVerifiedRuntimeBlockReason.ProviderNotRegistered,
                Plan = null
            };
        }

        return MspVerifiedRuntimeProviderContractEvaluator.CreateLaunchPlan(provider, request);
    }

    public IReadOnlyList<MspVerifiedRuntimeProviderContract> Snapshot()
    {
        lock (gate)
        {
            return new ReadOnlyCollection<MspVerifiedRuntimeProviderContract>(
                providers.Values.ToArray());
        }
    }

    private static MspVerifiedRuntimeProviderContract Freeze(
        MspVerifiedRuntimeProviderContract contract)
    {
        var profiles = contract.Profiles
            .Select(profile => profile with
            {
                ArgumentPrefix = new ReadOnlyCollection<string>(
                    (profile.ArgumentPrefix ?? Array.Empty<string>()).ToArray())
            })
            .ToArray();
        return contract with
        {
            Profiles = new ReadOnlyCollection<MspVerifiedRuntimeCommandProfile>(profiles)
        };
    }
}
