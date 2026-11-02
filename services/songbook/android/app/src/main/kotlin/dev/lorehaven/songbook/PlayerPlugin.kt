package dev.lorehaven.songbook

import android.app.Activity
import android.content.ComponentName
import android.net.Uri
import android.os.Handler
import android.os.Looper
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.Executors

/**
 * The sound of the screens: the commands the player's screens give an audio engine (`player/src/engine.rs`) done on the
 * playback service's own session, so that what the screens play is what the notification, the lock screen and Android Auto
 * show and control. Its events (what plays, the queue, what was played, what is kept) go back to the screens.
 *
 * The queue is kept in play order in the session's playlist (shuffling is done here, not by the player, so that the screens'
 * indexes mean what they do in the browser). Everything here runs on the main thread, where the controller lives.
 */
class PlayerPlugin(private val activity: Activity) : Plugin("player") {
    private val main = Handler(Looper.getMainLooper())
    private val work = Executors.newCachedThreadPool()
    private val brain = Brain.of(activity)
    private val session = Session.of(activity)
    private val offline = Offline.of(activity)
    private val settings = Settings.of(activity)
    private var controller: MediaController? = null
    private val waiting = mutableListOf<(MediaController) -> Unit>()
    private var closed = false

    /** The queue in the order it was given, for when shuffling is turned off. */
    private var original = mutableListOf<Long>()
    private var shuffle = false
    private var stopWatching: (() -> Unit)? = null
    private var stopWatchingKept: (() -> Unit)? = null

    private val ticker = object : Runnable {
        override fun run() {
            controller?.let { if (it.isPlaying) state(it) }
            if (!closed) main.postDelayed(this, TICK_MS)
        }
    }

    init {
        main.post { connect() }
    }

    private fun connect() {
        val token = SessionToken(activity, ComponentName(activity, PlaybackService::class.java))
        val future = MediaController.Builder(activity, token).buildAsync()
        future.addListener({
            if (closed) {
                future.cancel(false)
                return@addListener
            }
            val c = try { future.get() } catch (e: Exception) { return@addListener }
            controller = c
            c.addListener(object : Player.Listener {
                override fun onEvents(player: Player, events: Player.Events) {
                    state(c)
                    if (events.containsAny(Player.EVENT_MEDIA_ITEM_TRANSITION, Player.EVENT_TIMELINE_CHANGED)) {
                        queue(c)
                        if (events.contains(Player.EVENT_MEDIA_ITEM_TRANSITION)) recent()
                    }
                }

                override fun onPlayerError(error: androidx.media3.common.PlaybackException) {
                    emit("event", JSONObject().put("ev", "error").put("message", "Cannot play this song"))
                }
            })
            c.repeatModeFrom(settings)
            waiting.toList().also { waiting.clear() }.forEach { it(c) }
            state(c)
            queue(c)
        }, main::post)
        // What is kept follows the downloads, and the crossfade follows the setting wherever it is changed.
        stopWatchingKept = offline.onChange { main.post { kept() } }
        stopWatching = settings.onChange { main.post { controller?.let { state(it) } } }
        main.postDelayed(ticker, TICK_MS)
    }

    private fun Player.repeatModeFrom(@Suppress("UNUSED_PARAMETER") settings: Settings) {
        shuffleModeEnabled = false
    }

    /** The screens went away: stop listening (the playback goes on, it is the service's). */
    fun close() {
        closed = true
        stopWatching?.invoke()
        stopWatchingKept?.invoke()
        controller?.release()
        controller = null
        work.shutdown()
    }

    override fun disconnected() {}

    override fun call(method: String, args: JSONObject, call: Call) {
        when (method) {
            "send" -> {
                val command = JSONObject(args.getString("command"))
                main.post { with(command) }
                call.resolve()
            }
            "ask" -> {
                main.post { ask() }
                call.resolve()
            }
            else -> call.reject("unknown_method", "no method `$method`")
        }
    }

    /** Runs `block` once the controller is connected (at once if it is). */
    private fun with(command: JSONObject) {
        val c = controller
        if (c != null) run(c, command) else waiting.add { run(it, command) }
    }

    // ── Items ────────────────────────────────────────────────────────────────

    private fun item(id: Long): MediaItem = MediaItem.Builder()
        .setMediaId("s:q:$id")
        .setUri("${session.api}/stream/$id")
        .setMediaMetadata(
            MediaMetadata.Builder()
                .setArtworkUri(Uri.parse("${session.api}/art/song/$id?w=320"))
                .setIsPlayable(true)
                .setIsBrowsable(false)
                .setMediaType(MediaMetadata.MEDIA_TYPE_MUSIC)
                .build(),
        )
        .build()

