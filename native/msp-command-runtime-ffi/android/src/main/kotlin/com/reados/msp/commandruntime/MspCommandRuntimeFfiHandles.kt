package com.reados.msp.commandruntime

import java.nio.charset.StandardCharsets

/** Owns one native runtime handle and frees it exactly once. */
class MspCommandRuntimeFfiRuntimeHandle internal constructor(
    private val bridge: MspCommandRuntimeFfiBridge,
    handle: Long,
    private val operationLock: Any = Any(),
) : AutoCloseable {
    private var nativeHandle: Long = handle

    internal fun borrow(): Long = synchronized(operationLock) {
        synchronized(this) {
            check(nativeHandle != 0L) { "runtime handle is closed" }
            nativeHandle
        }
    }

    override fun close() {
        synchronized(operationLock) {
            val handle = synchronized(this) {
                val current = nativeHandle
                nativeHandle = 0L
                current
            }
            if (handle != 0L) bridge.runtimeFree(handle)
        }
    }
}

/** Owns one native virtual workspace handle and frees it exactly once. */
class MspCommandRuntimeFfiWorkspaceHandle internal constructor(
    private val bridge: MspCommandRuntimeFfiBridge,
    handle: Long,
    private val operationLock: Any = Any(),
) : AutoCloseable {
    private var nativeHandle: Long = handle

    internal fun borrow(): Long = synchronized(operationLock) {
        synchronized(this) {
            check(nativeHandle != 0L) { "workspace handle is closed" }
            nativeHandle
        }
    }

    /** Copy binary data into the virtual workspace using explicit UTF-8 bytes. */
    fun putFile(path: MspVirtualPath, data: ByteArray): Int {
        val pathBytes = path.value.toByteArray(StandardCharsets.UTF_8)
        require(pathBytes.size.toLong() <= MspCommandRuntimeFfiAbi.MAX_CWD_BYTES) {
            "virtual path exceeds the native byte limit"
        }
        require(data.size.toLong() <= MspCommandRuntimeFfiAbi.MAX_FILE_BYTES) {
            "file data exceeds the native byte limit"
        }
        // Snapshot both arrays before crossing JNI. The native implementation
        // copies them before returning and retains no JVM array reference.
        val pathSnapshot = pathBytes.clone()
        val dataSnapshot = data.clone()
        val status = synchronized(operationLock) {
            bridge.workspacePutFile(
                borrow(),
                pathSnapshot,
                pathSnapshot.size,
                dataSnapshot,
                dataSnapshot.size,
            )
        }
        if (status != MspCommandRuntimeFfiAbi.STATUS_OK) {
            throw MspCommandRuntimeFfiNativeStatusException(status)
        }
        return status
    }

    fun putFile(path: String, data: ByteArray): Int =
        putFile(MspVirtualPathValidator.requireValid(path), data)

    override fun close() {
        synchronized(operationLock) {
            val handle = synchronized(this) {
                val current = nativeHandle
                nativeHandle = 0L
                current
            }
            if (handle != 0L) bridge.workspaceFree(handle)
        }
    }
}

/** Owns one result handle; accessors copy bytes before the result is released. */
class MspCommandRuntimeFfiResultHandle internal constructor(
    private val bridge: MspCommandRuntimeFfiBridge,
    handle: Long,
) : AutoCloseable {
    private val operationLock = Any()
    private var nativeHandle: Long = handle

    private fun borrow(): Long = synchronized(operationLock) {
        synchronized(this) {
            check(nativeHandle != 0L) { "result handle is closed" }
            nativeHandle
        }
    }

    fun exitCode(): Int = synchronized(operationLock) {
        bridge.resultExitCode(borrow())
    }

    fun stdout(): ByteArray = synchronized(operationLock) {
        bridge.resultStdoutData(borrow()).copyBytes()
    }

    fun stderr(): ByteArray = synchronized(operationLock) {
        bridge.resultStderrData(borrow()).copyBytes()
    }

    fun diagnostic(): ByteArray = synchronized(operationLock) {
        bridge.resultDiagnosticData(borrow()).copyBytes()
    }

    fun toValue(): MspVirtualResult = synchronized(operationLock) {
        MspVirtualResult(
            exitCode = exitCode(),
            stdout = stdout(),
            stderr = stderr(),
            diagnostic = diagnostic(),
        )
    }

    override fun close() {
        synchronized(operationLock) {
            val handle = synchronized(this) {
                val current = nativeHandle
                nativeHandle = 0L
                current
            }
            if (handle != 0L) bridge.resultFree(handle)
        }
    }
}

/**
 * Owns a runtime and workspace pair. Calls are serialized so a caller cannot
 * mutate the workspace while an execute operation borrows it. Results are
 * independent native allocations and can outlive this session.
 */
