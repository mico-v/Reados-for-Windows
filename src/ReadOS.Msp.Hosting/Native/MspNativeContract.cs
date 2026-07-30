namespace ReadOS.Msp.Hosting.Native;

public static class MspNativeContract
{
    public const string Version = "reados-msp-native/1";

    public const uint AbiV2MajorVersion = 2;

    public const uint AbiV2MinorVersion = 0;

    public const ulong AbiV2ContractId = 0x324D534F44414552UL;

    public const ulong AbiV2RequiredCapabilities = 0xFUL;

    internal const uint AbiV2InfoSize = 32;

    internal const string GetAbiInfoV2Export = "msp_get_abi_info_v2";

    internal const string InvokeV2Export = "msp_invoke_v2";

    internal const string FreeBufferV2Export = "msp_free_buffer_v2";

    internal const string ExecuteExport = "msp_execute_json";

    internal const string ParseExport = "msp_parse_json";

    internal const string NormalizeWorkspacePathExport = "msp_normalize_workspace_path_json";

    internal const string FreeStringExport = "msp_free_string";
}
