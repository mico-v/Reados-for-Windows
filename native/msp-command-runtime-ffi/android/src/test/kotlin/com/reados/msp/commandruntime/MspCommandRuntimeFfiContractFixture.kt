package com.reados.msp.commandruntime

/** Kotlin contract tests shared by JVM and Android unit-test runners. */
object MspCommandRuntimeFfiContractFixture {
    fun runAll() {
        binaryInputAndEmptyStdinRemainDistinct()
        invalidPathsAndByteLimitsAreRejected()
        exportListIsExact()
        resultCopiesAreDefensive()
        defaultClientDoesNotLoadNative()
    }

    private fun binaryInputAndEmptyStdinRemainDistinct() {
        val binary = byteArrayOf(0, 0xff.toByte())
        val request = MspVirtualRequest(command = "cat", cwd = "/workspace", stdin = binary)
        check(request.stdin!!.contentEquals(binary))
        binary[0] = 1
        check(request.stdin!![0].toInt() == 0)
        check(MspVirtualRequest(command = "cat", cwd = "/workspace").stdin == null)
        check(MspVirtualRequest(command = "cat", cwd = "/workspace", stdin = byteArrayOf()).stdin!!.isEmpty())
    }

    private fun invalidPathsAndByteLimitsAreRejected() {
        val invalid = listOf("", "relative", "/", "/a/", "/a//b", "/a/../b", "/a/./b", "/a/.msp/x", "C:/a", "/a\\b", "/a\u0001b")
        invalid.forEach { check(!MspVirtualPathValidator.isValid(it)) { it } }
        val longPath = "/" + "é".repeat((MspCommandRuntimeFfiAbi.MAX_CWD_BYTES / 2L).toInt())
        check(MspVirtualPathValidator.validate(longPath) is MspVirtualPathValidation.Invalid)
        val oversized = ByteArray((MspCommandRuntimeFfiAbi.MAX_STDIN_BYTES + 1L).toInt())
        check(MspVirtualRequest.tryCreate(command = "cat", cwd = "/workspace", stdin = oversized) is MspVirtualRequestValidation.Invalid)
    }

    private fun exportListIsExact() {
        check(MspCommandRuntimeFfiAbi.EXPORT_NAMES == listOf(
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
        ))
    }

    private fun resultCopiesAreDefensive() {
        val source = byteArrayOf(0, 0xff.toByte())
        val result = MspVirtualResult(0, source, byteArrayOf(), byteArrayOf(1))
        source[0] = 9
        check(result.stdout[0].toInt() == 0)
        val output = result.stdout
        output[1] = 8
        check(result.stdout[1].toInt() and 0xff == 0xff)
    }

    private fun defaultClientDoesNotLoadNative() {
        val result = MspCommandRuntimeFfiClient().execute(
            MspVirtualRequest(command = "pwd", cwd = "/workspace"),
        )
        check(result == MspCommandRuntimeFfiCall.Failure(MspCommandRuntimeFfiError.NativeUnavailable))
    }
}
