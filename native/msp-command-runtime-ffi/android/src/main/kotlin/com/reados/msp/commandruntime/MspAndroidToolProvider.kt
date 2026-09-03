package com.reados.msp.commandruntime

import java.io.ByteArrayOutputStream
import java.io.File
import java.io.InputStream
import java.security.MessageDigest
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.concurrent.thread

/** The only provider sources recognized by the Android Toybox provider. */
enum class MspAndroidToolProviderSource {
    /** The production direction: an app-owned, digest-pinned multi-call binary. */
    AppOwnedBundle,

    /** Test-only oracle for the platform's Toybox; never a product distribution claim. */
    SystemToyboxOracle,

    /** Explicitly not implemented in this slice. */
    TermuxOptional,
}

/** Fixed command profiles; callers cannot select an executable or arbitrary argv. */
enum class MspAndroidToolProfile {
    PrintWorkingDirectory,
    ReadFile,
    ListDirectory,
}

data class MspAndroidToolRequest(
    val profile: MspAndroidToolProfile,
    val virtualCwd: String = "/workspace",
    val virtualPath: String? = null,
)

data class MspAndroidToolLimits(
    val maxOutputBytes: Int = 2 * 1024 * 1024,
    val timeoutMillis: Long = 30_000L,
) {
    fun validate(): Boolean =
        maxOutputBytes in 1..(2 * 1024 * 1024) && timeoutMillis in 1..30_000L
}

data class MspAndroidToolEvidence(
    val providerId: String,
    val bundleId: String,
    val source: MspAndroidToolProviderSource,
    val executableSha256: String,
    val manifestSha256: String,
    val licenseId: String,
    val noticeId: String,
)

sealed interface MspAndroidToolError {
    data object InvalidEvidence : MspAndroidToolError
    data object UnsupportedProviderSource : MspAndroidToolError
    data object ExecutableUnavailable : MspAndroidToolError
    data object BundleIdentityChanged : MspAndroidToolError
    data object WorkspaceUnavailable : MspAndroidToolError
    data object InvalidRequest : MspAndroidToolError
    data object Limits : MspAndroidToolError
    data object Cancelled : MspAndroidToolError
    data object SpawnFailure : MspAndroidToolError
    data object IoFailure : MspAndroidToolError
    data object BundleUnavailable : MspAndroidToolError
    data object BundleMetadataInvalid : MspAndroidToolError
    data object BundleExtractionFailed : MspAndroidToolError
}

sealed interface MspAndroidToolCall<out T> {
    data class Success<T>(val value: T) : MspAndroidToolCall<T>
    data class Failure(val error: MspAndroidToolError) : MspAndroidToolCall<Nothing>
}

data class MspAndroidToolResult(
    val profile: MspAndroidToolProfile,
    val stdout: ByteArray,
    val stderr: ByteArray,
    val exitCode: Int,
    val timedOut: Boolean,
    val cancelled: Boolean,
    val outputLimitExceeded: Boolean,
) {
    override fun equals(other: Any?): Boolean =
        other is MspAndroidToolResult &&
            profile == other.profile &&
            stdout.contentEquals(other.stdout) &&
            stderr.contentEquals(other.stderr) &&
            exitCode == other.exitCode &&
            timedOut == other.timedOut &&
            cancelled == other.cancelled &&
            outputLimitExceeded == other.outputLimitExceeded

    override fun hashCode(): Int =
        (((((profile.hashCode() * 31 + stdout.contentHashCode()) * 31 +
            stderr.contentHashCode()) * 31 + exitCode) * 31 +
            timedOut.hashCode()) * 31 + cancelled.hashCode()) * 31 +
            outputLimitExceeded.hashCode()
}

/**
 * Android platform process provider for a small, verified Toybox
 * profile. The constructor owns host paths; requests contain only virtual
 * paths and fixed profile data. No shell, PATH lookup, Termux session, or
 * product policy/audit behavior exists in this class.
 */
