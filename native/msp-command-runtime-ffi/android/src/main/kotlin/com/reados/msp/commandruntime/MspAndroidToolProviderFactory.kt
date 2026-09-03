package com.reados.msp.commandruntime

import android.content.Context
import java.io.File
import java.security.MessageDigest

/**
 * Host-owned Android binding for the app-owned Toybox provider. The AAR ships
 * only the provider metadata; a consuming app must add the reviewed `toybox`
 * asset. Missing assets, metadata drift, or extraction failures are explicit
 * unavailable results and never fall back to /system/bin/toybox or Termux.
 */
class MspAndroidToolProviderFactory(
    private val context: Context,
) {
    fun bindAppOwnedBundle(
        workspaceRoot: File,
        expectedExecutableSha256: String,
        limits: MspAndroidToolLimits = MspAndroidToolLimits(),
    ): MspAndroidToolCall<MspAndroidToolProvider> {
        if (!isPrivateWorkspace(workspaceRoot)) {
            return MspAndroidToolCall.Failure(MspAndroidToolError.WorkspaceUnavailable)
        }
        val assets = context.assets
        if (!hasAsset(assets, MspAndroidToolProvider.EXECUTABLE_ASSET)) {
            return MspAndroidToolCall.Failure(MspAndroidToolError.BundleUnavailable)
        }
        val manifestBytes = readAsset(assets, MspAndroidToolProvider.MANIFEST_ASSET)
            ?: return MspAndroidToolCall.Failure(MspAndroidToolError.BundleUnavailable)
        val provenanceBytes = readAsset(assets, MspAndroidToolProvider.PROVENANCE_ASSET)
            ?: return MspAndroidToolCall.Failure(MspAndroidToolError.BundleUnavailable)
        val noticeBytes = readAsset(assets, MspAndroidToolProvider.NOTICE_ASSET)
            ?: return MspAndroidToolCall.Failure(MspAndroidToolError.BundleUnavailable)
        if (!isSha256(expectedExecutableSha256) ||
            sha256(manifestBytes) != MspAndroidToolProvider.MANIFEST_SHA256 ||
            sha256(provenanceBytes) != MspAndroidToolProvider.PROVENANCE_SHA256 ||
            sha256(noticeBytes) != MspAndroidToolProvider.NOTICE_SHA256
        ) {
            return MspAndroidToolCall.Failure(MspAndroidToolError.BundleMetadataInvalid)
        }

        val extracted = try {
            extractExecutable(assets, context.noBackupFilesDir)
        } catch (_: Throwable) {
            return MspAndroidToolCall.Failure(MspAndroidToolError.BundleExtractionFailed)
        } ?: return MspAndroidToolCall.Failure(MspAndroidToolError.BundleUnavailable)

        val digest = MspAndroidToolProvider.sha256File(extracted)
            ?: return MspAndroidToolCall.Failure(MspAndroidToolError.BundleExtractionFailed)
        if (!digest.equals(expectedExecutableSha256, ignoreCase = true)) {
            extracted.delete()
            return MspAndroidToolCall.Failure(MspAndroidToolError.BundleIdentityChanged)
        }
        val evidence = MspAndroidToolEvidence(
            providerId = MspAndroidToolProvider.PROVIDER_ID,
            bundleId = MspAndroidToolProvider.BUNDLE_ID,
            source = MspAndroidToolProviderSource.AppOwnedBundle,
            executableSha256 = digest,
            manifestSha256 = MspAndroidToolProvider.MANIFEST_SHA256,
            licenseId = MspAndroidToolProvider.LICENSE_ID,
            noticeId = MspAndroidToolProvider.NOTICE_ID,
        )
        return MspAndroidToolProvider.bind(extracted, workspaceRoot, evidence, limits)
    }

    private fun extractExecutable(
        assets: android.content.res.AssetManager,
        privateRoot: File,
    ): File? {
        val destination = File(
            privateRoot,
            "msp/providers/${MspAndroidToolProvider.BUNDLE_ID}/toybox",
        )
        destination.parentFile?.mkdirs()
        val absolute = destination.absoluteFile
        if (runCatching { destination.canonicalFile.path != absolute.path }.getOrDefault(true)) {
            return null
        }
        if (destination.exists() && !destination.delete()) {
            return null
        }
        assets.open(MspAndroidToolProvider.EXECUTABLE_ASSET).use { input ->
            destination.outputStream().use { output -> input.copyTo(output) }
        }
        if (runCatching { destination.canonicalFile.path != absolute.path }.getOrDefault(true)) {
            destination.delete()
            return null
        }
        if (!destination.isFile || destination.length() == 0L ||
            !destination.setExecutable(true, true) || !destination.canExecute()
        ) {
            destination.delete()
            return null
        }
        return destination
    }

    private fun readAsset(
        assets: android.content.res.AssetManager,
        name: String,
    ): ByteArray? = runCatching {
        assets.open(name).use { it.readBytes() }
    }.getOrNull()

    private fun hasAsset(
        assets: android.content.res.AssetManager,
        name: String,
    ): Boolean = runCatching {
        assets.open(name).use { true }
    }.getOrDefault(false)

    private fun isPrivateWorkspace(workspaceRoot: File): Boolean {
        val workspace = runCatching { workspaceRoot.canonicalFile }.getOrNull()
            ?: return false
        if (!workspace.isDirectory) return false
        val roots = listOf(context.cacheDir, context.filesDir, context.noBackupFilesDir)
            .mapNotNull { runCatching { it.canonicalFile }.getOrNull() }
        return roots.any { root ->
            workspace.path == root.path ||
                workspace.path.startsWith(root.path + File.separator)
        }
    }

    private fun sha256(value: ByteArray): String =
        MessageDigest.getInstance("SHA-256")
            .digest(value)
            .joinToString("") { byte -> "%02x".format(byte) }

    private fun isSha256(value: String): Boolean =
        value.length == 64 && value.all { it in "0123456789abcdefABCDEF" }
}
