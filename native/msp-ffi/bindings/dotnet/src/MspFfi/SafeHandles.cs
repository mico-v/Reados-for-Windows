using Microsoft.Win32.SafeHandles;

namespace MspFfi;

internal sealed class SafeMspWorkspaceHandle : SafeHandleZeroOrMinusOneIsInvalid
{
    internal SafeMspWorkspaceHandle(IntPtr value)
        : base(ownsHandle: true)
    {
        SetHandle(value);
    }

    protected override bool ReleaseHandle()
    {
        try
        {
            NativeMethods.WorkspaceFree(handle);
        }
        catch
        {
            // SafeHandle release must never escape from a finalizer or Dispose.
        }

        return true;
    }
}

internal sealed class SafeMspSessionHandle : SafeHandleZeroOrMinusOneIsInvalid
{
    internal SafeMspSessionHandle(IntPtr value)
        : base(ownsHandle: true)
    {
        SetHandle(value);
    }

    protected override bool ReleaseHandle()
    {
        try
        {
            NativeMethods.SessionFree(handle);
        }
        catch
        {
            // SafeHandle release must never escape from a finalizer or Dispose.
        }

        return true;
    }
}

internal sealed class SafeMspResultHandle : SafeHandleZeroOrMinusOneIsInvalid
{
    internal SafeMspResultHandle(IntPtr value)
        : base(ownsHandle: true)
    {
        SetHandle(value);
    }

    protected override bool ReleaseHandle()
    {
        try
        {
            NativeMethods.ResultFree(handle);
        }
        catch
        {
            // SafeHandle release must never escape from a finalizer or Dispose.
        }

        return true;
    }
}
