package com.reados.msp.commandruntime

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertTrue

class MspAndroidToolHostTest {
    @Test
    fun deniedRequestDoesNotReachProviderAndProducesOneAudit() {
        val provider = FakeProvider()
        val audits = mutableListOf<MspAndroidToolHostAuditRecord>()
        val host = host(provider, MspAndroidToolHostPolicyDecision.Deny, audits)
        assertIs<MspAndroidToolHostRegistrationResult.Verified>(host.registerVerifiedProvider(provider))

        val result = host.execute(
            MspAndroidToolHostRequest(
                profile = MspAndroidToolProfile.PrintWorkingDirectory,
                actor = "operator",
                sessionId = "s1",
            ),
        )

        assertEquals(MspAndroidToolHostStatus.PolicyDenied, result.status)
        assertFalse(provider.called)
        assertEquals(1, audits.size)
        assertEquals(MspAndroidToolHostPolicyDecision.Deny, audits.single().decision)
    }

    @Test
    fun approvalIsHostOwnedAndRejectedRequestDoesNotStartProvider() {
        val provider = FakeProvider()
        val audits = mutableListOf<MspAndroidToolHostAuditRecord>()
        val host = host(provider, MspAndroidToolHostPolicyDecision.RequireApproval, audits, approved = false)
        assertIs<MspAndroidToolHostRegistrationResult.Verified>(host.registerVerifiedProvider(provider))

        val result = host.execute(MspAndroidToolHostRequest(MspAndroidToolProfile.ReadFile))

        assertEquals(MspAndroidToolHostStatus.ApprovalRequired, result.status)
        assertFalse(provider.called)
        assertEquals(1, audits.size)
        assertEquals(MspAndroidToolHostPolicyDecision.RequireApproval, audits.single().decision)
    }

    @Test
    fun allowedRequestExecutesProviderAndRecordsExactlyOneTerminalAudit() {
        val provider = FakeProvider()
        val audits = mutableListOf<MspAndroidToolHostAuditRecord>()
        val host = host(provider, MspAndroidToolHostPolicyDecision.Allow, audits)
        assertIs<MspAndroidToolHostRegistrationResult.Verified>(host.registerVerifiedProvider(provider))

        val result = host.execute(
            MspAndroidToolHostRequest(
                profile = MspAndroidToolProfile.ReadFile,
                virtualPath = "/workspace/input.bin",
            ),
        )

        assertTrue(result.succeeded)
        assertTrue(provider.called)
        assertEquals(MspAndroidToolProfile.ReadFile, provider.lastRequest?.profile)
        assertEquals(0, result.toolResult?.exitCode)
        assertEquals(1, audits.size)
        assertEquals(MspAndroidToolHostStatus.Succeeded, audits.single().status)
    }

    @Test
    fun unavailableProviderIsCapabilityBlockedAndStillAuditedOnce() {
        val audits = mutableListOf<MspAndroidToolHostAuditRecord>()
        val host = MspAndroidToolHost(
            factory = null,
            workspaceRoot = File("."),
            expectedExecutableSha256 = "0".repeat(64),
            targetRid = "aarch64-linux-android",
            targetArchitecture = MspAndroidToolTargetArchitecture.Arm64,
            policy = MspAndroidToolHostPolicy { MspAndroidToolHostPolicyDecision.Allow },
            approval = MspAndroidToolHostApproval { true },
            auditSink = MspAndroidToolHostAuditSink { audits += it },
        )

        val result = host.execute(MspAndroidToolHostRequest(MspAndroidToolProfile.PrintWorkingDirectory))

        assertEquals(MspAndroidToolHostStatus.CapabilityBlocked, result.status)
        assertEquals("provider_not_registered", result.errorCode)
        assertEquals(1, audits.size)
        assertEquals(MspAndroidToolHostStatus.CapabilityBlocked, audits.single().status)
    }

    @Test
    fun catalogRejectsSystemOracleAsProductRegistration() {
        val provider = FakeProvider(source = MspAndroidToolProviderSource.SystemToyboxOracle)
        val catalog = MspAndroidToolProviderCatalog()
        val result = catalog.register(
            provider,
            "aarch64-linux-android",
            MspAndroidToolTargetArchitecture.Arm64,
        )

        val blocked = assertIs<MspAndroidToolHostRegistrationResult.Blocked>(result)
        assertEquals("provider_evidence_invalid", blocked.errorCode)
    }

    private fun host(
        provider: FakeProvider,
        decision: MspAndroidToolHostPolicyDecision,
        audits: MutableList<MspAndroidToolHostAuditRecord>,
        approved: Boolean = true,
    ): MspAndroidToolHost = MspAndroidToolHost(
        factory = null,
        workspaceRoot = File("."),
        expectedExecutableSha256 = "0".repeat(64),
        targetRid = "aarch64-linux-android",
        targetArchitecture = MspAndroidToolTargetArchitecture.Arm64,
        policy = MspAndroidToolHostPolicy { decision },
        approval = MspAndroidToolHostApproval { approved },
        auditSink = MspAndroidToolHostAuditSink { audits += it },
    )

    private class FakeProvider(
        source: MspAndroidToolProviderSource = MspAndroidToolProviderSource.AppOwnedBundle,
    ) : MspAndroidToolHostProvider {
        override val registration = MspAndroidToolProviderRegistration(
            providerId = MspAndroidToolProvider.PROVIDER_ID,
            bundleId = MspAndroidToolProvider.BUNDLE_ID,
            source = source,
            executableSha256 = "a".repeat(64),
            manifestSha256 = MspAndroidToolProvider.MANIFEST_SHA256,
            licenseId = MspAndroidToolProvider.LICENSE_ID,
            noticeId = MspAndroidToolProvider.NOTICE_ID,
            profiles = setOf(
                MspAndroidToolProfile.PrintWorkingDirectory,
                MspAndroidToolProfile.ReadFile,
                MspAndroidToolProfile.ListDirectory,
            ),
        )

        var called = false
        var lastRequest: MspAndroidToolRequest? = null

        override fun execute(
            request: MspAndroidToolRequest,
            cancellation: MspAndroidCancellation,
        ): MspAndroidToolCall<MspAndroidToolResult> {
            called = true
            lastRequest = request
            return MspAndroidToolCall.Success(
                MspAndroidToolResult(
                    profile = request.profile,
                    stdout = "ok\n".toByteArray(),
                    stderr = ByteArray(0),
                    exitCode = 0,
                    timedOut = false,
                    cancelled = false,
                    outputLimitExceeded = false,
                ),
            )
        }
    }
}
