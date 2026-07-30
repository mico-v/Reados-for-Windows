namespace ReadOS.Msp.Hosting.Native;

public interface IMspNativeTransport : IDisposable
{
    byte[] Invoke(MspNativeOperation operation, ReadOnlyMemory<byte> requestJsonUtf8);
}
