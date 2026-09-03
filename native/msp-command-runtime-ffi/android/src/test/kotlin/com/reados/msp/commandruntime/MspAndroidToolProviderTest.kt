package com.reados.msp.commandruntime

import java.nio.file.Files
import kotlin.io.path.absolutePathString
import kotlin.io.path.writeBytes
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue

class MspAndroidToolProviderTest {
    @Test
    fun fixedProfilesUseOnlyProviderOwnedArgvAndProjectedPaths() {
        val fixture = fixture()
        val provider = bind(fixture)

        val pwd = assertIs<MspAndroidToolCall.Success<List<String>>>(
            provider.plan(MspAndroidToolRequest(MspAndroidToolProfile.PrintWorkingDirectory)),
        ).value
        assertEquals(listOf(fixture.executable.absolutePath, "pwd"), pwd)

        val cat = assertIs<MspAndroidToolCall.Success<List<String>>>(
            provider.plan(
                MspAndroidToolRequest(
                    MspAndroidToolProfile.ReadFile,
                    virtualPath = "/workspace/input.txt",
                ),
            ),
        ).value
        assertEquals(
            listOf(fixture.executable.absolutePath, "cat", fixture.file.absolutePath),
            cat,
        )
        assertTrue(cat.none { it.contains("..") })
    }

    @Test
    fun virtualEscapesAndHostPathsFailBeforeProcessPlanning() {
        val fixture = fixture()
        val provider = bind(fixture)
        for (path in listOf("/workspace/../outside", "C:\\secret", "/workspace-escape")) {
            val result = provider.plan(
                MspAndroidToolRequest(MspAndroidToolProfile.ReadFile, virtualPath = path),
            )
            assertEquals(
                MspAndroidToolError.WorkspaceUnavailable,
                assertIs<MspAndroidToolCall.Failure>(result).error,
                path,
            )
        }
    }

    @Test
    fun termuxSourceAndDigestDriftFailClosed() {
        val fixture = fixture()
        val termux = MspAndroidToolProvider.bind(
            fixture.executable,
            fixture.root.toFile(),
            evidence(fixture, MspAndroidToolProviderSource.TermuxOptional),
        )
        assertEquals(
            MspAndroidToolError.UnsupportedProviderSource,
            assertIs<MspAndroidToolCall.Failure>(termux).error,
        )

        val drift = MspAndroidToolProvider.bind(
            fixture.executable,
            fixture.root.toFile(),
            evidence(fixture).copy(executableSha256 = "0".repeat(64)),
        )
        assertEquals(
            MspAndroidToolError.BundleIdentityChanged,
            assertIs<MspAndroidToolCall.Failure>(drift).error,
        )
    }

    @Test
    fun missingVirtualPathIsRejectedWithoutHostDisclosure() {
        val fixture = fixture()
        val provider = bind(fixture)
        val result = provider.plan(
            MspAndroidToolRequest(
                MspAndroidToolProfile.ReadFile,
                virtualPath = "/workspace/missing.txt",
            ),
        )
        val error = assertIs<MspAndroidToolCall.Failure>(result).error
        assertEquals(MspAndroidToolError.WorkspaceUnavailable, error)
        assertTrue(!error.toString().contains(fixture.root.absolutePathString()))
    }

    @Test
    fun preCancelledRequestFailsBeforeExecutableInspection() {
        val fixture = fixture()
        val provider = bind(fixture)
        val token = MspAndroidCancellationToken()
        token.cancel()
        val result = provider.execute(
            MspAndroidToolRequest(MspAndroidToolProfile.PrintWorkingDirectory),
            token,
        )
        assertEquals(
            MspAndroidToolError.Cancelled,
            assertIs<MspAndroidToolCall.Failure>(result).error,
        )
    }

    private fun bind(fixture: Fixture): MspAndroidToolProvider =
        assertIs<MspAndroidToolCall.Success<MspAndroidToolProvider>>(
            MspAndroidToolProvider.bind(
                fixture.executable,
                fixture.root.toFile(),
                evidence(fixture),
            ),
        ).value

    private fun evidence(
        fixture: Fixture,
        source: MspAndroidToolProviderSource = MspAndroidToolProviderSource.AppOwnedBundle,
    ): MspAndroidToolEvidence = MspAndroidToolEvidence(
        providerId = MspAndroidToolProvider.PROVIDER_ID,
        bundleId = MspAndroidToolProvider.BUNDLE_ID,
        source = source,
        executableSha256 = MspAndroidToolProvider.sha256File(fixture.executable)!!,
        manifestSha256 = MspAndroidToolProvider.MANIFEST_SHA256,
        licenseId = MspAndroidToolProvider.LICENSE_ID,
        noticeId = MspAndroidToolProvider.NOTICE_ID,
    )

    private fun fixture(): Fixture {
        val root = Files.createTempDirectory("reados-android-tool-provider")
        val executable = root.resolve("toybox")
        executable.writeBytes(byteArrayOf(1, 2, 3, 4))
        executable.toFile().setExecutable(true)
        val file = root.resolve("input.txt")
        file.writeBytes("provider\n".toByteArray())
        return Fixture(root, executable.toFile(), file.toFile())
    }

    private data class Fixture(
        val root: java.nio.file.Path,
        val executable: java.io.File,
        val file: java.io.File,
    )
}
