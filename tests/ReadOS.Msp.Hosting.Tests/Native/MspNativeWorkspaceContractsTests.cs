using System.Runtime.InteropServices;
using ReadOS.Msp.Hosting.Native;

namespace ReadOS.Msp.Hosting.Tests.Native;

public sealed class MspNativeWorkspaceContractsTests
{
    [Fact]
    public void Host_contract_has_frozen_64_byte_layout_and_capabilities()
    {
        Assert.Equal(64, Marshal.SizeOf<MspNativeWorkspaceHostV1>());
        Assert.Equal(0, OffsetOf<MspNativeWorkspaceHostV1>(nameof(MspNativeWorkspaceHostV1.Size)));
        Assert.Equal(4, OffsetOf<MspNativeWorkspaceHostV1>(nameof(MspNativeWorkspaceHostV1.MajorVersion)));
        Assert.Equal(8, OffsetOf<MspNativeWorkspaceHostV1>(nameof(MspNativeWorkspaceHostV1.MinorVersion)));
        Assert.Equal(12, OffsetOf<MspNativeWorkspaceHostV1>(nameof(MspNativeWorkspaceHostV1.Reserved)));
        Assert.Equal(16, OffsetOf<MspNativeWorkspaceHostV1>(nameof(MspNativeWorkspaceHostV1.Capabilities)));
        Assert.Equal(24, OffsetOf<MspNativeWorkspaceHostV1>(nameof(MspNativeWorkspaceHostV1.Context)));
        Assert.Equal(32, OffsetOf<MspNativeWorkspaceHostV1>(nameof(MspNativeWorkspaceHostV1.Invoke)));
        Assert.Equal(40, OffsetOf<MspNativeWorkspaceHostV1>(nameof(MspNativeWorkspaceHostV1.Free)));
        Assert.Equal(48, OffsetOf<MspNativeWorkspaceHostV1>(nameof(MspNativeWorkspaceHostV1.IsCancelled)));
        Assert.Equal(56, OffsetOf<MspNativeWorkspaceHostV1>(nameof(MspNativeWorkspaceHostV1.Reserved2)));

        Assert.Equal(1U, MspNativeWorkspaceAbiV1.MajorVersion);
        Assert.Equal(0U, MspNativeWorkspaceAbiV1.MinorVersion);
        Assert.Equal(0xFUL, MspNativeWorkspaceAbiV1.RequiredCapabilities);
        Assert.Equal(
            MspNativeWorkspaceHostCapabilitiesV1.Required,
            MspNativeWorkspaceHostCapabilitiesV1.Stat |
            MspNativeWorkspaceHostCapabilitiesV1.ListDirectory |
            MspNativeWorkspaceHostCapabilitiesV1.ReadFileRange |
            MspNativeWorkspaceHostCapabilitiesV1.Cancellation);
    }

    [Fact]
    public void Request_contract_has_frozen_56_byte_layout()
    {
        Assert.Equal(56, Marshal.SizeOf<MspNativeWorkspaceRequestV1>());
        Assert.Equal(0, OffsetOf<MspNativeWorkspaceRequestV1>(nameof(MspNativeWorkspaceRequestV1.Size)));
        Assert.Equal(4, OffsetOf<MspNativeWorkspaceRequestV1>(nameof(MspNativeWorkspaceRequestV1.Operation)));
        Assert.Equal(8, OffsetOf<MspNativeWorkspaceRequestV1>(nameof(MspNativeWorkspaceRequestV1.BackendId)));
        Assert.Equal(16, OffsetOf<MspNativeWorkspaceRequestV1>(nameof(MspNativeWorkspaceRequestV1.PathUtf8)));
        Assert.Equal(24, OffsetOf<MspNativeWorkspaceRequestV1>(nameof(MspNativeWorkspaceRequestV1.PathLength)));
        Assert.Equal(32, OffsetOf<MspNativeWorkspaceRequestV1>(nameof(MspNativeWorkspaceRequestV1.Offset)));
        Assert.Equal(40, OffsetOf<MspNativeWorkspaceRequestV1>(nameof(MspNativeWorkspaceRequestV1.Length)));
        Assert.Equal(48, OffsetOf<MspNativeWorkspaceRequestV1>(nameof(MspNativeWorkspaceRequestV1.Reserved)));
    }

    [Fact]
    public void Callback_operations_and_closed_statuses_are_stable()
    {
        Assert.Equal(new uint[] { 1, 2, 3 }, Enum
            .GetValues<MspNativeWorkspaceOperationV1>()
            .Select(value => (uint)value));
        Assert.Equal(Enumerable.Range(0, 12), Enum
            .GetValues<MspNativeWorkspaceCallbackStatusV1>()
            .Select(value => (int)value));
    }

    private static int OffsetOf<T>(string fieldName)
    {
        return checked((int)Marshal.OffsetOf<T>(fieldName));
    }
}
