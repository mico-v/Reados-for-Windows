package com.reados.msp.commandruntime

/** Android ABI target used by the Host provider-registration record. */
enum class MspAndroidToolTargetArchitecture {
    Arm64,
    X64,
}

/** Evidence projected from a bound platform provider without host paths. */
data class MspAndroidToolProviderRegistration(
    val providerId: String,
    val bundleId: String,
    val source: MspAndroidToolProviderSource,
    val executableSha256: String,
    val manifestSha256: String,
    val licenseId: String,
    val noticeId: String,
    val profiles: Set<MspAndroidToolProfile>,
)

/** The only provider surface the Host may call after registration. */
interface MspAndroidToolHostProvider {
    val registration: MspAndroidToolProviderRegistration

    fun execute(
        request: MspAndroidToolRequest,
        cancellation: MspAndroidCancellation = MspAndroidNoCancellation,
    ): MspAndroidToolCall<MspAndroidToolResult>
}

/** Host policy is evaluated before a platform process can be started. */
data class MspAndroidToolHostPolicyRequest(
    val providerId: String,
    val profile: MspAndroidToolProfile,
    val virtualCwd: String,
    val virtualPath: String?,
    val actor: String,
    val sessionId: String,
)

enum class MspAndroidToolHostPolicyDecision {
    Allow,
    Deny,
    RequireApproval,
    NotEvaluated,
}

fun interface MspAndroidToolHostPolicy {
    fun authorize(request: MspAndroidToolHostPolicyRequest): MspAndroidToolHostPolicyDecision
}

fun interface MspAndroidToolHostApproval {
    fun approve(request: MspAndroidToolHostPolicyRequest): Boolean
}

fun interface MspAndroidToolHostAuditSink {
    fun record(record: MspAndroidToolHostAuditRecord)
}

data class MspAndroidToolHostAuditRecord(
    val providerId: String,
    val profile: MspAndroidToolProfile?,
    val actor: String,
    val sessionId: String,
    val decision: MspAndroidToolHostPolicyDecision,
    val status: MspAndroidToolHostStatus,
    val exitCode: Int?,
    val errorCode: String?,
)

enum class MspAndroidToolHostStatus {
    Succeeded,
    ToolFailed,
    PolicyDenied,
    ApprovalRequired,
    CapabilityBlocked,
    Cancelled,
    HostFailure,
}

data class MspAndroidToolHostRequest(
    val profile: MspAndroidToolProfile,
    val virtualCwd: String = "/workspace",
    val virtualPath: String? = null,
    val actor: String = "agent",
    val sessionId: String = "default",
)

data class MspAndroidToolHostResult(
    val status: MspAndroidToolHostStatus,
    val decision: MspAndroidToolHostPolicyDecision,
    val toolResult: MspAndroidToolResult? = null,
    val errorCode: String? = null,
) {
    val succeeded: Boolean
        get() = status == MspAndroidToolHostStatus.Succeeded
}

sealed interface MspAndroidToolHostRegistrationResult {
    data class Verified(
        val registration: MspAndroidToolProviderRegistration,
        val targetRid: String,
        val architecture: MspAndroidToolTargetArchitecture,
    ) : MspAndroidToolHostRegistrationResult

    data class Blocked(val errorCode: String) : MspAndroidToolHostRegistrationResult
}

/**
 * Host-owned registry for Android providers. It stores only verified bundle
 * evidence and never resolves paths, starts processes, or records audit.
 */
class MspAndroidToolProviderCatalog {
    private var provider: MspAndroidToolHostProvider? = null
    private var registration: MspAndroidToolHostRegistrationResult? = null

    @Synchronized
    fun register(
        candidate: MspAndroidToolHostProvider,
        targetRid: String,
        architecture: MspAndroidToolTargetArchitecture,
    ): MspAndroidToolHostRegistrationResult {
        if (provider != null) return MspAndroidToolHostRegistrationResult.Blocked("provider_already_registered")
        if (!isSafeRid(targetRid)) return MspAndroidToolHostRegistrationResult.Blocked("target_rid_invalid")

        val evidence = candidate.registration
        if (evidence.providerId != MspAndroidToolProvider.PROVIDER_ID ||
            evidence.bundleId != MspAndroidToolProvider.BUNDLE_ID ||
            evidence.source != MspAndroidToolProviderSource.AppOwnedBundle ||
            evidence.manifestSha256.lowercase() != MspAndroidToolProvider.MANIFEST_SHA256 ||
            evidence.licenseId != MspAndroidToolProvider.LICENSE_ID ||
            evidence.noticeId != MspAndroidToolProvider.NOTICE_ID ||
            !isSha256(evidence.executableSha256) ||
            evidence.profiles != setOf(
                MspAndroidToolProfile.PrintWorkingDirectory,
                MspAndroidToolProfile.ReadFile,
                MspAndroidToolProfile.ListDirectory,
            )
        ) {
            return MspAndroidToolHostRegistrationResult.Blocked("provider_evidence_invalid")
        }

        val verified = MspAndroidToolHostRegistrationResult.Verified(
            registration = evidence,
            targetRid = targetRid,
            architecture = architecture,
        )
        provider = candidate
        registration = verified
        return verified
    }

