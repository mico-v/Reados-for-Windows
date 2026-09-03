package com.reados.msp.commandruntime

import android.content.ContentResolver
import android.database.Cursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.os.CancellationSignal
import android.os.OperationCanceledException
import android.provider.DocumentsContract
import java.io.ByteArrayOutputStream
import java.io.FileNotFoundException
import java.io.IOException
import java.io.InputStream

/** Stable, path-free failure categories for the Android workspace boundary. */
sealed interface MspAndroidWorkspaceError {
    data object InvalidTreeUri : MspAndroidWorkspaceError
    data object PermissionDenied : MspAndroidWorkspaceError
    data object NotFound : MspAndroidWorkspaceError
    data object NotDirectory : MspAndroidWorkspaceError
    data object OutsideMount : MspAndroidWorkspaceError
    data object ProviderFailure : MspAndroidWorkspaceError
    data object NativeFailure : MspAndroidWorkspaceError
    data object Cancelled : MspAndroidWorkspaceError
    data object Closed : MspAndroidWorkspaceError
    data class LimitExceeded(val limitBytes: Long) : MspAndroidWorkspaceError
}

/** Result type used by the Android provider; it never carries host URI/path text. */
sealed interface MspAndroidWorkspaceResult<out T> {
    data class Success<T>(val value: T) : MspAndroidWorkspaceResult<T>
    data class Failure(val error: MspAndroidWorkspaceError) : MspAndroidWorkspaceResult<Nothing>
}

/** Cooperative cancellation token shared by provider reads and workspace projection. */
interface MspAndroidCancellation {
    fun isCancelled(): Boolean

    /** Register a callback that is invoked when cancellation is requested. */
    fun onCancel(callback: () -> Unit): AutoCloseable = AutoCloseable { }
}

/** Mutable lifecycle token; cancellation is observable without exposing host state. */
class MspAndroidCancellationToken : MspAndroidCancellation {
    @Volatile
    private var cancelled = false
    private val lock = Any()
    private val callbacks = LinkedHashSet<() -> Unit>()

    fun cancel() {
        val pending = synchronized(lock) {
            if (cancelled) return
            cancelled = true
            callbacks.toList().also { callbacks.clear() }
        }
        pending.forEach { callback -> runCatching { callback() } }
    }

    override fun isCancelled(): Boolean = cancelled

    override fun onCancel(callback: () -> Unit): AutoCloseable {
        synchronized(lock) {
            if (!cancelled) {
                callbacks += callback
                return AutoCloseable { synchronized(lock) { callbacks.remove(callback) } }
            }
        }
        runCatching { callback() }
        return AutoCloseable { }
    }
}

/** No-op token for callers that do not need cancellation. */
object MspAndroidNoCancellation : MspAndroidCancellation {
    override fun isCancelled(): Boolean = false
}

/**
 * Android-side source of document bytes. The implementation owns provider-specific identifiers
 * and must expose only virtual-path reads to the projection layer.
 */
interface MspAndroidWorkspaceSource : AutoCloseable {
    fun readFile(
        path: MspVirtualPath,
        cancellation: MspAndroidCancellation = MspAndroidNoCancellation,
    ): MspAndroidWorkspaceResult<ByteArray>

    override fun close() = Unit
}

/**
 * Projects one provider document into the Rust runtime's bounded virtual workspace.
 *
 * This is intentionally a projection, not a host-path bridge: the native request still contains
 * only `/workspace/...`, while SAF/ContentResolver identifiers remain inside this Android layer.
 */
