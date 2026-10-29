package dev.lorehaven.forklift

import android.app.Activity
import android.content.ContentValues
import android.net.Uri
import android.os.Build
import android.provider.MediaStore
import android.webkit.MimeTypeMap
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.ConcurrentHashMap

/**
 * Brings stored files back, streaming each from the server straight into the media library, so no file is held in
 * memory. A photo goes to Pictures/Forklift, a video to Movies/Forklift, and a file of a backed-up folder to
 * Downloads/Forklift/<folder> (the app only has read access to a folder the person picked, so it cannot write into it).
 * None of this needs a permission: an app may add files to the media library.
 *
 * Methods: `save {id, url, token, name, kind: images|video|folder, folder}` (answers at once; events `done {id}` or
 * `failed {id, message}`), `cancel {id}`.
 */
class RestorePlugin(private val activity: Activity) : Plugin("restore") {
    private val running = ConcurrentHashMap<String, HttpURLConnection>()

    override fun call(method: String, args: JSONObject, call: Call) {
        when (method) {
            "save" -> {
                save(args)
                call.resolve()
            }
            "cancel" -> {
                running.remove(args.getString("id"))?.disconnect()
                call.resolve()
            }
            else -> call.reject("unknown_method", "restore has no method `$method`")
        }
    }

    override fun disconnected() {
        running.values.forEach { it.disconnect() }
        running.clear()
    }

    private fun mime(name: String): String {
        val extension = name.substringAfterLast('.', "").lowercase()
        return MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension) ?: "application/octet-stream"
    }

    /** Where the file is created: the collection, and the folder below it. */
    private fun destination(kind: String, folder: String): Pair<Uri, String> {
        val volume = MediaStore.VOLUME_EXTERNAL_PRIMARY
        return when (kind) {
            "video" -> MediaStore.Video.Media.getContentUri(volume) to "Movies/Forklift"
            "folder" -> MediaStore.Downloads.getContentUri(volume) to "Download/Forklift/" + folder.replace('/', '_')
            else -> MediaStore.Images.Media.getContentUri(volume) to "Pictures/Forklift"
        }
    }

    private fun save(args: JSONObject) {
        val id = args.getString("id")
        Thread {
            var connection: HttpURLConnection? = null
            var target: Uri? = null
            try {
                if (Build.VERSION.SDK_INT < 29) throw IllegalStateException("restoring needs Android 10 or newer")
                val name = args.getString("name")
                val (collection, path) = destination(args.getString("kind"), args.optString("folder"))
                val resolver = activity.contentResolver
                connection = (URL(args.getString("url")).openConnection() as HttpURLConnection).apply {
                    setRequestProperty("Authorization", "Bearer " + args.getString("token"))
                    connectTimeout = 15_000
                    readTimeout = 120_000
                }
                running[id] = connection
                val status = connection.responseCode
                if (status !in 200..299) throw IllegalStateException("the server answered $status")
                val values = ContentValues().apply {
                    put(MediaStore.MediaColumns.DISPLAY_NAME, name)
                    put(MediaStore.MediaColumns.MIME_TYPE, mime(name))
                    put(MediaStore.MediaColumns.RELATIVE_PATH, path)
                    put(MediaStore.MediaColumns.IS_PENDING, 1)
                }
                target = resolver.insert(collection, values) ?: throw IllegalStateException("the media library refused the file")
                (resolver.openOutputStream(target) ?: throw IllegalStateException("cannot write the file")).use { output ->
                    connection.inputStream.use { it.copyTo(output, 64 * 1024) }
                }
                // Only now is it a file other apps may see.
                resolver.update(target, ContentValues().apply { put(MediaStore.MediaColumns.IS_PENDING, 0) }, null, null)
                emit("done", JSONObject().put("id", id))
            } catch (e: Exception) {
                // A half-written file is not left behind.
                target?.let { runCatching { activity.contentResolver.delete(it, null, null) } }
                emit("failed", JSONObject().put("id", id).put("message", e.message ?: e.javaClass.simpleName))
            } finally {
                running.remove(id)
                connection?.disconnect()
            }
        }.start()
    }
}
