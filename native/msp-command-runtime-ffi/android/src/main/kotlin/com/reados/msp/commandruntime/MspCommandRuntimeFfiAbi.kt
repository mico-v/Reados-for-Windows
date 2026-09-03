package com.reados.msp.commandruntime

/** Stable metadata copied from msp_command_runtime_ffi.h. */
object MspCommandRuntimeFfiAbi {
    const val ABI: Int = 1
    const val ABI_VERSION: Int = ABI
    const val VERSION: String = "0.1.0"
    const val VERSION_STRING: String = VERSION
    const val REQUEST_SCHEMA_VERSION: Int = 1

    // Header limits are byte/count limits, not UTF-16 character limits.
    const val MAX_JSON_REQUEST_BYTES: Long = 4L * 1024L * 1024L
    const val MAX_COMMAND_BYTES: Long = 128L * 1024L
    const val MAX_CWD_BYTES: Long = 4L * 1024L
    const val MAX_FILE_BYTES: Long = 8L * 1024L * 1024L
    const val MAX_WORKSPACE_BYTES: Long = 64L * 1024L * 1024L
    const val MAX_WORKSPACE_FILES: Long = 65_536L
    const val MAX_LIST_ENTRIES: Long = 65_536L
    const val MAX_STDIN_BYTES: Long = 2L * 1024L * 1024L
    const val MAX_OUTPUT_BYTES: Long = 2L * 1024L * 1024L
    const val MAX_DIAGNOSTIC_BYTES: Long = 4_096L
    const val MAX_VARIABLES: Long = 256L
    const val MAX_VARIABLE_NAME_BYTES: Long = 64L
    const val MAX_VARIABLE_VALUE_BYTES: Long = 64L * 1024L
    const val MAX_VARIABLE_TOTAL_BYTES: Long = 256L * 1024L

    const val STATUS_OK: Int = 0
    const val STATUS_INVALID_ARGUMENT: Int = 1
    const val STATUS_LIMIT_EXCEEDED: Int = 2
    const val STATUS_INTERNAL_ERROR: Int = 3
    const val STATUS_PANIC: Int = 4

    /** The virtual path bound used by msp-backend (and by cwd in the FFI header). */
    const val MAX_VIRTUAL_PATH_BYTES: Long = MAX_CWD_BYTES

    /** The exact order and spelling of the 13 C ABI exports. */
    val EXPORT_NAMES: List<String>
        get() = exportNames.toList()

    /** Alias retained for callers that call the metadata an export list. */
    val EXPORTS: List<String>
        get() = EXPORT_NAMES

    /** A typed view of the numeric limits in the native header. */
    val HEADER_LIMITS: MspCommandRuntimeFfiHeaderLimits
        get() = MspCommandRuntimeFfiHeaderLimits

    private val exportNames = arrayOf(
        "msp_command_runtime_ffi_abi_version",
        "msp_command_runtime_ffi_version_data",
        "msp_command_runtime_ffi_runtime_create",
        "msp_command_runtime_ffi_runtime_free",
        "msp_command_runtime_ffi_workspace_create",
        "msp_command_runtime_ffi_workspace_free",
        "msp_command_runtime_ffi_workspace_put_file",
        "msp_command_runtime_ffi_execute_json",
        "msp_command_runtime_ffi_result_exit_code",
        "msp_command_runtime_ffi_result_stdout_data",
        "msp_command_runtime_ffi_result_stderr_data",
        "msp_command_runtime_ffi_result_diagnostic_data",
        "msp_command_runtime_ffi_result_free",
    )
}

/** Immutable header limits; values are expressed as unsigned-header-compatible Longs. */
object MspCommandRuntimeFfiHeaderLimits {
    const val maxJsonRequestBytes: Long = MspCommandRuntimeFfiAbi.MAX_JSON_REQUEST_BYTES
    const val maxCommandBytes: Long = MspCommandRuntimeFfiAbi.MAX_COMMAND_BYTES
    const val maxCwdBytes: Long = MspCommandRuntimeFfiAbi.MAX_CWD_BYTES
    const val maxFileBytes: Long = MspCommandRuntimeFfiAbi.MAX_FILE_BYTES
    const val maxWorkspaceBytes: Long = MspCommandRuntimeFfiAbi.MAX_WORKSPACE_BYTES
    const val maxWorkspaceFiles: Long = MspCommandRuntimeFfiAbi.MAX_WORKSPACE_FILES
    const val maxListEntries: Long = MspCommandRuntimeFfiAbi.MAX_LIST_ENTRIES
    const val maxStdinBytes: Long = MspCommandRuntimeFfiAbi.MAX_STDIN_BYTES
    const val maxOutputBytes: Long = MspCommandRuntimeFfiAbi.MAX_OUTPUT_BYTES
    const val maxDiagnosticBytes: Long = MspCommandRuntimeFfiAbi.MAX_DIAGNOSTIC_BYTES
    const val maxVariables: Long = MspCommandRuntimeFfiAbi.MAX_VARIABLES
    const val maxVariableNameBytes: Long = MspCommandRuntimeFfiAbi.MAX_VARIABLE_NAME_BYTES
    const val maxVariableValueBytes: Long = MspCommandRuntimeFfiAbi.MAX_VARIABLE_VALUE_BYTES
    const val maxVariableTotalBytes: Long = MspCommandRuntimeFfiAbi.MAX_VARIABLE_TOTAL_BYTES
}