class MspAndroidWorkspaceProjection(
    private val source: MspAndroidWorkspaceSource,
    private val workspace: MspCommandRuntimeFfiWorkspaceHandle,
    private val cancellation: MspAndroidCancellation = MspAndroidNoCancellation,
) : AutoCloseable {
    private val lock = Any()
    private var closed = false

    fun projectFile(path: String): MspAndroidWorkspaceResult<Unit> {
        val virtualPath = when (val validation = MspVirtualPathValidator.validate(path)) {
            is MspVirtualPathValidation.Valid -> validation.path
            is MspVirtualPathValidation.Invalid ->
                return MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.OutsideMount)
        }
        if (!virtualPath.value.startsWith("/workspace/")) {
            return MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.OutsideMount)
        }
        synchronized(lock) {
            if (closed) return MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.Closed)
            if (cancellation.isCancelled()) {
                return MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.Cancelled)
            }
            return when (val read = source.readFile(virtualPath, cancellation)) {
                is MspAndroidWorkspaceResult.Failure -> read
                is MspAndroidWorkspaceResult.Success -> try {
                    workspace.putFile(virtualPath, read.value)
                    MspAndroidWorkspaceResult.Success(Unit)
                } catch (_: Throwable) {
                    MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.NativeFailure)
                }
            }
        }
    }

    override fun close() {
        synchronized(lock) {
            if (closed) return
            closed = true
            source.close()
        }
    }
}

/** Typed result for opening a ContentResolver-backed source. */
sealed interface MspAndroidWorkspaceSourceOpenResult {
    data class Success(val source: MspAndroidContentResolverWorkspaceSource) :
        MspAndroidWorkspaceSourceOpenResult

    data class Failure(val error: MspAndroidWorkspaceError) : MspAndroidWorkspaceSourceOpenResult
}

/**
 * SAF tree-backed source. Only the tree Uri is accepted at this boundary; no filesystem path,
 * shell, PATH, or process API is consulted. The Uri and provider document ids never enter errors,
 * portable requests, or result text.
 */
