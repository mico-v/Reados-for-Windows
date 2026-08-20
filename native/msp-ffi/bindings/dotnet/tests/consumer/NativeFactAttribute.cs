using MspFfi;
using Xunit;

namespace MspFfi.Consumer.Tests;

internal sealed class NativeFactAttribute : FactAttribute
{
    public NativeFactAttribute()
    {
        var path = Environment.GetEnvironmentVariable(
            MspRuntime.NativeLibraryPathEnvironmentVariable);
        Skip = string.IsNullOrWhiteSpace(path)
            ? $"Expected skip: set {MspRuntime.NativeLibraryPathEnvironmentVariable} to an explicit msp_ffi DLL path."
            : !File.Exists(path)
                ? $"Expected skip: explicitly supplied native DLL was not found: {path}"
                : null;
    }
}
