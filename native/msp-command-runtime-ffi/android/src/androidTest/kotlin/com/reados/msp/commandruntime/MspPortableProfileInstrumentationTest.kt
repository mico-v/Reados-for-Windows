package com.reados.msp.commandruntime

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import org.json.JSONArray
import org.json.JSONObject
import org.junit.runner.RunWith

/** Executes the canonical portable profile fixtures against the packaged Rust ABI. */
@RunWith(AndroidJUnit4::class)
class MspPortableProfileInstrumentationTest {
    @Test
    fun sharedPortableProfileFixturesExecuteOnDevice() {
        val context = InstrumentationRegistry.getInstrumentation().context
        val fixture = context.assets.open("portable_msp_v1_fixtures.json").bufferedReader().use { it.readText() }
        val document = JSONObject(fixture)
        assertEquals("reados-portable-msp-v1", document.getString("profile"))

        val workspace = document.getJSONObject("workspace")
        val cases = document.getJSONArray("cases")
        val portableCommands = setOf(
            "cat", "du", "echo", "find", "grep", "head", "ls", "printf", "pwd", "sed", "tail", "wc",
        )
        var executed = 0
        var excluded = 0
        MspCommandRuntimeFfiClient.withNative().openSession().use { session ->
            val workspaceHandle = session.workspaceHandle()
            val keys = workspace.keys()
            while (keys.hasNext()) {
                val path = keys.next()
                workspaceHandle.putFile(path, workspace.getString(path).toByteArray(Charsets.UTF_8))
            }

            for (index in 0 until cases.length()) {
                val fixtureCase = cases.getJSONObject(index)
                if (portableCommands.contains(fixtureCase.getString("command"))) {
                    executeCase(session, fixtureCase)
                    executed++
                } else {
                    assertEquals("command", fixtureCase.getString("command"))
                    val result = session.execute(
                        MspVirtualRequest(command = "'command'", cwd = "/work"),
                    )
                    check(result.exitCode != 0) { "excluded command helper was exposed" }
                    excluded++
                }
            }
        }
        assertEquals(16, executed)
        assertEquals(1, excluded)
    }

    private fun executeCase(session: MspCommandRuntimeFfiSession, fixture: JSONObject) {
        val arguments = fixture.optJSONArray("args") ?: JSONArray()
        val command = buildString {
            append(quoteShellWord(fixture.getString("command")))
            for (index in 0 until arguments.length()) {
                append(' ')
                append(quoteShellWord(arguments.getString(index)))
            }
        }
        val stdin = when {
            fixture.has("stdinBytes") -> {
                val values = fixture.getJSONArray("stdinBytes")
                ByteArray(values.length()) { values.getInt(it).toByte() }
            }
            fixture.has("stdin") -> fixture.getString("stdin").toByteArray(Charsets.UTF_8)
            else -> null
        }
        val result = session.execute(
            MspVirtualRequest(command = command, cwd = "/work", stdin = stdin),
        )
        val expected = fixture.getJSONObject("expected")
        assertEquals(expected.getInt("exitCode"), result.exitCode, fixture.getString("id"))
        assertContentEquals(
            expectedBytes(expected, "stdoutBytes", "stdout"),
            result.stdout,
            fixture.getString("id"),
        )
        assertContentEquals(
            expectedBytes(expected, "stderrBytes", "stderr"),
            result.stderr,
            fixture.getString("id"),
        )
        val disclosure = (result.stderr + result.diagnostic).toString(Charsets.UTF_8)
        check(!Regex("(?i)([A-Z]:\\\\|/Users/|/home/|PATH=|ANDROID_HOME|ANDROID_SDK)").containsMatchIn(disclosure)) {
            "fixture ${fixture.getString("id")} disclosed host state"
        }
    }

    private fun expectedBytes(expected: JSONObject, bytesKey: String, textKey: String): ByteArray =
        if (expected.has(bytesKey)) {
            val values = expected.getJSONArray(bytesKey)
            ByteArray(values.length()) { values.getInt(it).toByte() }
        } else {
            expected.optString(textKey, "").toByteArray(Charsets.UTF_8)
        }

    private fun quoteShellWord(value: String): String =
        "'" + value.replace("'", "'\\''") + "'"
}