class MspAndroidToolProvider private constructor(
    private val executable: File,
    private val workspaceRoot: File,
    private val evidence: MspAndroidToolEvidence,
    private val limits: MspAndroidToolLimits,
) : MspAndroidToolHostProvider {
    companion object {
        const val PROVIDER_ID: String = "reados-android-tool-provider"
        const val BUNDLE_ID: String = "reados-android-tool-provider-1"
        const val MANIFEST_SHA256: String =
            "a528dba0aee524f1a6e1a03f2b8db6c9204ceda58409219e95bbb80c2302175f"
        const val PROVENANCE_SHA256: String =
            "e624bbbfacce924904e06616a94644045530bd8cb0727888f1ba266076640e27"
        const val NOTICE_SHA256: String =
            "1c84b5c46a297853b05fcc766430d72057deac8b44a11d039b2749b789b7a219"
        const val LICENSE_ID: String = "0BSD"
        const val NOTICE_ID: String = "reados-android-tool-provider-notice-1"
        const val MANIFEST_ASSET: String = "android-tool-provider-v1.json"
        const val PROVENANCE_ASSET: String = "provenance.json"
        const val NOTICE_ASSET: String = "NOTICE"
        const val EXECUTABLE_ASSET: String = "toybox"

        fun bind(
            executable: File,
            workspaceRoot: File,
            evidence: MspAndroidToolEvidence,
            limits: MspAndroidToolLimits = MspAndroidToolLimits(),
        ): MspAndroidToolCall<MspAndroidToolProvider> {
            if (evidence.providerId != PROVIDER_ID ||
                evidence.bundleId != BUNDLE_ID ||
                evidence.manifestSha256.lowercase() != MANIFEST_SHA256 ||
                evidence.licenseId != LICENSE_ID ||
                evidence.noticeId != NOTICE_ID ||
                !isSha256(evidence.executableSha256) ||
                !limits.validate()
            ) {
                return MspAndroidToolCall.Failure(MspAndroidToolError.InvalidEvidence)
            }
            if (evidence.source == MspAndroidToolProviderSource.TermuxOptional) {
                return MspAndroidToolCall.Failure(MspAndroidToolError.UnsupportedProviderSource)
            }
            val executableCanonical = runCatching { executable.canonicalFile }.getOrNull()
                ?: return MspAndroidToolCall.Failure(MspAndroidToolError.ExecutableUnavailable)
            val workspaceCanonical = runCatching { workspaceRoot.canonicalFile }.getOrNull()
                ?: return MspAndroidToolCall.Failure(MspAndroidToolError.WorkspaceUnavailable)
            if (!executableCanonical.isFile || !executableCanonical.canExecute()) {
                return MspAndroidToolCall.Failure(MspAndroidToolError.ExecutableUnavailable)
            }
            if (!workspaceCanonical.isDirectory) {
                return MspAndroidToolCall.Failure(MspAndroidToolError.WorkspaceUnavailable)
            }
            if (executableCanonical.name != "toybox") {
                return MspAndroidToolCall.Failure(MspAndroidToolError.InvalidEvidence)
            }
            if (sha256File(executableCanonical) != evidence.executableSha256.lowercase()) {
                return MspAndroidToolCall.Failure(MspAndroidToolError.BundleIdentityChanged)
            }
            return MspAndroidToolCall.Success(
                MspAndroidToolProvider(
                    executableCanonical,
                    workspaceCanonical,
                    evidence,
                    limits,
                ),
            )
        }

        internal fun sha256File(file: File): String? = runCatching {
            val digest = MessageDigest.getInstance("SHA-256")
            file.inputStream().use { input ->
                val buffer = ByteArray(32 * 1024)
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    digest.update(buffer, 0, count)
                }
            }
            digest.digest().joinToString("") { byte -> "%02x".format(byte) }
        }.getOrNull()

        private fun isSha256(value: String): Boolean =
            value.length == 64 && value.all { it in "0123456789abcdefABCDEF" }
    }

    val isSystemOracle: Boolean
        get() = evidence.source == MspAndroidToolProviderSource.SystemToyboxOracle

    /**
     * Host-visible registration evidence. It contains no executable or
     * workspace path; the platform provider keeps those values private.
     */
    override val registration: MspAndroidToolProviderRegistration
        get() = MspAndroidToolProviderRegistration(
            providerId = evidence.providerId,
            bundleId = evidence.bundleId,
            source = evidence.source,
            executableSha256 = evidence.executableSha256.lowercase(),
            manifestSha256 = evidence.manifestSha256.lowercase(),
            licenseId = evidence.licenseId,
            noticeId = evidence.noticeId,
            profiles = setOf(
                MspAndroidToolProfile.PrintWorkingDirectory,
                MspAndroidToolProfile.ReadFile,
                MspAndroidToolProfile.ListDirectory,
            ),
        )

    /** Internal deterministic plan used by JVM tests; it contains host paths only below this layer. */
    internal fun plan(request: MspAndroidToolRequest): MspAndroidToolCall<List<String>> {
        if (resolveVirtualDirectory(request.virtualCwd) == null) {
            return MspAndroidToolCall.Failure(MspAndroidToolError.InvalidRequest)
        }
        return when (request.profile) {
            MspAndroidToolProfile.PrintWorkingDirectory -> {
                if (request.virtualPath != null) {
                    MspAndroidToolCall.Failure(MspAndroidToolError.InvalidRequest)
                } else {
                    MspAndroidToolCall.Success(listOf(executable.absolutePath, "pwd"))
                }
            }
            MspAndroidToolProfile.ReadFile -> {
                val path = request.virtualPath ?: return MspAndroidToolCall.Failure(
                    MspAndroidToolError.InvalidRequest,
                )
                val file = resolveVirtualFile(path)
                    ?: return MspAndroidToolCall.Failure(MspAndroidToolError.WorkspaceUnavailable)
                if (!file.isFile) {
                    MspAndroidToolCall.Failure(MspAndroidToolError.WorkspaceUnavailable)
                } else {
                    MspAndroidToolCall.Success(listOf(executable.absolutePath, "cat", file.absolutePath))
                }
            }
            MspAndroidToolProfile.ListDirectory -> {
                val path = request.virtualPath ?: "/workspace"
                val directory = resolveVirtualDirectory(path)
                    ?: return MspAndroidToolCall.Failure(MspAndroidToolError.WorkspaceUnavailable)
                MspAndroidToolCall.Success(
                    listOf(executable.absolutePath, "ls", "-1", directory.absolutePath),
                )
            }
        }
    }

    override fun execute(
        request: MspAndroidToolRequest,
        cancellation: MspAndroidCancellation = MspAndroidNoCancellation,
    ): MspAndroidToolCall<MspAndroidToolResult> {
        if (cancellation.isCancelled()) {
            return MspAndroidToolCall.Failure(MspAndroidToolError.Cancelled)
        }
        if (MspAndroidToolProvider.sha256File(executable) != evidence.executableSha256.lowercase()) {
            return MspAndroidToolCall.Failure(MspAndroidToolError.BundleIdentityChanged)
        }
        val cwd = resolveVirtualDirectory(request.virtualCwd)
            ?: return MspAndroidToolCall.Failure(MspAndroidToolError.InvalidRequest)
        val planned = when (val result = plan(request)) {
            is MspAndroidToolCall.Failure -> return result
            is MspAndroidToolCall.Success -> result.value
        }
        val process = try {
            ProcessBuilder(planned)
                .directory(cwd)
                .apply {
                    environment().clear()
                    environment()["LANG"] = "C"
                    environment()["LC_ALL"] = "C"
                    redirectInput(ProcessBuilder.Redirect.PIPE)
                    redirectOutput(ProcessBuilder.Redirect.PIPE)
                    redirectError(ProcessBuilder.Redirect.PIPE)
                }
                .start()
        } catch (_: Throwable) {
            return MspAndroidToolCall.Failure(MspAndroidToolError.SpawnFailure)
        }

        val overflow = AtomicBoolean(false)
        val stdoutThread = collectAsync(process.inputStream, overflow)
        val stderrThread = collectAsync(process.errorStream, overflow)
        runCatching { process.outputStream.close() }
        val registration = cancellation.onCancel { terminate(process) }
        var cancelled = false
        var timedOut = false
        var outputLimitExceeded = false
        var exitCode = 1
        val started = System.nanoTime()
        try {
            while (true) {
                if (cancellation.isCancelled()) {
                    cancelled = true
                    terminate(process)
                    break
                }
                if (overflow.get()) {
                    outputLimitExceeded = true
                    terminate(process)
                    break
                }
                try {
                    exitCode = process.exitValue()
                    break
                } catch (_: IllegalThreadStateException) {
                    if ((System.nanoTime() - started) / 1_000_000L >= limits.timeoutMillis) {
                        timedOut = true
                        terminate(process)
                        break
                    }
                    Thread.sleep(10L)
                }
            }
            if (process.isAlive) terminate(process)
            exitCode = runCatching { process.exitValue() }.getOrDefault(exitCode)
        } catch (_: InterruptedException) {
            Thread.currentThread().interrupt()
            terminate(process)
            return MspAndroidToolCall.Failure(MspAndroidToolError.Cancelled)
        } finally {
            registration.close()
        }
        val stdout = stdoutThread.finish()
        val stderr = stderrThread.finish()
        return MspAndroidToolCall.Success(
            MspAndroidToolResult(
                request.profile,
                sanitize(stdout, request.virtualCwd),
                sanitize(stderr, request.virtualCwd),
                exitCode,
                timedOut,
                cancelled,
                outputLimitExceeded,
            ),
        )
    }

    private fun resolveVirtualDirectory(path: String): File? {
        if (!isWorkspacePath(path)) return null
        val candidate = resolveVirtualPath(path) ?: return null
        return candidate.takeIf { it.isDirectory }
    }

    private fun resolveVirtualFile(path: String): File? {
        if (!isWorkspacePath(path)) return null
        return resolveVirtualPath(path)
    }

    private fun resolveVirtualPath(path: String): File? {
        if (MspVirtualPathValidator.validate(path) !is MspVirtualPathValidation.Valid) {
            return null
        }
        val relative = path.removePrefix("/workspace").trimStart('/')
        val candidate = if (relative.isEmpty()) workspaceRoot else File(workspaceRoot, relative)
        val canonical = runCatching { candidate.canonicalFile }.getOrNull() ?: return null
        return canonical.takeIf { isWithin(workspaceRoot, it) }
    }

    private fun sanitize(bytes: ByteArray, virtualCwd: String): ByteArray {
        var result = bytes
        result = replaceBytes(result, executable.absolutePath.toByteArray(), "[redacted-executable]".toByteArray())
        result = replaceBytes(result, workspaceRoot.absolutePath.toByteArray(), virtualCwd.toByteArray())
        return result
    }

    private fun collectAsync(input: InputStream, overflow: AtomicBoolean): BoundedCollector {
        val collector = BoundedCollector(input, limits.maxOutputBytes, overflow)
        collector.start()
        return collector
    }

    private fun terminate(process: Process) {
        runCatching { process.destroy() }
        if (process.isAlive) runCatching { process.destroyForcibly() }
    }

    private class BoundedCollector(
        private val input: InputStream,
        private val limit: Int,
        private val overflow: AtomicBoolean,
    ) {
        private val output = ByteArrayOutputStream(minOf(limit, 64 * 1024))
        private lateinit var worker: Thread

        fun start() {
            worker = thread(start = true, isDaemon = true, name = "reados-android-tool-output") {
                try {
                    val buffer = ByteArray(16 * 1024)
                    while (true) {
                        val count = input.read(buffer)
                        if (count < 0) break
                        if (output.size() + count > limit) {
                            val remaining = (limit - output.size()).coerceAtLeast(0)
                            if (remaining > 0) output.write(buffer, 0, remaining)
                            overflow.set(true)
                            break
                        }
                        output.write(buffer, 0, count)
                    }
                } catch (_: Throwable) {
                    // The process is terminated by the owner on cancellation,
                    // timeout, or output overflow; never expose stream text.
                } finally {
                    runCatching { input.close() }
                }
            }
        }

        fun finish(): ByteArray {
            runCatching { worker.join(1_000L) }
            return output.toByteArray()
        }
    }

    private fun isWorkspacePath(path: String): Boolean =
        path == "/workspace" || path.startsWith("/workspace/")

    private fun isWithin(root: File, candidate: File): Boolean {
        val rootPath = root.path.trimEnd(File.separatorChar)
        return candidate.path == rootPath || candidate.path.startsWith("$rootPath${File.separator}")
    }

    private fun replaceBytes(input: ByteArray, needle: ByteArray, replacement: ByteArray): ByteArray {
        if (needle.isEmpty()) return input
        val output = ByteArrayOutputStream(input.size)
        var cursor = 0
        while (cursor <= input.size - needle.size) {
            var match = true
            for (index in needle.indices) {
                if (input[cursor + index] != needle[index]) {
                    match = false
                    break
                }
            }
            if (match) {
                output.write(replacement)
                cursor += needle.size
            } else {
                output.write(input[cursor].toInt())
                cursor++
            }
        }
        while (cursor < input.size) {
            output.write(input[cursor].toInt())
            cursor++
        }
        return output.toByteArray()
    }
}
