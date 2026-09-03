using ReadOS.App.Services.Msp;
using ReadOS.Msp.Hosting.Native.RuntimeFfi;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsPackagedRuntimeFfiLoaderTests
{
    [Fact]
    public void GetLibraryPath_uses_the_fixed_application_base_directory_path()
    {
        var baseDirectory = Path.Combine(Path.GetTempPath(), "reados-loader-tests", "base");

        var path = ReadOsPackagedRuntimeFfiLoader.GetLibraryPath(baseDirectory);

        Assert.Equal(
            Path.GetFullPath(Path.Combine(baseDirectory, ReadOsPackagedRuntimeFfiLoader.LibraryFileName)),
            path);
    }

    [Fact]
    public void TryLoad_returns_null_when_the_packaged_dll_is_absent()
    {
        var baseDirectory = CreateTemporaryDirectory();
        try
        {
            var adapter = ReadOsPackagedRuntimeFfiLoader.TryLoad(baseDirectory);

            Assert.Null(adapter);
        }
        finally
        {
            TryDelete(baseDirectory);
        }
    }

    [Fact]
    public void TryLoad_returns_null_when_present_dll_fails_abi_validation()
    {
        var baseDirectory = CreateTemporaryDirectory();
        try
        {
            File.WriteAllBytes(
                ReadOsPackagedRuntimeFfiLoader.GetLibraryPath(baseDirectory),
                [0x4D, 0x5A]);
            var loadCalled = false;

            var adapter = ReadOsPackagedRuntimeFfiLoader.TryLoad(
                baseDirectory,
                _ =>
                {
                    loadCalled = true;
                    throw new InvalidOperationException("simulated ABI mismatch");
                });

            Assert.True(loadCalled);
            Assert.Null(adapter);
        }
        finally
        {
            TryDelete(baseDirectory);
        }
    }

    private static string CreateTemporaryDirectory()
    {
        var path = Path.Combine(
            Path.GetTempPath(),
            "reados-loader-tests",
            Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(path);
        return path;
    }

    private static void TryDelete(string path)
    {
        try
        {
            if (Directory.Exists(path))
            {
                Directory.Delete(path, recursive: true);
            }
        }
        catch
        {
            // Best-effort cleanup for temporary test data.
        }
    }
}
