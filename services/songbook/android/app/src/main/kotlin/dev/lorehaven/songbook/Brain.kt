package dev.lorehaven.songbook

import android.content.Context
import android.util.Log
import org.json.JSONObject
import java.io.BufferedReader
import java.io.File
import java.io.IOException
import java.io.InputStreamReader
import java.util.concurrent.CompletableFuture
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import java.util.concurrent.TimeoutException
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread

/**
 * The Rust program that is the brain of the app (`native/`, built by `quench android build-native`), run as a process of its
 * own: no JNI, nothing of it is loaded into the app. It signs in, talks to the service and makes the tree the car and the
 * phone browse; this class only asks it things, one JSON line each way (see `native/src/proto.rs`), and starts it again if it
 * ever dies. Playback, downloads and the media session stay in Kotlin: only an app on the Android side can do those.
 *
 * Blocking: call it off the main thread (the answer to `status` is fast, as the files are read from the app's own storage).
 */
class Brain private constructor(private val context: Context) {
    private val ids = AtomicInteger(1)
    private val pending = ConcurrentHashMap<Int, CompletableFuture<JSONObject>>()
    private var process: Process? = null
    private var input: java.io.Writer? = null

    @Synchronized
    private fun ensure(): java.io.Writer {
        process?.takeIf { it.isAlive }?.let { return input!! }
        val file = File(context.applicationInfo.nativeLibraryDir, PROGRAM)
        if (!file.exists()) throw IOException("the native program is not in this build")
        val started = ProcessBuilder(file.absolutePath)
            .directory(context.filesDir)
            .apply {
                environment()["HOME"] = context.filesDir.absolutePath
                SessionKey.hex(context)?.let { environment()["FORGE_SESSION_KEY"] = it }
            }
            .start()
        process = started
        input = started.outputStream.bufferedWriter()
        thread(name = "brain-stderr", isDaemon = true) {
            try { started.errorStream.bufferedReader().forEachLine { Log.w(TAG, it) } } catch (_: IOException) {}
        }
        thread(name = "brain-reader", isDaemon = true) {
            try {
                BufferedReader(InputStreamReader(started.inputStream)).forEachLine { line ->
                    val answer = try { JSONObject(line) } catch (e: Exception) { return@forEachLine }
                    pending.remove(answer.optInt("id"))?.complete(answer)
                }
            } catch (_: IOException) {
            }
            // It ended: whoever waits is told, and the next call starts it again.
            val gone = IOException("the native program ended")
            pending.keys.toList().forEach { pending.remove(it)?.completeExceptionally(gone) }
        }
        return input!!
    }

    /**
     * One request; the `ok` of the answer. [SignedOut] when the session is over, [Unreachable] when the service is not there,
     * [IllegalArgumentException] for a refusal (its message is in words for the person).
     */
    fun call(request: JSONObject, timeoutMs: Long = 60_000): JSONObject {
        val id = ids.getAndIncrement()
        val future = CompletableFuture<JSONObject>()
        pending[id] = future
        try {
            synchronized(this) {
                val writer = ensure()
                writer.write(request.put("id", id).toString() + "\n")
                writer.flush()
            }
        } catch (e: IOException) {
            pending.remove(id)
            throw Unreachable(e.message ?: "the native program does not run")
        }
        val answer = try {
            future.get(timeoutMs, TimeUnit.MILLISECONDS)
        } catch (e: TimeoutException) {
            pending.remove(id)
            throw Unreachable("no answer in time")
        } catch (e: java.util.concurrent.ExecutionException) {
            throw Unreachable(e.cause?.message ?: "the native program ended")
        }
        answer.optJSONObject("err")?.let { err ->
            val message = err.optString("message")
            when (err.optString("code")) {
                "signed_out" -> throw SignedOut()
                "unreachable" -> throw Unreachable(message)
                "refused" -> throw IllegalArgumentException(message)
                else -> throw IOException(message)
            }
        }
        return answer.optJSONObject("ok") ?: JSONObject()
    }

    fun call(op: String, vararg fields: Pair<String, Any?>): JSONObject =
        call(JSONObject().put("op", op).also { o -> fields.forEach { (k, v) -> o.put(k, v ?: JSONObject.NULL) } })

    companion object {
        private const val TAG = "SongbookBrain"
        private const val PROGRAM = "libquench_native.so"

        @Volatile private var instance: Brain? = null

        fun of(context: Context): Brain = instance ?: synchronized(this) {
            instance ?: Brain(context.applicationContext).also { instance = it }
        }
    }
}
