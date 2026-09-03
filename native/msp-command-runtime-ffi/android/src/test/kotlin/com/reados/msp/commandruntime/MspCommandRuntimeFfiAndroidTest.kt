package com.reados.msp.commandruntime

import kotlin.test.Test

class MspCommandRuntimeFfiContractTest {
    @Test
    fun contractFixturePasses() {
        MspCommandRuntimeFfiContractFixture.runAll()
    }

    @Test
    fun sessionJsonEncodingPreservesBinaryAndExplicitEmptyStdin() {
        val binary = byteArrayOf(0, 0xff.toByte())
        val request = MspVirtualRequest(command = "cat", cwd = "/workspace", stdin = binary)
        val encoded = MspCommandRuntimeFfiJson.encode(request)
        val text = encoded.toString(Charsets.UTF_8)
        check("\"stdinBase64\":\"AP8=\"" in text)
        check(MspCommandRuntimeFfiJson.encode(
            MspVirtualRequest(command = "cat", cwd = "/workspace", stdin = byteArrayOf()),
        ).toString(Charsets.UTF_8).contains("\"stdinBase64\":\"\""))
    }
}
