package com.reados.msp.commandruntime

import android.database.Cursor
import android.database.MatrixCursor
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import android.provider.DocumentsProvider
import java.io.FileNotFoundException

/** Minimal in-memory DocumentsProvider used only by the Android instrumentation test. */
class MspTestDocumentsProvider : DocumentsProvider() {
    companion object {
        const val AUTHORITY = "com.reados.msp.commandruntime.test.documents"
        const val ROOT_ID = "root"
        private const val MIME_DIR = "vnd.android.document/directory"
        private val files = mapOf(
            "doc.bin" to byteArrayOf(0, 0xff.toByte(), 1, 0),
            "nested/inner.txt" to "android provider\n".toByteArray(Charsets.UTF_8),
        )

        private val DEFAULT_ROOT_PROJECTION = arrayOf(
            DocumentsContract.Root.COLUMN_ROOT_ID,
            DocumentsContract.Root.COLUMN_DOCUMENT_ID,
            DocumentsContract.Root.COLUMN_TITLE,
            DocumentsContract.Root.COLUMN_FLAGS,
            DocumentsContract.Root.COLUMN_MIME_TYPES,
        )
        private val DEFAULT_DOCUMENT_PROJECTION = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_FLAGS,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED,
        )
    }

    override fun onCreate(): Boolean = true

    override fun queryRoots(projection: Array<out String>?): Cursor {
        val cursor = MatrixCursor(projection ?: DEFAULT_ROOT_PROJECTION)
        val row = cursor.newRow()
        row.add(DocumentsContract.Root.COLUMN_ROOT_ID, ROOT_ID)
        row.add(DocumentsContract.Root.COLUMN_DOCUMENT_ID, ROOT_ID)
        row.add(DocumentsContract.Root.COLUMN_TITLE, "ReadOS test documents")
        row.add(DocumentsContract.Root.COLUMN_FLAGS, DocumentsContract.Root.FLAG_LOCAL_ONLY)
        row.add(DocumentsContract.Root.COLUMN_MIME_TYPES, "*/*")
        return cursor
    }

    override fun queryDocument(documentId: String, projection: Array<out String>?): Cursor =
        documentCursor(projection, documentId)

    override fun queryChildDocuments(
        parentDocumentId: String,
        projection: Array<out String>?,
        sortOrder: String?,
    ): Cursor {
        val children = when (parentDocumentId) {
            ROOT_ID -> listOf("doc.bin", "nested")
            "nested" -> listOf("nested/inner.txt")
            else -> emptyList()
        }
        val cursor = MatrixCursor(projection ?: DEFAULT_DOCUMENT_PROJECTION)
        children.forEach { documentId -> addDocumentRow(cursor, documentId) }
        return cursor
    }

    override fun openDocument(
        documentId: String,
        mode: String,
        signal: CancellationSignal?,
    ): ParcelFileDescriptor {
        val bytes = files[documentId] ?: throw FileNotFoundException()
        val pipe = ParcelFileDescriptor.createPipe()
        Thread {
            try {
                pipe[1].use { descriptor ->
                    ParcelFileDescriptor.AutoCloseOutputStream(descriptor).use { output ->
                        if (signal?.isCanceled == true) return@Thread
                        output.write(bytes)
                        output.flush()
                    }
                }
            } catch (_: Throwable) {
                runCatching { pipe[1].close() }
            }
        }.start()
        return pipe[0]
    }

    override fun getDocumentType(documentId: String): String =
        if (isDirectory(documentId)) MIME_DIR else "application/octet-stream"

    override fun isChildDocument(parentDocumentId: String, documentId: String): Boolean =
        when (parentDocumentId) {
            ROOT_ID -> documentId == "doc.bin" || documentId == "nested"
            "nested" -> documentId == "nested/inner.txt"
            else -> false
        }

    private fun documentCursor(projection: Array<out String>?, documentId: String): Cursor {
        val cursor = MatrixCursor(projection ?: DEFAULT_DOCUMENT_PROJECTION)
        if (documentId == ROOT_ID || files.containsKey(documentId) || isDirectory(documentId)) {
            addDocumentRow(cursor, documentId)
        }
        return cursor
    }

    private fun addDocumentRow(cursor: MatrixCursor, documentId: String) {
        val row = cursor.newRow()
        row.add(DocumentsContract.Document.COLUMN_DOCUMENT_ID, documentId)
        row.add(
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            if (documentId == ROOT_ID) "root" else documentId.substringAfterLast('/'),
        )
        row.add(
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            if (isDirectory(documentId)) MIME_DIR else "application/octet-stream",
        )
        row.add(
            DocumentsContract.Document.COLUMN_SIZE,
            files[documentId]?.size?.toLong() ?: null,
        )
        row.add(DocumentsContract.Document.COLUMN_FLAGS, 0)
        row.add(DocumentsContract.Document.COLUMN_LAST_MODIFIED, 0L)
    }

    private fun isDirectory(documentId: String): Boolean =
        documentId == ROOT_ID || files.keys.any { it.startsWith("$documentId/") }

}
