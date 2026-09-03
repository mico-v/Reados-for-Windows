using ReadOS.Msp.Hosting.Native.RuntimeFfi;

namespace ReadOS.App.Services.Msp;

/// <summary>
/// Loads only the optional Runtime FFI artifact shipped beside the app. The
/// package directory is the trust boundary; no environment or PATH lookup is
/// used, and every load/ABI failure is fail-closed.
/// </summary>
internal static class ReadOsPackagedRuntimeFfiLoader
{
    public const string LibraryFileName = "msp_command_runtime_ffi.dll";

    public static string GetLibraryPath(string? baseDirectory = null)
    {
        var directory = string.IsNullOrWhiteSpace(baseDirectory)
            ? AppContext.BaseDirectory
            : baseDirectory;
        return Path.GetFullPath(Path.Combine(directory, LibraryFileName));
    }

    public static MspCommandRuntimeFfiEchoCommandAdapter? TryLoad(
        string? baseDirectory = null,
        Func<string, MspCommandRuntimeFfiLibrary>? load = null)
    {
        try
        {
            var path = GetLibraryPath(baseDirectory);
            if (!File.Exists(path))
            {
                return null;
            }

            var library = (load ?? (path => MspCommandRuntimeFfiLibrary.Load(path)))(path);
            if (library is null)
            {
                return null;
            }

            try
            {
                return new MspCommandRuntimeFfiEchoCommandAdapter(
                    library,
                    ownsAdapter: true);
            }
            catch
            {
                library.Dispose();
                throw;
            }
        }
        catch (Exception exception) when (exception is not OutOfMemoryException and not StackOverflowException)
        {
            return null;
        }
    }
}

public sealed class ReadOsPackagedRuntimeFfiRegistration
{
    public ReadOsPackagedRuntimeFfiRegistration(
        MspCommandRuntimeFfiEchoCommandAdapter? adapter)
    {
        Adapter = adapter;
    }

    /// <summary>
    /// The adapter is transferred to the single host that consumes this
    /// registration. The registration deliberately does not implement
    /// <see cref="IDisposable"/>; the host owns disposal so the native library
    /// cannot be released twice by both DI and the command host.
    /// </summary>
    public MspCommandRuntimeFfiEchoCommandAdapter? Adapter { get; }
}
