package com.reados.msp.commandruntime

/**
 * Length-delimited byte storage returned by a bridge accessor.
 *
 * `length` is the number of meaningful bytes beginning at index zero; bytes
 * after `length` are ignored. Implementations must return `0 <= length <=
 * bytes.size`, and callers must preserve embedded NUL and `0xFF` unchanged.
 */
class MspFfiByteBuffer(bytes: ByteArray, val length: Int = bytes.size) {
    private val bytesSnapshot = bytes.clone()

    init {
        require(length >= 0) { "length must be non-negative" }
        require(length <= bytesSnapshot.size) { "length exceeds byte-array size" }
    }

    val bytes: ByteArray
        get() = bytesSnapshot.clone()

    fun copyBytes(): ByteArray = bytesSnapshot.copyOf(length)
}

/**
 * Explicit-length boundary for the 13 native exports. The production Android
 * implementation is [MspCommandRuntimeFfiNative]; test doubles may implement
 * this interface without loading native code. Every pointer/length pair from
 * the C header is modeled as a byte array plus an explicit length. For a zero
 * length, a null array is permitted; for a nonzero length, the array must
 * contain at least that many bytes. All byte arrays are binary data, not
 * NUL-terminated strings.
 */
interface MspCommandRuntimeFfiBridge {
    fun abiVersion(): Int

    /** Returns the five non-NUL-terminated UTF-8 bytes of the static version. */
    fun versionData(): MspFfiByteBuffer

    fun runtimeCreate(): Long
    fun runtimeFree(runtime: Long)

    fun workspaceCreate(): Long
    fun workspaceFree(workspace: Long)

    /**
     * Equivalent to `msp_command_runtime_ffi_workspace_put_file`.
     * `pathLength` and `dataLength` are explicit byte lengths.
     */
    fun workspacePutFile(
        workspace: Long,
        path: ByteArray?,
        pathLength: Int,
        data: ByteArray?,
        dataLength: Int,
    ): Int

    /**
     * Equivalent to `msp_command_runtime_ffi_execute_json`.
     * `requestLength` is the explicit UTF-8 JSON byte length; the JSON is not
     * NUL-terminated and must not contain raw NUL bytes.
     */
    fun executeJson(
        runtime: Long,
        workspace: Long,
        request: ByteArray?,
        requestLength: Int,
    ): Long

    fun resultExitCode(result: Long): Int
    fun resultStdoutData(result: Long): MspFfiByteBuffer
    fun resultStderrData(result: Long): MspFfiByteBuffer
    fun resultDiagnosticData(result: Long): MspFfiByteBuffer
    fun resultFree(result: Long)
}
