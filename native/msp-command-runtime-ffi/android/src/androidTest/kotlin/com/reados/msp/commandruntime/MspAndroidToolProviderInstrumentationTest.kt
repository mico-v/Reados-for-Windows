package com.reados.msp.commandruntime

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Runs the fixed argv provider against the target device's Toybox oracle. */
@RunWith(AndroidJUnit4::class)
class MspAndroidToolProviderInstrumentationTest {
    @Test
    fun appOwnedBindingFailsClosedWhenToyboxAssetIsNotBundled() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val workspace = context.cacheDir.resolve("msp-tool-provider-app-owned").apply {
            deleteRecursively()
            mkdirs()
        }
        val result = MspAndroidToolProviderFactory(context).bindAppOwnedBundle(
            workspace,
            "0".repeat(64),
        )
        val failure = assertIs<MspAndroidToolCall.Failure>(result)
        assertEquals(MspAndroidToolError.BundleUnavailable, failure.error)
    }

    @Test
    fun appOwnedBindingRejectsWorkspaceOutsideAppPrivateRoots() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val result = MspAndroidToolProviderFactory(context).bindAppOwnedBundle(
            File("/system"),
            "0".repeat(64),
        )
        val failure = assertIs<MspAndroidToolCall.Failure>(result)
        assertEquals(MspAndroidToolError.WorkspaceUnavailable, failure.error)
    }

    @Test
    fun systemToyboxOracleReadsOnlyTheProjectedWorkspaceFile() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val workspace = context.cacheDir.resolve("msp-tool-provider-workspace").apply {
            deleteRecursively()
            mkdirs()
        }
        val input = File(workspace, "input.bin").apply {
            writeBytes(byteArrayOf(0, 0xff.toByte(), 1, 0))
        }
        val executable = File("/system/bin/toybox")
        assertTrue(executable.isFile && executable.canExecute())
        val digest = MspAndroidToolProvider.sha256File(executable)
        check(digest != null)
        val evidence = MspAndroidToolEvidence(
            providerId = MspAndroidToolProvider.PROVIDER_ID,
            bundleId = MspAndroidToolProvider.BUNDLE_ID,
            source = MspAndroidToolProviderSource.SystemToyboxOracle,
            executableSha256 = digest,
            manifestSha256 = MspAndroidToolProvider.MANIFEST_SHA256,
            licenseId = MspAndroidToolProvider.LICENSE_ID,
            noticeId = MspAndroidToolProvider.NOTICE_ID,
        )
        val provider = assertIs<MspAndroidToolCall.Success<MspAndroidToolProvider>>(
            MspAndroidToolProvider.bind(executable, workspace, evidence),
        ).value
        val result = assertIs<MspAndroidToolCall.Success<MspAndroidToolResult>>(
            provider.execute(
                MspAndroidToolRequest(
                    profile = MspAndroidToolProfile.ReadFile,
                    virtualPath = "/workspace/input.bin",
                ),
            ),
        ).value
        assertEquals(0, result.exitCode)
        assertContentEquals(byteArrayOf(0, 0xff.toByte(), 1, 0), result.stdout)
        assertTrue(!String(result.stderr, Charsets.UTF_8).contains(workspace.absolutePath))
        workspace.deleteRecursively()
    }

    @Test
    fun systemOraclePwdIsProjectedAndTermuxIsNotImplicitlyUsed() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val workspace = context.cacheDir.resolve("msp-tool-provider-pwd").apply {
            deleteRecursively()
            mkdirs()
        }
        val executable = File("/system/bin/toybox")
        val digest = MspAndroidToolProvider.sha256File(executable)
        check(digest != null)
        val evidence = MspAndroidToolEvidence(
            MspAndroidToolProvider.PROVIDER_ID,
            MspAndroidToolProvider.BUNDLE_ID,
            MspAndroidToolProviderSource.SystemToyboxOracle,
            digest,
            MspAndroidToolProvider.MANIFEST_SHA256,
            MspAndroidToolProvider.LICENSE_ID,
            MspAndroidToolProvider.NOTICE_ID,
        )
        val provider = assertIs<MspAndroidToolCall.Success<MspAndroidToolProvider>>(
            MspAndroidToolProvider.bind(executable, workspace, evidence),
        ).value
        val result = assertIs<MspAndroidToolCall.Success<MspAndroidToolResult>>(
            provider.execute(MspAndroidToolRequest(MspAndroidToolProfile.PrintWorkingDirectory)),
        ).value
        assertEquals(0, result.exitCode)
        assertTrue(String(result.stdout, Charsets.UTF_8).contains("/workspace"))
        assertTrue(!String(result.stdout, Charsets.UTF_8).contains(workspace.absolutePath))
        workspace.deleteRecursively()
    }
}
