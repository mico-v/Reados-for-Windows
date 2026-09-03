package com.reados.msp.commandruntime

import java.nio.charset.StandardCharsets

sealed interface MspCommandRuntimeFfiError {
    /** No JNI bridge was supplied; no library lookup is attempted. */
    data object NativeUnavailable : MspCommandRuntimeFfiError
    data class InvalidRequest(val error: MspVirtualRequestError) : MspCommandRuntimeFfiError
    data class RequestTooLong(val limitBytes: Long) : MspCommandRuntimeFfiError
    data class AbiMismatch(val expected: Int, val actual: Int) : MspCommandRuntimeFfiError
    data class VersionMismatch(val expected: String, val actual: String) : MspCommandRuntimeFfiError
    data class NativeStatus(val status: Int) : MspCommandRuntimeFfiError
    data object BridgeFailure : MspCommandRuntimeFfiError
}

sealed interface MspCommandRuntimeFfiCall {
    data class Success(val result: MspVirtualResult) : MspCommandRuntimeFfiCall
    data class Failure(val error: MspCommandRuntimeFfiError) : MspCommandRuntimeFfiCall
}

/**
 * Explicit client for the command-runtime C ABI. The default client is
 * intentionally unavailable and never performs library loading. Production
 * Android code can use [withNative] or call [MspCommandRuntimeFfiNative.load]
 * explicitly and inject the returned bridge. */
class MspCommandRuntimeFfiClient(
    private val bridge: MspCommandRuntimeFfiBridge? = null,
) {
    companion object {
        /**
         * Load and validate the packaged Rust/JNI bridge for production use.
         * The source-only default constructor remains side-effect free.
         */
        @JvmStatic
        fun withNative(): MspCommandRuntimeFfiClient =
            MspCommandRuntimeFfiClient(MspCommandRuntimeFfiNative.load())
    }
    val isNativeAvailable: Boolean
        get() = bridge != null

    /** Validate metadata and create a session that owns runtime/workspace handles. */
    fun openSession(): MspCommandRuntimeFfiSession {
        val nativeBridge = bridge ?: throw IllegalStateException("native bridge is unavailable")
        validateBridge(nativeBridge)
        return MspCommandRuntimeFfiSession(nativeBridge)
    }

    fun execute(request: MspVirtualRequest): MspCommandRuntimeFfiCall {
        when (val validation = request.validate()) {
            is MspVirtualRequestValidation.Invalid ->
                return MspCommandRuntimeFfiCall.Failure(
                    MspCommandRuntimeFfiError.InvalidRequest(validation.error),
                )
            is MspVirtualRequestValidation.Valid -> Unit
        }

        val nativeBridge = bridge
            ?: return MspCommandRuntimeFfiCall.Failure(MspCommandRuntimeFfiError.NativeUnavailable)

        return try {
            validateBridge(nativeBridge)
            MspCommandRuntimeFfiSession(nativeBridge).use { session ->
                MspCommandRuntimeFfiCall.Success(session.execute(request))
            }
        } catch (error: MspCommandRuntimeFfiAbiException) {
            if (error.actualAbi != error.expectedAbi) {
                MspCommandRuntimeFfiCall.Failure(
                    MspCommandRuntimeFfiError.AbiMismatch(error.expectedAbi, error.actualAbi),
                )
            } else {
                MspCommandRuntimeFfiCall.Failure(
                    MspCommandRuntimeFfiError.VersionMismatch(
                        error.expectedVersion,
                        error.actualVersion,
                    ),
                )
            }
        } catch (error: MspCommandRuntimeFfiNativeStatusException) {
            MspCommandRuntimeFfiCall.Failure(MspCommandRuntimeFfiError.NativeStatus(error.status))
        } catch (error: IllegalArgumentException) {
            // The only expected IllegalArgumentException after request validation
            // is the explicit JSON byte limit.
            if (error.message?.contains("native byte limit") == true) {
                MspCommandRuntimeFfiCall.Failure(
                    MspCommandRuntimeFfiError.RequestTooLong(
                        MspCommandRuntimeFfiAbi.MAX_JSON_REQUEST_BYTES,
                    ),
                )
            } else {
                MspCommandRuntimeFfiCall.Failure(MspCommandRuntimeFfiError.BridgeFailure)
            }
        } catch (_: Throwable) {
            // Do not leak native paths, loader details, or arbitrary JNI text.
            MspCommandRuntimeFfiCall.Failure(MspCommandRuntimeFfiError.BridgeFailure)
        }
    }

    private fun validateBridge(nativeBridge: MspCommandRuntimeFfiBridge) {
        val actualAbi = nativeBridge.abiVersion()
        val actualVersion = try {
            val bytes = nativeBridge.versionData().copyBytes()
            String(bytes, StandardCharsets.UTF_8)
        } catch (_: Throwable) {
            "<invalid>"
        }
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
    }
}
