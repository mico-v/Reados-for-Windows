package com.reados.msp.commandruntime

/**
 * A detached native result. Every byte array is copied at construction and on
 * access, so freeing/reusing native storage cannot mutate this value.
 */
class MspVirtualResult(
    val exitCode: Int,
    stdout: ByteArray,
    stderr: ByteArray,
    diagnostic: ByteArray,
) {
    private val stdoutSnapshot = stdout.clone()
    private val stderrSnapshot = stderr.clone()
    private val diagnosticSnapshot = diagnostic.clone()

    val stdout: ByteArray
        get() = stdoutSnapshot.clone()

    val stderr: ByteArray
        get() = stderrSnapshot.clone()

    val diagnostic: ByteArray
        get() = diagnosticSnapshot.clone()

    val stdoutData: ByteArray
        get() = stdout

    val stderrData: ByteArray
        get() = stderr

    val diagnosticData: ByteArray
        get() = diagnostic

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is MspVirtualResult) return false
        return exitCode == other.exitCode &&
            stdout.contentEquals(other.stdoutSnapshot) &&
            stderr.contentEquals(other.stderrSnapshot) &&
            diagnostic.contentEquals(other.diagnosticSnapshot)
    }

    override fun hashCode(): Int {
        var result = exitCode
        result = 31 * result + stdoutSnapshot.contentHashCode()
        result = 31 * result + stderrSnapshot.contentHashCode()
        result = 31 * result + diagnosticSnapshot.contentHashCode()
        return result
    }
}
