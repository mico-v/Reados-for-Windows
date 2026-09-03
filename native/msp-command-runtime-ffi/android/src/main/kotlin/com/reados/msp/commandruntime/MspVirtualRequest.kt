package com.reados.msp.commandruntime

import java.nio.charset.StandardCharsets
import java.util.Collections
import java.util.LinkedHashMap

/** Stable categories for request validation. Rejected input is not retained. */
sealed interface MspVirtualRequestError {
    data class SchemaVersionMismatch(val expected: Int, val actual: Int) : MspVirtualRequestError
    data class CommandTooLong(val limitBytes: Long) : MspVirtualRequestError
    data object CommandContainsNul : MspVirtualRequestError
    data object CommandContainsControl : MspVirtualRequestError
    data class InvalidCwd(val error: MspVirtualPathError) : MspVirtualRequestError
    data class TooManyVariables(val limit: Long) : MspVirtualRequestError
    data class InvalidVariableName(val error: MspVariableError) : MspVirtualRequestError
    data class VariableNameTooLong(val limitBytes: Long) : MspVirtualRequestError
    data class VariableValueTooLong(val limitBytes: Long) : MspVirtualRequestError
    data class VariableTotalTooLong(val limitBytes: Long) : MspVirtualRequestError
    data object VariableContainsNul : MspVirtualRequestError
    data object VariableContainsControl : MspVirtualRequestError
    data class StdinTooLong(val limitBytes: Long) : MspVirtualRequestError
    data class RequestTooLong(val limitBytes: Long) : MspVirtualRequestError
}

sealed interface MspVariableError {
    data object Empty : MspVariableError
    data object FirstCharacter : MspVariableError
    data object Character : MspVariableError
}

sealed interface MspVirtualRequestValidation {
    data class Valid(val request: MspVirtualRequest) : MspVirtualRequestValidation
    data class Invalid(val error: MspVirtualRequestError) : MspVirtualRequestValidation
}

/**
 * A validated virtual command request. The constructor snapshots all mutable
 * inputs. `stdin == null` means the field was omitted; `stdin.isEmpty()` means
 * an explicit empty stdin buffer.
 *
 * This type contains only virtual values. It has no host path, process,
 * environment, policy, audit, or cancellation field.
 */