    private fun idOf(item: MediaItem?): Long = item?.mediaId?.substringAfterLast(':')?.toLongOrNull() ?: 0L

    private fun ids(array: JSONArray?): List<Long> = (0 until (array?.length() ?: 0)).map { array!!.getLong(it) }

    private fun shuffled(list: List<Long>, first: Int?): List<Long> {
        val rest = list.filterIndexed { i, _ -> i != first }.shuffled()
        return if (first == null || first !in list.indices) rest else listOf(list[first]) + rest
    }

    // ── Commands ─────────────────────────────────────────────────────────────

    private fun run(c: MediaController, command: JSONObject) {
        when (command.optString("op")) {
            "load" -> load(c, ids(command.optJSONArray("queue")), command.optInt("index", 0), command.optBoolean("shuffle"))
            "play" -> {
                if (c.mediaItemCount > 0 && c.currentMediaItemIndex < 0) c.seekToDefaultPosition(0)
                c.prepare()
                c.play()
            }
            "pause" -> c.pause()
            "toggle" -> if (c.isPlaying) c.pause() else {
                c.prepare()
                c.play()
            }
            "next" -> c.seekToNext()
            "prev" -> c.seekToPrevious()
            "jump" -> {
                val at = command.optInt("index", -1)
                if (at in 0 until c.mediaItemCount) {
                    c.seekTo(at, 0)
                    c.play()
                }
            }
            "seek" -> c.seekTo((command.optDouble("to", 0.0).coerceAtLeast(0.0) * 1000).toLong())
            "shuffle" -> setShuffle(c, command.optBoolean("on"))
            "repeat" -> {
                c.repeatMode = when (command.optString("mode")) {
                    "all" -> Player.REPEAT_MODE_ALL
                    "one" -> Player.REPEAT_MODE_ONE
                    else -> Player.REPEAT_MODE_OFF
                }
                state(c)
            }
            "fade" -> settings.crossfadeSecs = command.optDouble("secs", 0.0).toInt()
            "volume" -> {
                c.volume = command.optDouble("v", 1.0).toFloat().coerceIn(0f, 1f)
                state(c)
            }
            "meta" -> {} // The service looks a song up when it starts it, so the lock screen and the car have it.
            "enqueue" -> enqueue(c, ids(command.optJSONArray("ids")), command.optBoolean("next"))
            "remove" -> {
                val at = command.optInt("index", -1)
                if (at in 0 until c.mediaItemCount && at != c.currentMediaItemIndex) {
                    original.remove(idOf(c.getMediaItemAt(at)))
                    c.removeMediaItem(at)
                }
            }
            "cache" -> {
                settings.cacheGiB = command.optInt("gib", Settings.DEFAULT_CACHE_GIB)
                cache()
            }
            "keep" -> keep(ids(command.optJSONArray("ids")))
            "drop" -> drop(ids(command.optJSONArray("ids")))
            "ask" -> ask()
        }
    }

    private fun load(c: MediaController, queue: List<Long>, index: Int, shuffleOn: Boolean) {
        if (queue.isEmpty()) return
        original = queue.toMutableList()
        shuffle = shuffleOn
        val start = index.coerceIn(0, queue.size - 1)
        val order = if (shuffle) shuffled(queue, if (index < 0) null else start) else queue
        c.shuffleModeEnabled = false
        c.setMediaItems(order.map(::item), if (shuffle) 0 else start, 0)
        c.prepare()
        c.play()
    }

    private fun setShuffle(c: MediaController, on: Boolean) {
        if (on == shuffle) return
        shuffle = on
        val now = c.currentMediaItemIndex
        val count = c.mediaItemCount
        if (now < 0 || count == 0) return
        val current = idOf(c.currentMediaItem)
        if (on) {
            // What comes after the song playing is mixed.
            val upcoming = (now + 1 until count).map { idOf(c.getMediaItemAt(it)) }
            if (upcoming.isNotEmpty()) {
                c.removeMediaItems(now + 1, count)
                c.addMediaItems(upcoming.shuffled().map(::item))
            }
        } else {
            // The order it was given, from where the song playing stands in it.
            val at = original.indexOf(current)
            if (at < 0) return
            if (now + 1 < count) c.removeMediaItems(now + 1, count)
            if (now > 0) c.removeMediaItems(0, now)
            c.addMediaItems(original.drop(at + 1).map(::item))
            c.addMediaItems(0, original.take(at).map(::item))
        }
    }