    @Synchronized
    fun providerOrNull(): MspAndroidToolHostProvider? = provider

    @Synchronized
    fun registrationOrNull(): MspAndroidToolHostRegistrationResult? = registration

    private fun isSafeRid(value: String): Boolean =
        value.isNotEmpty() && value.length <= 64 &&
            value.all { it.isLetterOrDigit() || it == '-' || it == '_' || it == '.' }

    private fun isSha256(value: String): Boolean =
        value.length == 64 && value.all { it in "0123456789abcdefABCDEF" }
}

/**
 * Consuming-app Host facade. Binding, policy, approval, terminal projection,
 * and exactly-once audit are kept here; the Android provider owns only
 * workspace projection and process lifecycle.
 */
class MspAndroidToolHost(
    private val factory: MspAndroidToolProviderFactory?,
    private val workspaceRoot: java.io.File,
    private val expectedExecutableSha256: String,
    private val targetRid: String,
    private val targetArchitecture: MspAndroidToolTargetArchitecture,
    private val policy: MspAndroidToolHostPolicy,
    private val approval: MspAndroidToolHostApproval,
    private val auditSink: MspAndroidToolHostAuditSink,
    private val limits: MspAndroidToolLimits = MspAndroidToolLimits(),
    private val catalog: MspAndroidToolProviderCatalog = MspAndroidToolProviderCatalog(),
) {
    private var bindingErrorCode: String? = null

    fun bindAppOwnedBundle(): MspAndroidToolHostRegistrationResult {
        val resolvedFactory = factory ?: run {
            bindingErrorCode = "factory_unavailable"
            return MspAndroidToolHostRegistrationResult.Blocked(bindingErrorCode!!)
        }
        val bound = resolvedFactory.bindAppOwnedBundle(
            workspaceRoot = workspaceRoot,
            expectedExecutableSha256 = expectedExecutableSha256,
            limits = limits,
        )
        val candidate = when (bound) {
            is MspAndroidToolCall.Success -> bound.value
            is MspAndroidToolCall.Failure -> {
                bindingErrorCode = bound.error.code()
                return MspAndroidToolHostRegistrationResult.Blocked(bindingErrorCode!!)
            }
        }
        val result = catalog.register(candidate, targetRid, targetArchitecture)
        if (result is MspAndroidToolHostRegistrationResult.Blocked) {
            bindingErrorCode = result.errorCode
        }
        return result
    }

    /** Register a provider already bound by the consuming Host or test harness. */
    fun registerVerifiedProvider(
        provider: MspAndroidToolHostProvider,
    ): MspAndroidToolHostRegistrationResult {
        val result = catalog.register(provider, targetRid, targetArchitecture)
        if (result is MspAndroidToolHostRegistrationResult.Blocked) {
            bindingErrorCode = result.errorCode
        }
        return result
    }

    fun execute(
        request: MspAndroidToolHostRequest,
        cancellation: MspAndroidCancellation = MspAndroidNoCancellation,
    ): MspAndroidToolHostResult {
        val provider = catalog.providerOrNull()
        if (provider == null) {
            return complete(
                request,
                MspAndroidToolHostPolicyDecision.NotEvaluated,
                MspAndroidToolHostStatus.CapabilityBlocked,
                errorCode = bindingErrorCode ?: "provider_not_registered",
            )
        }

        val policyRequest = MspAndroidToolHostPolicyRequest(
            providerId = provider.registration.providerId,
            profile = request.profile,
            virtualCwd = request.virtualCwd,
            virtualPath = request.virtualPath,
            actor = request.actor,
            sessionId = request.sessionId,
        )
        val decision = try {
            policy.authorize(policyRequest)
        } catch (_: Throwable) {
            return complete(
                request,
                MspAndroidToolHostPolicyDecision.NotEvaluated,
                MspAndroidToolHostStatus.HostFailure,
                errorCode = "policy_failure",
            )
        }

        when (decision) {
            MspAndroidToolHostPolicyDecision.Deny ->
                return complete(request, decision, MspAndroidToolHostStatus.PolicyDenied, "policy_denied")
            MspAndroidToolHostPolicyDecision.RequireApproval -> {
                val approved = try {
                    approval.approve(policyRequest)
                } catch (_: Throwable) {
                    return complete(
                        request,
                        decision,
                        MspAndroidToolHostStatus.HostFailure,
                        "approval_failure",
                    )
                }
                if (!approved) {
                    return complete(
                        request,
                        decision,
                        MspAndroidToolHostStatus.ApprovalRequired,
                        "approval_required",
                    )
                }
            }
            MspAndroidToolHostPolicyDecision.Allow -> Unit
            MspAndroidToolHostPolicyDecision.NotEvaluated ->
                return complete(request, decision, MspAndroidToolHostStatus.HostFailure, "policy_not_evaluated")
        }

        val call = try {
            provider.execute(
                MspAndroidToolRequest(
                    profile = request.profile,
                    virtualCwd = request.virtualCwd,
                    virtualPath = request.virtualPath,
                ),
                cancellation,
            )
        } catch (_: Throwable) {
            return complete(request, decision, MspAndroidToolHostStatus.HostFailure, "provider_failure")
        }

        return when (call) {
            is MspAndroidToolCall.Success -> {
                val result = call.value
                complete(
                    request,
                    decision,
                    if (result.cancelled) MspAndroidToolHostStatus.Cancelled
                    else if (result.exitCode == 0 && !result.timedOut && !result.outputLimitExceeded) {
                        MspAndroidToolHostStatus.Succeeded
                    } else {
                        MspAndroidToolHostStatus.ToolFailed
                    },
                    toolResult = result,
                    errorCode = when {
                        result.cancelled -> "cancelled"
                        result.timedOut -> "timeout"
                        result.outputLimitExceeded -> "output_limit_exceeded"
                        result.exitCode != 0 -> "tool_exit"
                        else -> null
                    },
                )
            }
            is MspAndroidToolCall.Failure -> complete(
                request,
                decision,
                if (call.error == MspAndroidToolError.Cancelled) {
                    MspAndroidToolHostStatus.Cancelled
                } else {
                    MspAndroidToolHostStatus.ToolFailed
                },
                errorCode = call.error.code(),
            )
        }
    }

    private fun complete(
        request: MspAndroidToolHostRequest,
        decision: MspAndroidToolHostPolicyDecision,
        status: MspAndroidToolHostStatus,
        errorCode: String? = null,
        toolResult: MspAndroidToolResult? = null,
    ): MspAndroidToolHostResult {
        val providerId = catalog.providerOrNull()?.registration?.providerId
            ?: MspAndroidToolProvider.PROVIDER_ID
        runCatching {
            auditSink.record(
                MspAndroidToolHostAuditRecord(
                    providerId = providerId,
                    profile = request.profile,
                    actor = request.actor,
                    sessionId = request.sessionId,
                    decision = decision,
                    status = status,
                    exitCode = toolResult?.exitCode,
                    errorCode = errorCode,
                ),
            )
        }
        return MspAndroidToolHostResult(status, decision, toolResult, errorCode)
    }
}

private fun MspAndroidToolError.code(): String = when (this) {
    MspAndroidToolError.InvalidEvidence -> "invalid_evidence"
    MspAndroidToolError.UnsupportedProviderSource -> "unsupported_provider_source"
    MspAndroidToolError.ExecutableUnavailable -> "executable_unavailable"
    MspAndroidToolError.BundleIdentityChanged -> "bundle_identity_changed"
    MspAndroidToolError.WorkspaceUnavailable -> "workspace_unavailable"
    MspAndroidToolError.InvalidRequest -> "invalid_request"
    MspAndroidToolError.Limits -> "limits_invalid"
    MspAndroidToolError.Cancelled -> "cancelled"
    MspAndroidToolError.SpawnFailure -> "spawn_failure"
    MspAndroidToolError.IoFailure -> "io_failure"
    MspAndroidToolError.BundleUnavailable -> "bundle_unavailable"
    MspAndroidToolError.BundleMetadataInvalid -> "bundle_metadata_invalid"
    MspAndroidToolError.BundleExtractionFailed -> "bundle_extraction_failed"
}