class MspVirtualRequest private constructor(
    val schemaVersion: Int,
    val command: String,
    private val cwdPath: MspVirtualPath,
    stdinBytes: ByteArray?,
    variablesInput: Map<String, String>,
    val errorOnUnbound: Boolean,
) {
    private val stdinBytesSnapshot: ByteArray? = stdinBytes?.clone()
    private val variablesSnapshot: Map<String, String> =
        Collections.unmodifiableMap(LinkedHashMap(variablesInput))

    /** The validated virtual cwd text (never a host path). */
    val cwd: String
        get() = cwdPath.value

    /** The same cwd as a validated virtual-path value. */
    val virtualCwd: MspVirtualPath
        get() = cwdPath

    /** A fresh copy on every access. Null and empty remain distinguishable. */
    val stdin: ByteArray?
        get() = stdinBytesSnapshot?.clone()

    /** A read-only snapshot, detached from the caller's mutable map. */
    val variables: Map<String, String>
        get() = variablesSnapshot

    /** Re-check the invariant at the call boundary without exposing mutable state. */
    fun validate(): MspVirtualRequestValidation =
        validateInputs(
            schemaVersion,
            command,
            cwdPath.value,
            stdinBytesSnapshot,
            variablesSnapshot,
            errorOnUnbound,
        )

    companion object {
        /** Construct a request or throw `IllegalArgumentException` with a stable category. */
        operator fun invoke(
            schemaVersion: Int = MspCommandRuntimeFfiAbi.REQUEST_SCHEMA_VERSION,
            command: String,
            cwd: String,
            stdin: ByteArray? = null,
            variables: Map<String, String> = emptyMap(),
            errorOnUnbound: Boolean = false,
        ): MspVirtualRequest {
            return when (val validation = tryCreate(
                schemaVersion,
                command,
                cwd,
                stdin,
                variables,
                errorOnUnbound,
            )) {
                is MspVirtualRequestValidation.Valid -> validation.request
                is MspVirtualRequestValidation.Invalid ->
                    throw IllegalArgumentException("invalid virtual request: ${validation.error}")
            }
        }

        /** Validate without throwing or retaining invalid input. */
        fun create(
            schemaVersion: Int = MspCommandRuntimeFfiAbi.REQUEST_SCHEMA_VERSION,
            command: String,
            cwd: String,
            stdin: ByteArray? = null,
            variables: Map<String, String> = emptyMap(),
            errorOnUnbound: Boolean = false,
        ): MspVirtualRequestValidation = tryCreate(
            schemaVersion,
            command,
            cwd,
            stdin,
            variables,
            errorOnUnbound,
        )

        /** Validate without throwing or retaining invalid input. */
        fun tryCreate(
            schemaVersion: Int = MspCommandRuntimeFfiAbi.REQUEST_SCHEMA_VERSION,
            command: String,
            cwd: String,
            stdin: ByteArray? = null,
            variables: Map<String, String> = emptyMap(),
            errorOnUnbound: Boolean = false,
        ): MspVirtualRequestValidation {
            val inputStdin = stdin?.clone()
            val inputVariables = LinkedHashMap(variables)
            return when (
                val validation = validateInputs(
                    schemaVersion,
                    command,
                    cwd,
                    inputStdin,
                    inputVariables,
                    errorOnUnbound,
                )
            ) {
                is MspVirtualRequestValidation.Invalid -> validation
                is MspVirtualRequestValidation.Valid ->
                    MspVirtualRequestValidation.Valid(
                        MspVirtualRequest(
                            schemaVersion,
                            command,
                            validation.request.virtualCwd,
                            inputStdin,
                            inputVariables,
                            errorOnUnbound,
                        ),
                    )
            }
        }

        fun require(
            schemaVersion: Int = MspCommandRuntimeFfiAbi.REQUEST_SCHEMA_VERSION,
            command: String,
            cwd: String,
            stdin: ByteArray? = null,
            variables: Map<String, String> = emptyMap(),
            errorOnUnbound: Boolean = false,
        ): MspVirtualRequest = invoke(
            schemaVersion,
            command,
            cwd,
            stdin,
            variables,
            errorOnUnbound,
        )

        private fun validateInputs(
            schemaVersion: Int,
            command: String,
            cwd: String,
            stdin: ByteArray?,
            variables: Map<String, String>,
            @Suppress("UNUSED_PARAMETER") errorOnUnbound: Boolean,
        ): MspVirtualRequestValidation {
            if (schemaVersion != MspCommandRuntimeFfiAbi.REQUEST_SCHEMA_VERSION) {
                return MspVirtualRequestValidation.Invalid(
                    MspVirtualRequestError.SchemaVersionMismatch(
                        MspCommandRuntimeFfiAbi.REQUEST_SCHEMA_VERSION,
                        schemaVersion,
                    ),
                )
            }
            val commandBytes = utf8BytesOrNull(command)
                ?: return MspVirtualRequestValidation.Invalid(MspVirtualRequestError.CommandContainsControl)
            if (commandBytes.size.toLong() > MspCommandRuntimeFfiAbi.MAX_COMMAND_BYTES) {
                return MspVirtualRequestValidation.Invalid(
                    MspVirtualRequestError.CommandTooLong(MspCommandRuntimeFfiAbi.MAX_COMMAND_BYTES),
                )
            }
            if (commandBytes.contains(0)) {
                return MspVirtualRequestValidation.Invalid(MspVirtualRequestError.CommandContainsNul)
            }
            if (command.any(::isControlCharacter)) {
                return MspVirtualRequestValidation.Invalid(MspVirtualRequestError.CommandContainsControl)
            }

            val cwdValidation = MspVirtualPathValidator.validate(cwd)
            val virtualCwd = when (cwdValidation) {
                is MspVirtualPathValidation.Valid -> cwdValidation.path
                is MspVirtualPathValidation.Invalid ->
                    return MspVirtualRequestValidation.Invalid(
                        MspVirtualRequestError.InvalidCwd(cwdValidation.error),
                    )
            }
            if (stdin != null && stdin.size.toLong() > MspCommandRuntimeFfiAbi.MAX_STDIN_BYTES) {
                return MspVirtualRequestValidation.Invalid(
                    MspVirtualRequestError.StdinTooLong(MspCommandRuntimeFfiAbi.MAX_STDIN_BYTES),
                )
            }
            if (variables.size.toLong() > MspCommandRuntimeFfiAbi.MAX_VARIABLES) {
                return MspVirtualRequestValidation.Invalid(
                    MspVirtualRequestError.TooManyVariables(MspCommandRuntimeFfiAbi.MAX_VARIABLES),
                )
            }

            var totalBytes = 0L
            for ((name, value) in variables) {
                val nameBytes = utf8BytesOrNull(name)
                    ?: return MspVirtualRequestValidation.Invalid(
                        MspVirtualRequestError.InvalidVariableName(MspVariableError.Character),
                    )
                val valueBytes = utf8BytesOrNull(value)
                    ?: return MspVirtualRequestValidation.Invalid(MspVirtualRequestError.VariableContainsControl)
                when (val nameError = variableNameError(name)) {
                    null -> Unit
                    else -> return MspVirtualRequestValidation.Invalid(
                        MspVirtualRequestError.InvalidVariableName(nameError),
                    )
                }
                if (nameBytes.contains(0) || valueBytes.contains(0)) {
                    return MspVirtualRequestValidation.Invalid(MspVirtualRequestError.VariableContainsNul)
                }
                if (name.any(::isControlCharacter) || value.any(::isControlCharacter)) {
                    return MspVirtualRequestValidation.Invalid(MspVirtualRequestError.VariableContainsControl)
                }
                if (nameBytes.size.toLong() > MspCommandRuntimeFfiAbi.MAX_VARIABLE_NAME_BYTES) {
                    return MspVirtualRequestValidation.Invalid(
                        MspVirtualRequestError.VariableNameTooLong(
                            MspCommandRuntimeFfiAbi.MAX_VARIABLE_NAME_BYTES,
                        ),
                    )
                }
                if (valueBytes.size.toLong() > MspCommandRuntimeFfiAbi.MAX_VARIABLE_VALUE_BYTES) {
                    return MspVirtualRequestValidation.Invalid(
                        MspVirtualRequestError.VariableValueTooLong(
                            MspCommandRuntimeFfiAbi.MAX_VARIABLE_VALUE_BYTES,
                        ),
                    )
                }
                totalBytes = totalBytes
                    .plus(nameBytes.size.toLong())
                    .plus(valueBytes.size.toLong())
                if (totalBytes > MspCommandRuntimeFfiAbi.MAX_VARIABLE_TOTAL_BYTES) {
                    return MspVirtualRequestValidation.Invalid(
                        MspVirtualRequestError.VariableTotalTooLong(
                            MspCommandRuntimeFfiAbi.MAX_VARIABLE_TOTAL_BYTES,
                        ),
                    )
                }
            }

            // The native JSON ABI also bounds the encoded document. The exact
            // JSON bytes are checked by the client after escaping/base64 encoding.
            return MspVirtualRequestValidation.Valid(
                MspVirtualRequest(
                    schemaVersion,
                    command,
                    virtualCwd,
                    stdin,
                    variables,
                    errorOnUnbound,
                ),
            )
        }

        private fun variableNameError(name: String): MspVariableError? {
            if (name.isEmpty()) return MspVariableError.Empty
            val first = name[0]
            if (first != '_' && !first.isAsciiLetter()) return MspVariableError.FirstCharacter
            if (name.drop(1).any { it != '_' && !it.isAsciiLetterOrDigit() }) {
                return MspVariableError.Character
            }
            return null
        }

        private fun utf8BytesOrNull(value: String): ByteArray? =
            if (value.any { it.isSurrogate() } && hasUnpairedSurrogate(value)) {
                null
            } else {
                value.toByteArray(StandardCharsets.UTF_8)
            }

        private fun hasUnpairedSurrogate(value: String): Boolean {
            var index = 0
            while (index < value.length) {
                val character = value[index]
                when {
                    character.isHighSurrogate() -> {
                        if (index + 1 >= value.length || !value[index + 1].isLowSurrogate()) return true
                        index++
                    }
                    character.isLowSurrogate() -> return true
                }
                index++
            }
            return false
        }

        private fun isControlCharacter(character: Char): Boolean =
            character.code <= 0x1F || character.code in 0x7F..0x9F

        private fun Char.isAsciiLetter(): Boolean =
            this in 'A'..'Z' || this in 'a'..'z'

        private fun Char.isAsciiLetterOrDigit(): Boolean =
            isAsciiLetter() || this in '0'..'9'
    }
}
