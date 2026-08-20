using System.Reflection;
using System.Runtime.InteropServices;

namespace MspFfi;

/// <summary>Controls explicit loading and ABI validation for the native library.</summary>
public static class MspRuntime
{
    public const uint ExpectedAbiVersion = 1;
    public const string ExpectedVersion = "0.1.0";
    public const string NativeLibraryPathEnvironmentVariable = "MSP_FFI_NATIVE_DLL";
    public const string NativeLibraryPathKey = "MspFfi.NativeLibraryPath";

    private static readonly object Gate = new();
    private static IntPtr nativeLibrary;
    private static string? nativeLibraryPath;

    static MspRuntime()
    {
        NativeLibrary.SetDllImportResolver(typeof(NativeMethods).Assembly, ResolveLibrary);
    }

    /// <summary>Gets the explicitly configured DLL path, or <see langword="null"/>.</summary>
    public static string? NativeLibraryPath => Volatile.Read(ref nativeLibraryPath);

    /// <summary>Gets whether an explicit native library has been loaded.</summary>
    public static bool IsNativeLibraryConfigured =>
        Volatile.Read(ref nativeLibrary) != IntPtr.Zero;

    /// <summary>
    /// Loads one native DLL selected by the caller. No default probing is performed.
    /// </summary>
    public static void UseNativeLibrary(string path)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(path);

        var fullPath = Path.GetFullPath(path);
        if (!File.Exists(fullPath))
        {
            throw new FileNotFoundException(
                "The explicitly supplied msp_ffi native DLL was not found.",
                fullPath);
        }

        lock (Gate)
        {
            if (nativeLibrary != IntPtr.Zero)
            {
                if (string.Equals(nativeLibraryPath, fullPath, StringComparison.OrdinalIgnoreCase))
                {
                    return;
                }

                throw new InvalidOperationException(
                    $"The msp_ffi native library is already configured as '{nativeLibraryPath}'.");
            }

            try
            {
                nativeLibrary = NativeLibrary.Load(fullPath);
            }
            catch (Exception exception) when (
                exception is DllNotFoundException
                    or BadImageFormatException
                    or FileLoadException
                    or UnauthorizedAccessException)
            {
                throw new MspNativeException(
                    $"Unable to load the explicitly supplied msp_ffi DLL '{fullPath}'.",
                    exception);
            }

            nativeLibraryPath = fullPath;
        }
    }

    /// <summary>Compatibility spelling for explicit native-library configuration.</summary>
    public static void ConfigureNativeLibrary(string path) => UseNativeLibrary(path);

    /// <summary>Compatibility spelling for explicit native-library configuration.</summary>
    public static void Load(string path) => UseNativeLibrary(path);

    /// <summary>
    /// Attempts explicit configuration without probing. A missing or unloadable
    /// supplied path returns <see langword="false"/>.
    /// </summary>
    public static bool TryUseNativeLibrary(string? path)
    {
        if (string.IsNullOrWhiteSpace(path))
        {
            return false;
        }

        try
        {
            UseNativeLibrary(path);
            return true;
        }
        catch (Exception exception) when (
            exception is ArgumentException
                or FileNotFoundException
                or MspNativeException
                or IOException
                or UnauthorizedAccessException)
        {
            return false;
        }
    }

    /// <summary>Reads and validates the ABI metadata from the configured DLL.</summary>
    public static MspRuntimeInfo GetInfo()
    {
        EnsureNativeConfigured();

        uint abiVersion;
        IntPtr versionPointer;
        try
        {
            abiVersion = NativeMethods.RuntimeAbiVersion();
            versionPointer = NativeMethods.RuntimeVersion();
        }
        catch (Exception exception) when (
            exception is DllNotFoundException
                or EntryPointNotFoundException
                or BadImageFormatException)
        {
            throw new MspNativeException(
                "The configured msp_ffi DLL does not expose the required runtime exports.",
                exception);
        }

        var version = versionPointer == IntPtr.Zero
            ? null
            : Marshal.PtrToStringUTF8(versionPointer);
        if (abiVersion != ExpectedAbiVersion || !string.Equals(
                version,
                ExpectedVersion,
                StringComparison.Ordinal))
        {
            throw new MspNativeException(
                $"Unsupported msp_ffi runtime ABI/version: {abiVersion}/{version ?? "<null>"}; "
                + $"expected {ExpectedAbiVersion}/{ExpectedVersion}.");
        }

        return new MspRuntimeInfo(abiVersion, version!);
    }

    internal static void EnsureNativeConfigured()
    {
        if (!IsNativeLibraryConfigured)
        {
            throw new DllNotFoundException(
                "No msp_ffi DLL was explicitly supplied. Call "
                + $"{nameof(UseNativeLibrary)} with its full path before creating a wrapper.");
        }
    }

    private static IntPtr ResolveLibrary(
        string libraryName,
        Assembly assembly,
        DllImportSearchPath? searchPath)
    {
        if (!string.Equals(libraryName, NativeMethods.LibraryName, StringComparison.Ordinal))
        {
            return IntPtr.Zero;
        }

        lock (Gate)
        {
            // Returning zero without an explicit handle permits the runtime to
            // probe. Public entry points call EnsureNativeConfigured first; this
            // resolver returns the loaded handle only after explicit selection.
            return nativeLibrary;
        }
    }
}
