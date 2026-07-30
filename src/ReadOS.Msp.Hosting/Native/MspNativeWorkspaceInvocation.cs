using System.Collections.ObjectModel;
using System.Text;
using ReadOS.Msp.Workspace;

namespace ReadOS.Msp.Hosting.Native;

public sealed record MspNativeWorkspaceBackend
{
    public required ulong Id { get; init; }

    public required IMspNativeReadOnlyWorkspace Workspace { get; init; }
}

public sealed record MspNativeWorkspaceMount
{
    public required string Path { get; init; }

    public required MspNativeWorkspaceBackend Backend { get; init; }
}

/// <summary>
/// Immutable callback topology for one native invocation. A local host root, if
/// any, remains part of the command request and is validated against a callback
/// base later by the adapter; no host path is stored in this topology.
/// </summary>
public sealed class MspNativeWorkspaceInvocation
{
    private static readonly StringComparer PathComparer = StringComparer.Ordinal;

    private readonly IReadOnlyDictionary<ulong, IMspNativeReadOnlyWorkspace> backends;

    public MspNativeWorkspaceInvocation(
        MspNativeWorkspaceBackend? callbackBase = null,
        IEnumerable<MspNativeWorkspaceMount>? mounts = null)
    {
        var resolvedMounts = (mounts ?? Array.Empty<MspNativeWorkspaceMount>()).ToArray();
        if (resolvedMounts.Length > MspNativeWorkspaceAbiV1.MaximumMountCount)
        {
            throw new ArgumentOutOfRangeException(
                nameof(mounts),
                $"A native workspace invocation supports at most {MspNativeWorkspaceAbiV1.MaximumMountCount} mounts.");
        }

        if (callbackBase is null && resolvedMounts.Length == 0)
        {
            throw new ArgumentException(
                "A native workspace invocation requires a callback base or at least one mount.",
                nameof(mounts));
        }

        var backendMap = new Dictionary<ulong, IMspNativeReadOnlyWorkspace>();
        CallbackBase = callbackBase is null
            ? null
            : FreezeAndRegisterBackend(callbackBase, backendMap, nameof(callbackBase));

        var seenPaths = new HashSet<string>(PathComparer);
        var frozenMounts = new List<MspNativeWorkspaceMount>(resolvedMounts.Length);
        foreach (var mount in resolvedMounts)
        {
            ArgumentNullException.ThrowIfNull(mount);
            ArgumentException.ThrowIfNullOrWhiteSpace(mount.Path);
            ValidateMountPath(mount.Path, nameof(mounts));
            if (!seenPaths.Add(mount.Path))
            {
                throw new ArgumentException(
                    $"The native workspace mount path is duplicated: {mount.Path}",
                    nameof(mounts));
            }

            var backend = FreezeAndRegisterBackend(
                mount.Backend ?? throw new ArgumentException(
                    "A native workspace mount requires a backend.",
                    nameof(mounts)),
                backendMap,
                nameof(mounts));
            frozenMounts.Add(mount with
            {
                Path = mount.Path,
                Backend = backend
            });
        }

        frozenMounts.Sort(static (left, right) =>
        {
            var length = right.Path.Length.CompareTo(left.Path.Length);
            return length != 0
                ? length
                : StringComparer.Ordinal.Compare(left.Path, right.Path);
        });

        Mounts = Array.AsReadOnly(frozenMounts.ToArray());
        backends = new ReadOnlyDictionary<ulong, IMspNativeReadOnlyWorkspace>(backendMap);
    }

    public MspNativeWorkspaceBackend? CallbackBase { get; }

    public IReadOnlyList<MspNativeWorkspaceMount> Mounts { get; }

    internal bool TryGetBackend(
        ulong backendId,
        out IMspNativeReadOnlyWorkspace workspace)
    {
        return backends.TryGetValue(backendId, out workspace!);
    }

    private static MspNativeWorkspaceBackend FreezeAndRegisterBackend(
        MspNativeWorkspaceBackend backend,
        IDictionary<ulong, IMspNativeReadOnlyWorkspace> backends,
        string parameterName)
    {
        ArgumentNullException.ThrowIfNull(backend);
        if (backend.Id == 0)
        {
            throw new ArgumentOutOfRangeException(
                parameterName,
                "Native workspace backend identifiers must be nonzero.");
        }

        var workspace = backend.Workspace ?? throw new ArgumentException(
            "A native workspace backend requires a managed workspace.",
            parameterName);
        if (backends.TryGetValue(backend.Id, out var existing))
        {
            if (!ReferenceEquals(existing, workspace))
            {
                throw new ArgumentException(
                    $"Native workspace backend identifier {backend.Id} maps to multiple workspaces.",
                    parameterName);
            }
        }
        else
        {
            backends.Add(backend.Id, workspace);
        }

        return backend with { Workspace = workspace };
    }

    private static void ValidateMountPath(string path, string parameterName)
    {
        if (path.Contains('\0') ||
            path.Contains('\\') ||
            path.Any(char.IsControl) ||
            Encoding.UTF8.GetByteCount(path) > MspNativeWorkspaceAbiV1.MaximumPathBytes ||
            !path.StartsWith("/", StringComparison.Ordinal) ||
            path == "/" ||
            !string.Equals(MspPathUtility.Normalize(path), path, StringComparison.Ordinal))
        {
            throw new ArgumentException(
                "Native workspace mount paths must be normalized, non-root virtual paths.",
                parameterName);
        }

        if (path
            .Split('/', StringSplitOptions.RemoveEmptyEntries)
            .Any(component => string.Equals(component, ".msp", StringComparison.OrdinalIgnoreCase)))
        {
            throw new ArgumentException(
                "Native workspace mount paths must not target hidden workspace storage.",
                parameterName);
        }
    }
}