class MspCommandRuntimeFfiSession internal constructor(
    private val bridge: MspCommandRuntimeFfiBridge,
) : AutoCloseable {
    private val operationLock = Any()
    private val runtime: MspCommandRuntimeFfiRuntimeHandle
    private val workspace: MspCommandRuntimeFfiWorkspaceHandle
    private var closed = false

    init {
        val runtimeHandle = bridge.runtimeCreate()
        if (runtimeHandle == 0L) {
            throw IllegalStateException("native runtime creation failed")
        }
        runtime = MspCommandRuntimeFfiRuntimeHandle(bridge, runtimeHandle, operationLock)
        try {
            val workspaceHandle = bridge.workspaceCreate()
            if (workspaceHandle == 0L) {
                throw IllegalStateException("native workspace creation failed")
            }
            workspace = MspCommandRuntimeFfiWorkspaceHandle(bridge, workspaceHandle, operationLock)
        } catch (failure: Throwable) {
            runtime.close()
            throw failure
        }
    }

    fun runtimeHandle(): MspCommandRuntimeFfiRuntimeHandle = runtime

    fun workspaceHandle(): MspCommandRuntimeFfiWorkspaceHandle = workspace

    /** Execute and return an explicitly owned result handle. */
    fun executeHandle(request: MspVirtualRequest): MspCommandRuntimeFfiResultHandle {
        when (val validation = request.validate()) {
            is MspVirtualRequestValidation.Invalid ->
                throw IllegalArgumentException("invalid virtual request: ${validation.error}")
            is MspVirtualRequestValidation.Valid -> Unit
        }
        val requestBytes = MspCommandRuntimeFfiJson.encode(request)
        synchronized(operationLock) {
            check(!closed) { "runtime session is closed" }
            val result = bridge.executeJson(
                runtime.borrow(),
                workspace.borrow(),
                requestBytes,
                requestBytes.size,
            )
            if (result == 0L) throw IllegalStateException("native result creation failed")
            return MspCommandRuntimeFfiResultHandle(bridge, result)
        }
    }

    fun execute(request: MspVirtualRequest): MspVirtualResult =
        executeHandle(request).use(MspCommandRuntimeFfiResultHandle::toValue)

    override fun close() {
        synchronized(operationLock) {
            if (closed) return
            closed = true
            // Results are independent allocations; handles are released in
            // reverse ownership order and each close is idempotent.
            try {
                workspace.close()
            } finally {
                runtime.close()
            }
        }
    }
}

internal object MspCommandRuntimeFfiJson {
    fun encode(request: MspVirtualRequest): ByteArray {
        val json = StringBuilder()
        json.append('{')
        json.append("\"version\":").append(request.schemaVersion)
        json.append(",\"command\":").append(quote(request.command))
        json.append(",\"cwd\":").append(quote(request.cwd))
        request.stdin?.let {
            json.append(",\"stdinBase64\":")
                .append(quote(encodeBase64(it)))
        }
        json.append(",\"variables\":{")
        request.variables.entries.sortedBy { it.key }.forEachIndexed { index, entry ->
            if (index != 0) json.append(',')
            json.append(quote(entry.key)).append(':').append(quote(entry.value))
        }
        json.append('}')
        json.append(",\"errorOnUnbound\":").append(request.errorOnUnbound)
        json.append('}')
        val bytes = json.toString().toByteArray(StandardCharsets.UTF_8)
        require(bytes.size.toLong() <= MspCommandRuntimeFfiAbi.MAX_JSON_REQUEST_BYTES) {
            "request exceeds the native byte limit"
        }
        return bytes
    }

    private fun quote(value: String): String {
        val output = StringBuilder(value.length + 2)
        output.append('"')
        for (character in value) {
            when (character) {
                '"' -> output.append("\\\"")
                '\\' -> output.append("\\\\")
                '\b' -> output.append("\\b")
                '\n' -> output.append("\\n")
                '\r' -> output.append("\\r")
                '\t' -> output.append("\\t")
                else -> {
                    if (character.code < 0x20) {
                        output.append("\\u")
                            .append(character.code.toString(16).padStart(4, '0'))
                    } else {
                        output.append(character)
                    }
                }
            }
        }
        output.append('"')
        return output.toString()
    }

    private fun encodeBase64(bytes: ByteArray): String {
        val alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        val output = StringBuilder(((bytes.size + 2) / 3) * 4)
        var index = 0
        while (index < bytes.size) {
            val first = bytes[index].toInt() and 0xff
            val hasSecond = index + 1 < bytes.size
            val hasThird = index + 2 < bytes.size
            val second = if (hasSecond) bytes[index + 1].toInt() and 0xff else 0
            val third = if (hasThird) bytes[index + 2].toInt() and 0xff else 0
            output.append(alphabet[first ushr 2])
            output.append(alphabet[((first and 0x03) shl 4) or (second ushr 4)])
            output.append(if (hasSecond) alphabet[((second and 0x0f) shl 2) or (third ushr 6)] else '=')
            output.append(if (hasThird) alphabet[third and 0x3f] else '=')
            index += 3
        }
        return output.toString()
    }
}
