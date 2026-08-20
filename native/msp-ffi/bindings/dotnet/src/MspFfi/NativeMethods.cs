using System.Runtime.InteropServices;

namespace MspFfi;

internal static class NativeMethods
{
    internal const string LibraryName = "msp_ffi";

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_runtime_abi_version")]
    internal static extern uint RuntimeAbiVersion();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_runtime_version")]
    internal static extern IntPtr RuntimeVersion();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_workspace_create")]
    internal static extern IntPtr WorkspaceCreate();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_workspace_free")]
    internal static extern void WorkspaceFree(IntPtr workspace);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_workspace_put_file")]
    internal static extern int WorkspacePutFile(
        SafeMspWorkspaceHandle workspace,
        IntPtr virtualPath,
        [In] byte[]? data,
        nuint dataLength);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_workspace_add_file")]
    internal static extern int WorkspaceAddFile(
        SafeMspWorkspaceHandle workspace,
        IntPtr virtualPath,
        [In] byte[]? data,
        nuint dataLength);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_workspace_create_directory")]
    internal static extern int WorkspaceCreateDirectory(
        SafeMspWorkspaceHandle workspace,
        IntPtr virtualPath);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_session_create")]
    internal static extern IntPtr SessionCreate(SafeMspWorkspaceHandle workspace);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_session_create_default")]
    internal static extern IntPtr SessionCreateDefault();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_session_free")]
    internal static extern void SessionFree(IntPtr session);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_session_run_n")]
    internal static extern IntPtr SessionRunN(
        SafeMspSessionHandle session,
        [In] byte[]? command,
        nuint commandLength);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_result_exit_code")]
    internal static extern int ResultExitCode(SafeMspResultHandle result);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_result_stdout_data")]
    internal static extern IntPtr ResultStdoutData(
        SafeMspResultHandle result,
        out nuint length);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_result_stderr_data")]
    internal static extern IntPtr ResultStderrData(
        SafeMspResultHandle result,
        out nuint length);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_result_free")]
    internal static extern void ResultFree(IntPtr result);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_workspace_stat")]
    internal static extern IntPtr WorkspaceStat(
        SafeMspWorkspaceHandle workspace,
        IntPtr virtualPath);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_workspace_list")]
    internal static extern IntPtr WorkspaceList(
        SafeMspWorkspaceHandle workspace,
        IntPtr virtualPath);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_workspace_list_directory")]
    internal static extern IntPtr WorkspaceListDirectory(
        SafeMspWorkspaceHandle workspace,
        IntPtr virtualPath);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_workspace_read")]
    internal static extern IntPtr WorkspaceRead(
        SafeMspWorkspaceHandle workspace,
        IntPtr virtualPath,
        ulong offset,
        nuint length);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl,
        ExactSpelling = true, EntryPoint = "msp_workspace_read_file_range")]
    internal static extern IntPtr WorkspaceReadFileRange(
        SafeMspWorkspaceHandle workspace,
        IntPtr virtualPath,
        ulong offset,
        nuint length);
}