class MspAndroidContentResolverWorkspaceSource private constructor(
    private val resolver: ContentResolver,
    private val treeUri: Uri,
    private val rootDocumentId: String,
    private val maxFileBytes: Long,
    private val maxListEntries: Long,
) : MspAndroidWorkspaceSource {
    private val lock = Any()
    private val activeSignals = LinkedHashSet<CancellationSignal>()
    private var closed = false

    companion object {
        fun open(
            resolver: ContentResolver,
            treeUri: Uri,
            maxFileBytes: Long = MspCommandRuntimeFfiAbi.MAX_FILE_BYTES,
            maxListEntries: Long = MspCommandRuntimeFfiAbi.MAX_LIST_ENTRIES,
        ): MspAndroidWorkspaceSourceOpenResult {
            if (treeUri.scheme != "content" || treeUri.authority.isNullOrBlank()) {
                return MspAndroidWorkspaceSourceOpenResult.Failure(
                    MspAndroidWorkspaceError.InvalidTreeUri,
                )
            }
            if (maxFileBytes <= 0L || maxFileBytes > Int.MAX_VALUE || maxListEntries <= 0L) {
                return MspAndroidWorkspaceSourceOpenResult.Failure(
                    MspAndroidWorkspaceError.LimitExceeded(maxFileBytes.coerceAtLeast(0L)),
                )
            }
            val rootId = try {
                DocumentsContract.getTreeDocumentId(treeUri)
            } catch (_: Throwable) {
                null
            }
            if (rootId.isNullOrEmpty()) {
                return MspAndroidWorkspaceSourceOpenResult.Failure(
                    MspAndroidWorkspaceError.InvalidTreeUri,
                )
            }
            return MspAndroidWorkspaceSourceOpenResult.Success(
                MspAndroidContentResolverWorkspaceSource(
                    resolver,
                    treeUri,
                    rootId,
                    maxFileBytes,
                    maxListEntries,
                ),
            )
        }
    }

    override fun readFile(
        path: MspVirtualPath,
        cancellation: MspAndroidCancellation,
    ): MspAndroidWorkspaceResult<ByteArray> {
        return withProviderSignal(cancellation) { signal ->
            val pathText = path.value
            if (!pathText.startsWith("/workspace/")) {
                return@withProviderSignal MspAndroidWorkspaceResult.Failure(
                    MspAndroidWorkspaceError.OutsideMount,
                )
            }
            val document = when (val resolved = resolveDocument(pathText, cancellation, signal)) {
                is MspAndroidWorkspaceResult.Failure -> return@withProviderSignal resolved
                is MspAndroidWorkspaceResult.Success -> resolved.value
            }
            if (document.isDirectory) {
                return@withProviderSignal MspAndroidWorkspaceResult.Failure(
                    MspAndroidWorkspaceError.NotDirectory,
                )
            }
            document.sizeBytes?.let { size ->
                if (size > maxFileBytes) {
                    return@withProviderSignal MspAndroidWorkspaceResult.Failure(
                        MspAndroidWorkspaceError.LimitExceeded(maxFileBytes),
                    )
                }
            }
            val documentUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, document.id)
            val descriptor = resolver.openFileDescriptor(documentUri, "r", signal)
                ?: return@withProviderSignal MspAndroidWorkspaceResult.Failure(
                    MspAndroidWorkspaceError.NotFound,
                )
            ParcelFileDescriptor.AutoCloseInputStream(descriptor).use { input ->
                MspAndroidWorkspaceResult.Success(readBounded(input, cancellation))
            }
        }
    }

    private fun resolveDocument(
        path: String,
        cancellation: MspAndroidCancellation,
        signal: CancellationSignal,
    ): MspAndroidWorkspaceResult<DocumentRef> {
        val components = path.removePrefix("/workspace/").split('/')
        var currentId = rootDocumentId
        var current: DocumentRef? = null
        for (component in components) {
            if (cancellation.isCancelled()) {
                return MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.Cancelled)
            }
            val next = when (val result = findChild(currentId, component, cancellation, signal)) {
                is MspAndroidWorkspaceResult.Failure -> return result
                is MspAndroidWorkspaceResult.Success -> result.value
            }
            current = next
            currentId = next.id
        }
        return current?.let { MspAndroidWorkspaceResult.Success(it) }
            ?: MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.NotFound)
    }

    private fun findChild(
        parentId: String,
        expectedName: String,
        cancellation: MspAndroidCancellation,
        signal: CancellationSignal,
    ): MspAndroidWorkspaceResult<DocumentRef> {
        if (!isSafeChildName(expectedName)) {
            return MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.NotFound)
        }
        val childrenUri = try {
            DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentId)
        } catch (_: Throwable) {
            return MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.InvalidTreeUri)
        }
        return queryChildren(childrenUri, expectedName, cancellation, signal)
    }

    private fun queryChildren(
        childrenUri: Uri,
        expectedName: String,
        cancellation: MspAndroidCancellation,
        signal: CancellationSignal,
    ): MspAndroidWorkspaceResult<DocumentRef> {
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
        )
        return try {
            val cursor = resolver.query(childrenUri, projection, null, null, null, signal)
                ?: return MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.ProviderFailure)
            cursor.use { rows ->
                val idColumn = rows.getColumnIndex(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
                val nameColumn = rows.getColumnIndex(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
                val mimeColumn = rows.getColumnIndex(DocumentsContract.Document.COLUMN_MIME_TYPE)
                val sizeColumn = rows.getColumnIndex(DocumentsContract.Document.COLUMN_SIZE)
                if (idColumn < 0 || nameColumn < 0 || mimeColumn < 0) {
                    return MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.ProviderFailure)
                }
                var count = 0L
                var match: DocumentRef? = null
                while (rows.moveToNext()) {
                    if (cancellation.isCancelled()) {
                        return MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.Cancelled)
                    }
                    count++
                    if (count > maxListEntries) {
                        return MspAndroidWorkspaceResult.Failure(
                            MspAndroidWorkspaceError.LimitExceeded(maxFileBytes),
                        )
                    }
                    val id = rows.getString(idColumn) ?: continue
                    val name = rows.getString(nameColumn) ?: continue
                    if (!isSafeChildName(name) || name != expectedName) continue
                    val mime = rows.getString(mimeColumn) ?: continue
                    val size = if (sizeColumn >= 0 && !rows.isNull(sizeColumn)) {
                        rows.getLong(sizeColumn).takeIf { it >= 0L }
                    } else {
                        null
                    }
                    val candidate = DocumentRef(
                        id = id,
                        isDirectory = mime == DocumentsContract.Document.MIME_TYPE_DIR,
                        sizeBytes = size,
                    )
                    if (match != null) {
                        return MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.ProviderFailure)
                    }
                    match = candidate
                }
                match?.let { MspAndroidWorkspaceResult.Success(it) }
                    ?: MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.NotFound)
            }
        } catch (error: OperationCanceledException) {
            throw error
        } catch (_: SecurityException) {
            MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.PermissionDenied)
        } catch (_: FileNotFoundException) {
            MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.NotFound)
        } catch (_: IllegalArgumentException) {
            MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.InvalidTreeUri)
        } catch (_: Throwable) {
            MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.ProviderFailure)
        }
    }

    private fun readBounded(
        input: InputStream,
        cancellation: MspAndroidCancellation,
    ): ByteArray {
        val output = ByteArrayOutputStream(minOf(maxFileBytes, 64L * 1024L).toInt())
        val buffer = ByteArray(16 * 1024)
        var total = 0L
        while (true) {
            if (cancellation.isCancelled()) throw MspAndroidCancelledException
            val count = input.read(buffer)
            if (count < 0) break
            total += count.toLong()
            if (total > maxFileBytes) throw MspAndroidLimitException
            output.write(buffer, 0, count)
        }
        return output.toByteArray()
    }

    private fun <T> withProviderSignal(
        cancellation: MspAndroidCancellation,
        block: (CancellationSignal) -> MspAndroidWorkspaceResult<T>,
    ): MspAndroidWorkspaceResult<T> {
        if (cancellation.isCancelled()) {
            return MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.Cancelled)
        }
        val signal = CancellationSignal()
        synchronized(lock) {
            if (closed) return MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.Closed)
            activeSignals += signal
        }
        val registration = cancellation.onCancel { signal.cancel() }
        return try {
            block(signal)
        } catch (_: OperationCanceledException) {
            MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.Cancelled)
        } catch (_: MspAndroidCancelledException) {
            MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.Cancelled)
        } catch (_: MspAndroidLimitException) {
            MspAndroidWorkspaceResult.Failure(
                MspAndroidWorkspaceError.LimitExceeded(maxFileBytes),
            )
        } catch (_: SecurityException) {
            MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.PermissionDenied)
        } catch (_: FileNotFoundException) {
            MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.NotFound)
        } catch (_: IOException) {
            MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.ProviderFailure)
        } catch (_: IllegalArgumentException) {
            MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.InvalidTreeUri)
        } catch (_: Throwable) {
            MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.ProviderFailure)
        } finally {
            registration.close()
            synchronized(lock) { activeSignals.remove(signal) }
        }
    }

    private fun isSafeChildName(name: String): Boolean {
        if (name.isEmpty() || name == "." || name == "..") return false
        if (name.equals(".msp", ignoreCase = true)) return false
        if (name.any { it == '/' || it == '\\' || it == ':' || it.code <= 0x1F || it.code in 0x7F..0x9F }) {
            return false
        }
        return !name.any { it.isSurrogate() }
    }

    override fun close() {
        synchronized(lock) {
            if (closed) return
            closed = true
            activeSignals.forEach { signal -> signal.cancel() }
            activeSignals.clear()
        }
    }

    private data class DocumentRef(
        val id: String,
        val isDirectory: Boolean,
        val sizeBytes: Long?,
    )

    private object MspAndroidCancelledException : IOException()
    private object MspAndroidLimitException : IOException()
}
