package com.reados.msp.commandruntime

import java.nio.charset.Charset
import java.nio.charset.StandardCharsets

/** Stable categories for virtual-path rejection. The rejected value is never retained. */
sealed interface MspVirtualPathError {
    data object Empty : MspVirtualPathError
    data class TooLong(val limitBytes: Long) : MspVirtualPathError
    data object NotAbsolute : MspVirtualPathError
    data object Root : MspVirtualPathError
    data object NonVirtualSyntax : MspVirtualPathError
    data object NonCanonical : MspVirtualPathError
    data object ControlCharacter : MspVirtualPathError
    data object Traversal : MspVirtualPathError
    data object HiddenMsp : MspVirtualPathError
    data object InvalidUtf8 : MspVirtualPathError
}

/** Result of validating a path without consulting the host operating system. */
sealed interface MspVirtualPathValidation {
    data class Valid(val path: MspVirtualPath) : MspVirtualPathValidation
    data class Invalid(val error: MspVirtualPathError) : MspVirtualPathValidation
}

/** A canonical, absolute path in the virtual workspace namespace. */
class MspVirtualPath internal constructor(private val text: String) {
    val value: String
        get() = text

    override fun toString(): String = text
    override fun equals(other: Any?): Boolean = other is MspVirtualPath && text == other.text
    override fun hashCode(): Int = text.hashCode()

    companion object {
        /** Construct only after applying the same checks as [MspVirtualPathValidator]. */
        fun validate(value: String): MspVirtualPathValidation =
            MspVirtualPathValidator.validate(value)
    }
}

/**
 * Pure Kotlin implementation of `msp_backend::VirtualPath::new`.
 *
 * Limits are measured in UTF-8 bytes. A virtual path is not a host path: this
 * validator does not normalize, resolve, or query a platform filesystem.
 */
object MspVirtualPathValidator {
    private val utf8: Charset = StandardCharsets.UTF_8

    fun validate(value: String): MspVirtualPathValidation {
        if (value.isEmpty()) return MspVirtualPathValidation.Invalid(MspVirtualPathError.Empty)
        if (hasUnpairedSurrogate(value)) {
            return MspVirtualPathValidation.Invalid(MspVirtualPathError.InvalidUtf8)
        }
        val byteLength = value.toByteArray(utf8).size.toLong()
        if (byteLength > MspCommandRuntimeFfiAbi.MAX_VIRTUAL_PATH_BYTES) {
            return MspVirtualPathValidation.Invalid(
                MspVirtualPathError.TooLong(MspCommandRuntimeFfiAbi.MAX_VIRTUAL_PATH_BYTES),
            )
        }
        if (!value.startsWith('/')) return MspVirtualPathValidation.Invalid(MspVirtualPathError.NotAbsolute)
        if (value == "/") return MspVirtualPathValidation.Invalid(MspVirtualPathError.Root)
        if ('\\' in value || ':' in value) {
            return MspVirtualPathValidation.Invalid(MspVirtualPathError.NonVirtualSyntax)
        }
        if (value.endsWith('/') || "//" in value) {
            return MspVirtualPathValidation.Invalid(MspVirtualPathError.NonCanonical)
        }
        if (value.any(::isControlCharacter)) {
            return MspVirtualPathValidation.Invalid(MspVirtualPathError.ControlCharacter)
        }

        for (component in value.split('/').drop(1)) {
            if (component == "." || component == "..") {
                return MspVirtualPathValidation.Invalid(MspVirtualPathError.Traversal)
            }
            if (component.equals(".msp", ignoreCase = true)) {
                return MspVirtualPathValidation.Invalid(MspVirtualPathError.HiddenMsp)
            }
        }
        return MspVirtualPathValidation.Valid(MspVirtualPath(value))
    }

    fun isValid(value: String): Boolean = validate(value) is MspVirtualPathValidation.Valid

    fun requireValid(value: String): MspVirtualPath {
        return when (val result = validate(value)) {
            is MspVirtualPathValidation.Valid -> result.path
            is MspVirtualPathValidation.Invalid ->
                throw IllegalArgumentException("invalid virtual path: ${result.error}")
        }
    }

    private fun isControlCharacter(character: Char): Boolean =
        character.code <= 0x1F || character.code in 0x7F..0x9F

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
}
