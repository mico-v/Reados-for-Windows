using System.Text.Json;
using MspFfi;

namespace MspFfi.Consumer.Tests;

public sealed class MspFfiConsumerTests
{
    [NativeFact]
    public void Consumer_preserves_virtual_bytes_and_disposes_owners_deterministically()
    {
        var nativePath = Environment.GetEnvironmentVariable(
            MspRuntime.NativeLibraryPathEnvironmentVariable)
            ?? throw new InvalidOperationException("NativeFact did not provide a native DLL path.");

        MspRuntime.UseNativeLibrary(nativePath);
        var runtime = MspRuntime.GetInfo();
        Assert.Equal(MspRuntime.ExpectedAbiVersion, runtime.AbiVersion);
        Assert.Equal(MspRuntime.ExpectedVersion, runtime.Version);

        using var workspace = MspWorkspace.Create();
        var binary = new byte[] { 0x00, 0xff, 0x41, 0x0a };
        Assert.Equal(MspStatus.Ok, workspace.PutFile("/bytes.bin", binary));
        Assert.Equal(MspStatus.Ok, workspace.CreateDirectory("/docs"));
        Assert.Equal(MspStatus.Ok, workspace.AddFile("/docs/name.bin", binary));

        using (var read = workspace.ReadFileRange("/bytes.bin", 0, binary.Length))
        {
            Assert.True(read.IsSuccess);
            Assert.Equal(binary, read.StdoutBytes);
            Assert.Empty(read.StderrBytes);
        }

        using (var stat = workspace.Stat("/bytes.bin"))
        {
            stat.EnsureSuccess("virtual stat");
            using var document = JsonDocument.Parse(stat.StdoutBytes);
            Assert.Equal("regularFile", document.RootElement
                .GetProperty("fileInfo")
                .GetProperty("fileType")
                .GetString());
            Assert.Equal(4, document.RootElement
                .GetProperty("fileInfo")
                .GetProperty("sizeBytes")
                .GetInt32());
        }

        using (var listing = workspace.ListDirectory("/docs"))
        {
            listing.EnsureSuccess("virtual list");
            using var document = JsonDocument.Parse(listing.StdoutBytes);
            Assert.Equal("name.bin", document.RootElement
                .GetProperty("entries")[0]
                .GetProperty("name")
                .GetString());
        }

        using var session = workspace.CreateSession();
        Assert.Throws<ArgumentException>(() => session.Run("echo\0hidden"));
        Assert.Throws<ArgumentException>(() => session.Run("\ud800"));
        Assert.Throws<ArgumentException>(() => workspace.PutFile("C:\\secret", binary));
        Assert.Throws<ArgumentException>(() => workspace.Stat("/bad\0path"));

        using (var command = session.Run("echo dotnet-consumer"))
        {
            command.EnsureSuccess("registered command");
            Assert.Equal(
                "dotnet-consumer\n"u8.ToArray(),
                command.StdoutBytes);
            Assert.Empty(command.StderrBytes);
        }

        // The native session retains its workspace reference independently.
        workspace.Dispose();
        using var retained = session.Run("echo retained");
        retained.EnsureSuccess("retained session");
        Assert.Equal("retained\n"u8.ToArray(), retained.StdoutBytes);

        session.Dispose();
        session.Dispose();
        Assert.Throws<ObjectDisposedException>(() => session.Run("echo after-dispose"));
    }
}
