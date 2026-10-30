package dev.lorehaven.forklift

import android.Manifest
import android.app.Activity
import android.content.ContentResolver
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.util.Base64
import org.json.JSONArray
import org.json.JSONObject
import java.io.FileInputStream
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.ConcurrentHashMap

/**
 * Files the person owns: the media library (MediaStore), folders and files they pick with the
 * system's picker (the Storage Access Framework, with the grant kept across restarts), and ways to
 * move their bytes without holding them in the page: `read` in chunks, and `upload`, which streams a
 * file to a URL from native code and reports progress as events.
 *
 * Methods: `media {kind: images|video|audio, limit, offset}`, `pick_folder`, `pick_files`,
 * `list_folder {uri, limit}`, `stat {uri}`, `read {uri, offset, length}` (base64, at most 256 KiB),
 * `upload {id, uri, url, method, headers}` (events `progress {id, sent, total}` and
 * `done {id, status, body}` or `failed {id, message}`), `cancel_upload {id}`.
 */
class FilesPlugin(private val activity: Activity) : Plugin("files") {
    private val resolver: ContentResolver get() = activity.contentResolver
    private val uploads = ConcurrentHashMap<String, HttpURLConnection>()
    private var pending: Call? = null

    override fun call(method: String, args: JSONObject, call: Call) {
        try {
            when (method) {
                "media" -> media(args, call)
                "pick_folder" -> pick(call, Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).addFlags(GRANTS), PICK_FOLDER)
                "pick_files" -> pick(
                    call,
                    Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("*/*")
                        .putExtra(Intent.EXTRA_ALLOW_MULTIPLE, args.optBoolean("multiple", true)).addFlags(GRANTS),
                    PICK_FILES
                )
                "list_folder" -> call.resolve(list(Uri.parse(args.getString("uri")), args.optInt("limit", 500)))
                "stat" -> call.resolve(stat(Uri.parse(args.getString("uri"))))
                "read" -> call.resolve(read(Uri.parse(args.getString("uri")), args.optLong("offset", 0), args.optInt("length", CHUNK).coerceIn(1, CHUNK)))
                "upload" -> upload(args, call)
                "cancel_upload" -> { uploads.remove(args.getString("id"))?.disconnect(); call.resolve() }
                else -> call.reject("unknown_method", "files has no method `$method`")
            }
        } catch (e: SecurityException) {
            call.reject("permission_denied", e.message ?: "not allowed")
        } catch (e: java.io.FileNotFoundException) {
            call.reject("not_found", e.message ?: "no such file")
        }
    }

    private fun media(args: JSONObject, call: Call) {
        val kind = args.optString("kind", "images")
        val (uri, permission) = when (kind) {
            "images" -> MediaStore.Images.Media.EXTERNAL_CONTENT_URI to Manifest.permission.READ_MEDIA_IMAGES
            "video" -> MediaStore.Video.Media.EXTERNAL_CONTENT_URI to Manifest.permission.READ_MEDIA_VIDEO
            "audio" -> MediaStore.Audio.Media.EXTERNAL_CONTENT_URI to Manifest.permission.READ_MEDIA_AUDIO
            else -> return call.reject("invalid_args", "kind is images, video or audio")
        }
        // Before Android 13 one permission covers all the media.
        val needed = if (Build.VERSION.SDK_INT >= 33) permission else Manifest.permission.READ_EXTERNAL_STORAGE
        call.permissions(needed) { granted ->
            if (!granted) return@permissions call.reject("permission_denied", "the media library is not allowed")
            val query = Bundle().apply {
                putInt(ContentResolver.QUERY_ARG_LIMIT, args.optInt("limit", 50).coerceIn(1, 5000))
                putInt(ContentResolver.QUERY_ARG_OFFSET, args.optInt("offset", 0))
                putStringArray(ContentResolver.QUERY_ARG_SORT_COLUMNS, arrayOf(MediaStore.MediaColumns.DATE_MODIFIED))
                putInt(ContentResolver.QUERY_ARG_SORT_DIRECTION, ContentResolver.QUERY_SORT_DIRECTION_DESCENDING)
            }
            val columns = arrayOf(
                MediaStore.MediaColumns._ID, MediaStore.MediaColumns.DISPLAY_NAME, MediaStore.MediaColumns.SIZE,
                MediaStore.MediaColumns.MIME_TYPE, MediaStore.MediaColumns.DATE_MODIFIED,
            )
            val list = JSONArray()
            resolver.query(uri, columns, query, null)?.use { c ->
                while (c.moveToNext()) {
                    list.put(
                        JSONObject()
                            .put("uri", android.content.ContentUris.withAppendedId(uri, c.getLong(0)).toString())
                            .put("name", c.getString(1)).put("size", c.getLong(2))
                            .put("mime", c.getString(3)).put("modified_ms", c.getLong(4) * 1000)
                    )
                }
            }
            call.resolve(list)
        }
    }

    private fun pick(call: Call, intent: Intent, code: Int) {
        pending = call
        activity.startActivityForResult(intent, code)
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        if (requestCode != PICK_FOLDER && requestCode != PICK_FILES) return
        val call = pending ?: return
        pending = null
        if (resultCode != Activity.RESULT_OK || data == null) return call.reject("cancelled", "nothing was picked")
        val uris = data.clipData?.let { clip -> (0 until clip.itemCount).map { clip.getItemAt(it).uri } } ?: listOfNotNull(data.data)
        // Keep the grant, so the folder or file is still readable after a restart.
        uris.forEach { runCatching { resolver.takePersistableUriPermission(it, Intent.FLAG_GRANT_READ_URI_PERMISSION) } }
        call.resolve(JSONObject().put("uris", JSONArray(uris.map { it.toString() })))
    }

    private fun list(uri: Uri, limit: Int): JSONArray {
        val docId = if (DocumentsContract.isDocumentUri(activity, uri)) DocumentsContract.getDocumentId(uri) else DocumentsContract.getTreeDocumentId(uri)
        val tree = DocumentsContract.buildTreeDocumentUri(uri.authority, DocumentsContract.getTreeDocumentId(uri))
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, docId)
        val columns = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE, DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED,
        )
        val list = JSONArray()
        resolver.query(children, columns, null, null, null)?.use { c ->
            while (c.moveToNext() && list.length() < limit) {
                val mime = c.getString(2)
                list.put(
                    JSONObject()
                        .put("uri", DocumentsContract.buildDocumentUriUsingTree(tree, c.getString(0)).toString())
                        .put("name", c.getString(1)).put("mime", mime)
                        .put("is_dir", mime == DocumentsContract.Document.MIME_TYPE_DIR)
                        .put("size", if (c.isNull(3)) -1 else c.getLong(3)).put("modified_ms", if (c.isNull(4)) 0 else c.getLong(4))
                )
            }
        }
        return list
    }

    private fun stat(uri: Uri): JSONObject {
        resolver.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME, android.provider.OpenableColumns.SIZE), null, null, null)?.use { c ->
            if (c.moveToFirst()) {
                return JSONObject().put("name", c.getString(0)).put("size", if (c.isNull(1)) -1 else c.getLong(1)).put("mime", resolver.getType(uri))
            }
        }
        throw java.io.FileNotFoundException(uri.toString())
    }

    private fun read(uri: Uri, offset: Long, length: Int): JSONObject {
        val descriptor = resolver.openFileDescriptor(uri, "r") ?: throw java.io.FileNotFoundException(uri.toString())
        descriptor.use { fd ->
            val channel = FileInputStream(fd.fileDescriptor).channel
            val size = fd.statSize
            channel.position(offset)
            val buffer = java.nio.ByteBuffer.allocate(length)
            val count = channel.read(buffer).coerceAtLeast(0)
            return JSONObject()
                .put("data", Base64.encodeToString(buffer.array(), 0, count, Base64.NO_WRAP))
                .put("size", size).put("eof", size >= 0 && offset + count >= size)
        }
    }

    private fun upload(args: JSONObject, call: Call) {
        val id = args.getString("id")
        val uri = Uri.parse(args.getString("uri"))
        val url = URL(args.getString("url"))
        val headers = args.optJSONObject("headers") ?: JSONObject()
        val total = runCatching { stat(uri).getLong("size") }.getOrDefault(-1)
        call.resolve(JSONObject().put("id", id).put("total", total))
        Thread {
            var connection: HttpURLConnection? = null
            try {
                connection = (url.openConnection() as HttpURLConnection).apply {
                    requestMethod = args.optString("method", "PUT")
                    doOutput = true
                    headers.keys().forEach { setRequestProperty(it, headers.getString(it)) }
                    if (total >= 0) setFixedLengthStreamingMode(total) else setChunkedStreamingMode(64 * 1024)
                }
                uploads[id] = connection
                var sent = 0L
                var last = 0L
                (resolver.openInputStream(uri) ?: throw java.io.FileNotFoundException(uri.toString())).use { input ->
                    connection.outputStream.use { output ->
                        val buffer = ByteArray(64 * 1024)
                        while (true) {
                            val n = input.read(buffer)
                            if (n < 0) break
                            output.write(buffer, 0, n)
                            sent += n
                            val now = System.currentTimeMillis()
                            if (now - last >= 200) {
                                last = now
                                emit("progress", JSONObject().put("id", id).put("sent", sent).put("total", total))
                            }
                        }
                    }
                }
                emit("progress", JSONObject().put("id", id).put("sent", sent).put("total", total))
                val status = connection.responseCode
                val body = (if (status < 400) connection.inputStream else connection.errorStream)?.use { String(it.readNBytes(4096)) } ?: ""
                emit("done", JSONObject().put("id", id).put("status", status).put("body", body))
            } catch (e: Exception) {
                emit("failed", JSONObject().put("id", id).put("message", e.message ?: e.javaClass.simpleName))
            } finally {
                uploads.remove(id)
                connection?.disconnect()
            }
        }.start()
    }

    private companion object {
        const val PICK_FOLDER = 0x5101
        const val PICK_FILES = 0x5102
        const val CHUNK = 256 * 1024
        const val GRANTS = Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or Intent.FLAG_GRANT_PREFIX_URI_PERMISSION
    }
}