    private fun enqueue(c: MediaController, add: List<Long>, next: Boolean) {
        if (add.isEmpty()) return
        if (c.mediaItemCount == 0) {
            load(c, add, 0, false)
            return
        }
        val at = if (next) c.currentMediaItemIndex + 1 else c.mediaItemCount
        c.addMediaItems(at, add.map(::item))
        original.addAll(add)
    }

    private fun keep(ids: List<Long>) {
        work.execute {
            try {
                val songs = ids.chunked(100).flatMap { chunk ->
                    val items = brain.call("songs", "ids" to JSONArray(chunk)).getJSONArray("items")
                    (0 until items.length()).map { i ->
                        val o = items.getJSONObject(i)
                        Song(o.getString("id").substringAfterLast(':').toLong(), o.getString("title"), o.optString("artist").split(", ").filter { a -> a.isNotEmpty() }, o.optString("album"), 0, (o.optLong("duration_ms") / 1000).toInt())
                    }
                }
                offline.keep(songs)
            } catch (e: java.io.IOException) {
                main.post { emit("event", JSONObject().put("ev", "error").put("message", "Cannot keep songs: ${e.message}")) }
            }
        }
    }

    private fun drop(ids: List<Long>) {
        work.execute { offline.remove(ids.map { Song(it, "", emptyList(), "", 0, 0) }) }
    }

    // ── Events ───────────────────────────────────────────────────────────────

    private fun cache() {
        emit("event", JSONObject().put("ev", "cache").put("used", offline.playedBytes()).put("limit", settings.cacheGiB * GIB))
    }

    private fun ask() {
        cache()
        controller?.let {
            state(it)
            queue(it)
        }
        recent()
        kept()
    }

    private fun state(c: Player) {
        val at = c.currentMediaItemIndex
        val duration = if (c.duration > 0) c.duration / 1000.0 else 0.0
        emit(
            "event",
            JSONObject()
                .put("ev", "state")
                .put("playing", c.isPlaying)
                .put("id", if (at < 0) 0 else idOf(c.currentMediaItem))
                .put("index", if (c.mediaItemCount == 0) -1 else at)
                .put("len", c.mediaItemCount)
                .put("pos", c.currentPosition.coerceAtLeast(0) / 1000.0)
                .put("dur", duration)
                .put("shuffle", shuffle)
                .put("repeat", when (c.repeatMode) { Player.REPEAT_MODE_ALL -> "all"; Player.REPEAT_MODE_ONE -> "one"; else -> "off" })
                .put("fade", settings.crossfadeSecs.toDouble())
                .put("vol", c.volume.toDouble())
                .put("loading", c.playbackState == Player.STATE_BUFFERING && c.playWhenReady),
        )
    }

    private fun queue(c: Player) {
        val at = c.currentMediaItemIndex.coerceAtLeast(0)
        val upcoming = (at until minOf(c.mediaItemCount, at + 100)).map { idOf(c.getMediaItemAt(it)) }
        emit("event", JSONObject().put("ev", "queue").put("ids", JSONArray(upcoming)).put("index", if (c.mediaItemCount == 0) -1 else c.currentMediaItemIndex).put("len", c.mediaItemCount))
    }

    /** What was played last, on any of the person's devices. */
    private fun recent() {
        work.execute {
            try {
                val items = brain.call("children", "parent" to "recent").getJSONArray("items")
                val ids = (0 until items.length()).map { items.getJSONObject(it).getString("id").substringAfterLast(':').toLong() }
                main.post { emit("event", JSONObject().put("ev", "recent").put("ids", JSONArray(ids))) }
            } catch (e: java.io.IOException) {
                // Not now: the next song asks again.
            }
        }
    }

    /** The songs on the device, how far the ones on their way are, and the room they take. */
    private fun kept() {
        work.execute {
            try {
                val songs = offline.kept()
                val done = songs.count { offline.state(it.id) == 100 }
                val ids = songs.filter { offline.state(it.id) == 100 }.map { it.id }
                main.post {
                    cache()
                    emit("event", JSONObject().put("ev", "cached").put("ids", JSONArray(ids)))
                    emit("event", JSONObject().put("ev", "saving").put("done", done).put("total", if (done == songs.size) 0 else songs.size))
                    val quota = activity.filesDir.usableSpace + offline.bytes()
                    emit("event", JSONObject().put("ev", "storage").put("used", offline.bytes()).put("quota", quota))
                }
            } catch (e: Exception) {
                // The download index is busy: the next change asks again.
            }
        }
    }

    companion object {
        private const val TICK_MS = 1000L
        private const val GIB = 1024L * 1024 * 1024
    }
}
