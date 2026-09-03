package com.reados.msp.commandruntime

import java.nio.charset.StandardCharsets

/** A native status returned by the Rust C ABI and surfaced without host text. */
class MspCommandRuntimeFfiNativeStatusException(
    val status: Int,
) : IllegalStateException("msp command runtime native status $status")

/** The packaged Rust/JNI libraries could not be loaded. */
class MspCommandRuntimeFfiLoadException :
    IllegalStateException("msp command runtime FFI native libraries unavailable")

/** The packaged Rust artifact and JNI shim disagree about ABI metadata. */
class MspCommandRuntimeFfiAbiException(
    val expectedAbi: Int,
    val actualAbi: Int,
    val expectedVersion: String,
    val actualVersion: String,
) : IllegalStateException("msp command runtime FFI ABI/version mismatch")

/**
 * Explicit Android JNI implementation of all 13 exports in the adjacent C
 * header. Loading is never implicit: callers must call [load], which loads the
 * Rust artifact first, then the JNI shim, and validates ABI/version metadata.
 *
 * The two shared objects are packaged in the AAR under arm64-v8a. No host path,
 * process, environment, policy, audit, or cancellation data is accepted here.
 */
class MspCommandRuntimeFfiNative private constructor() : MspCommandRuntimeFfiBridge {
    override fun abiVersion(): Int = nativeAbiVersion()

    override fun versionData(): MspFfiByteBuffer = nativeVersionData()

    override fun runtimeCreate(): Long = nativeRuntimeCreate()

    override fun runtimeFree(runtime: Long) {
        if (runtime != 0L) nativeRuntimeFree(runtime)
    }

    override fun workspaceCreate(): Long = nativeWorkspaceCreate()

    override fun workspaceFree(workspace: Long) {
        if (workspace != 0L) nativeWorkspaceFree(workspace)
    }

    override fun workspacePutFile(
        workspace: Long,
        path: ByteArray?,
        pathLength: Int,
        data: ByteArray?,
        dataLength: Int,
    ): Int {
        require(pathLength >= 0 && pathLength <= (path?.size ?: 0)) {
            "pathLength must be within path byte-array bounds"
        }
        require(dataLength >= 0 && dataLength <= (data?.size ?: 0)) {
            "dataLength must be within data byte-array bounds"
        }
        val status = nativeWorkspacePutFile(workspace, path, pathLength, data, dataLength)
        if (status != MspCommandRuntimeFfiAbi.STATUS_OK) {
            throw MspCommandRuntimeFfiNativeStatusException(status)
        }
        return status
    }

    override fun executeJson(
        runtime: Long,
        workspace: Long,
        request: ByteArray?,
        requestLength: Int,
    ): Long {
        require(requestLength >= 0 && requestLength <= (request?.size ?: 0)) {
            "requestLength must be within request byte-array bounds"
        }
        return nativeExecuteJson(runtime, workspace, request, requestLength)
    }

    override fun resultExitCode(result: Long): Int = nativeResultExitCode(result)

    override fun resultStdoutData(result: Long): MspFfiByteBuffer = nativeResultStdoutData(result)

    override fun resultStderrData(result: Long): MspFfiByteBuffer = nativeResultStderrData(result)

    override fun resultDiagnosticData(result: Long): MspFfiByteBuffer = nativeResultDiagnosticData(result)

    override fun resultFree(result: Long) {
        if (result != 0L) nativeResultFree(result)
    }

    /** Create an explicitly owned runtime/workspace session. */
    fun openSession(): MspCommandRuntimeFfiSession = MspCommandRuntimeFfiSession(this)

    private external fun nativeAbiVersion(): Int
    private external fun nativeVersionData(): MspFfiByteBuffer
    private external fun nativeRuntimeCreate(): Long
    private external fun nativeRuntimeFree(runtime: Long)
    private external fun nativeWorkspaceCreate(): Long
    private external fun nativeWorkspaceFree(workspace: Long)
    private external fun nativeWorkspacePutFile(
        workspace: Long,
        path: ByteArray?,
        pathLength: Int,
        data: ByteArray?,
        dataLength: Int,
    ): Int
    private external fun nativeExecuteJson(
        runtime: Long,
        workspace: Long,
        request: ByteArray?,
        requestLength: Int,
    ): Long
    private external fun nativeResultExitCode(result: Long): Int
    private external fun nativeResultStdoutData(result: Long): MspFfiByteBuffer
    private external fun nativeResultStderrData(result: Long): MspFfiByteBuffer
    private external fun nativeResultDiagnosticData(result: Long): MspFfiByteBuffer
    private external fun nativeResultFree(result: Long)

    companion object {
        private const val RUST_LIBRARY = "msp_command_runtime_ffi"
        private const val JNI_LIBRARY = "msp_command_runtime_ffi_jni"
        private val lock = Any()
        @Volatile private var loaded: MspCommandRuntimeFfiNative? = null

        /**
         * Load the packaged Rust and JNI libraries exactly once and validate
         * both static metadata values before returning a usable bridge.
         */
        @JvmStatic
        fun load(): MspCommandRuntimeFfiNative = synchronized(lock) {
            loaded ?: run {
                // These are deliberate Android library names, not host paths or
                // PATH lookups. The Rust dependency must precede the JNI shim.
                try {
                    System.loadLibrary(RUST_LIBRARY)
                    System.loadLibrary(JNI_LIBRARY)
                } catch (_: LinkageError) {
                    throw MspCommandRuntimeFfiLoadException()
                } catch (_: SecurityException) {
                    throw MspCommandRuntimeFfiLoadException()
                }
                val bridge = MspCommandRuntimeFfiNative()
                val actualAbi = bridge.abiVersion()
                val actualVersion = String(
                    bridge.versionData().copyBytes(),
                    StandardCharsets.UTF_8,
                )
                if (actualAbi != MspCommandRuntimeFfiAbi.ABI ||
                    actualVersion != MspCommandRuntimeFfiAbi.VERSION
                ) {
                    throw MspCommandRuntimeFfiAbiException(
                        MspCommandRuntimeFfiAbi.ABI,
                        actualAbi,
                        MspCommandRuntimeFfiAbi.VERSION,
                        actualVersion,
                    )
                }
                loaded = bridge
                bridge
            }
        }
    }
}
