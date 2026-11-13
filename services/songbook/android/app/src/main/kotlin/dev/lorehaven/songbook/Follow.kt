package dev.lorehaven.songbook

import android.content.Context
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.Handler
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import org.json.JSONArray
import java.util.UUID
import java.util.concurrent.Executor

/**
 * Where the person stopped, on every device they use (the web player does the same: `engine.js`). The service keeps one place
 * per person: the song, the exact point in it and the list. This device writes it while it plays (and when it pauses or moves on
 * to another song), and takes a newer one written by another device, paused at that point, so that "play" goes on from there.
 * Playing here, this device is the one listened to: what others write waits.
 */
class Sync(
    private val context: Context,
    private val player: Player,
    private val brain: Brain,
    private val tree: Tree,
    private val work: Executor,
    private val main: Handler,
) {
    private val me: String = device()

    /** The newest write of the service that this device has taken or made (seconds). */
    private var syncedAt = 0L
    private var lastPush = 0L

    /** Whether the person played anything here since the last time another device's place was taken: only then is there something to write. */
    private var touched = false

    /** The song that was put in place from another device and has not played here yet: putting it there is no play to remember. */
    @Volatile var placedId: String? = null
        private set

    private var stopped = false

    private val tick = object : Runnable {
        override fun run() {
            if (stopped) return
            if (player.isPlaying) push(false) else pull()
            main.postDelayed(this, EVERY_MS)
        }
    }

    private val listener = object : Player.Listener {
        override fun onIsPlayingChanged(isPlaying: Boolean) {
            if (isPlaying) {
                touched = true
                placedId = null
            } else {
                push(true)
            }
        }

        override fun onMediaItemTransition(mediaItem: MediaItem?, reason: Int) {
            if (reason != Player.MEDIA_ITEM_TRANSITION_REASON_PLAYLIST_CHANGED) {
                placedId = null
                if (touched) push(true)
            }
        }

        override fun onPositionDiscontinuity(oldPosition: Player.PositionInfo, newPosition: Player.PositionInfo, reason: Int) {
            if (reason == Player.DISCONTINUITY_REASON_SEEK && touched) push(false)
        }
    }

    fun start() {
        player.addListener(listener)
        main.postDelayed(tick, FIRST_MS)
    }

    fun stop() {
        stopped = true
        main.removeCallbacks(tick)
        player.removeListener(listener)
        if (touched) push(true)
    }

    private fun idOf(item: MediaItem): Long = item.mediaId.substringAfterLast(':').toLongOrNull() ?: 0L

    /** Writes the place (main thread). `force`: at once, not at most every ten seconds. */
    private fun push(force: Boolean) {
        val at = player.currentMediaItemIndex
        if (!touched || at < 0 || player.mediaItemCount == 0) return
        val now = System.currentTimeMillis()
        if (!force && now - lastPush < PUSH_EVERY_MS) return
        val song = player.currentMediaItem?.let(::idOf) ?: return
        if (song <= 0) return
        lastPush = now
        // At most a window of the list, with the song's place in it.
        val from = (at - BEHIND).coerceAtLeast(0)
        val to = minOf(player.mediaItemCount, from + MOST)
        val queue = JSONArray((from until to).map { idOf(player.getMediaItemAt(it)) })
        val position = player.currentPosition.coerceAtLeast(0)
        work.execute {
            try {
                val kept = brain.call(
                    "resume_put",
                    "song_id" to song, "position_ms" to position, "queue" to queue, "index" to at - from, "device" to me,
                )
                syncedAt = maxOf(syncedAt, kept.optLong("at"))
            } catch (e: SignedOut) {
                SignedOutNotice.show(context)
            } catch (e: java.io.IOException) {
                // Out of reach: the next write tells it.
            }
        }
    }

    /** Takes a newer place written by another device, if this one is not playing (the first call is the one that matters on opening the app). */
    fun pull() {
        work.execute {
            try {
                val found = brain.call("resume_get").optJSONObject("resume") ?: return@execute
                val at = found.optLong("at")
                if (found.optString("device") == me || at <= syncedAt) {
                    syncedAt = maxOf(syncedAt, at)
                    return@execute
                }
                val ids = found.optJSONArray("queue") ?: return@execute
                val queue = (0 until ids.length()).map { ids.getLong(it) }
                val index = found.optInt("index")
                val position = found.optLong("position_ms")
                main.post { place(queue, index, position, at) }
            } catch (e: SignedOut) {
                // Said by whatever asks for the library; nothing to take.
            } catch (e: java.io.IOException) {
                // Out of reach: what is here stays.
            }
        }
    }

    /** Puts the other device's list in the player, paused at its point (main thread). */
    private fun place(queue: List<Long>, index: Int, position: Long, at: Long) {
        if (player.isPlaying || stopped || index !in queue.indices) return
        syncedAt = at
        touched = false
        val items = queue.map(tree::bareItem)
        placedId = items[index].mediaId
        player.setMediaItems(items, index, position)
        player.prepare()
    }

    private fun device(): String {
        val prefs = context.getSharedPreferences("sync", Context.MODE_PRIVATE)
        return prefs.getString("device", null) ?: ("android-" + UUID.randomUUID().toString().take(8)).also { prefs.edit().putString("device", it).apply() }
    }

    private companion object {
        const val FIRST_MS = 1_500L
        const val EVERY_MS = 10_000L
        const val PUSH_EVERY_MS = 10_000L
        const val BEHIND = 100
        const val MOST = 2000
    }
}

/**
 * Plays again when what took the sound away is over: a game's advertisement, a video that played for a moment. A short loss of
 * the sound (a navigation voice) the player gets over by itself; a long one (another app *took* the sound) it does not, and
 * nothing says when the other app is done. This looks at what else plays: when nothing does, soon after a loss that was not
 * long ago, it plays. A person's own pause, and a loss that went on for minutes (they went to another player), are left alone.
 */
class FocusResume(context: Context, private val player: Player, private val main: Handler) {
    private val audio = context.getSystemService(AudioManager::class.java)
    private var lostAt = 0L
    private var stopped = false

    private val resume = Runnable {
        if (stopped || lostAt == 0L) return@Runnable
        lostAt = 0
        if (!player.playWhenReady && player.mediaItemCount > 0) player.play()
    }

    private val others = object : AudioManager.AudioPlaybackCallback() {
        override fun onPlaybackConfigChanged(configs: MutableList<android.media.AudioPlaybackConfiguration>) {
            main.removeCallbacks(resume)
            val wanted = lostAt != 0L && System.currentTimeMillis() - lostAt <= LONGEST_MS
            if (wanted && configs.isEmpty()) main.postDelayed(resume, QUIET_MS)
        }
    }

    private var holdUntil = 0L

    /** Does not play again for a while: the person's speaker or car went away, and the phone's own speaker must stay quiet. */
    fun hold() {
        holdUntil = System.currentTimeMillis() + HOLD_MS
        lostAt = 0
        main.removeCallbacks(resume)
    }

    private val listener = object : Player.Listener {
        override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
            if (System.currentTimeMillis() < holdUntil) return
            lostAt = if (!playWhenReady && reason == Player.PLAY_WHEN_READY_CHANGE_REASON_AUDIO_FOCUS_LOSS) System.currentTimeMillis() else 0L
            if (lostAt == 0L) main.removeCallbacks(resume)
        }
    }

    fun start() {
        player.addListener(listener)
        audio.registerAudioPlaybackCallback(others, main)
    }

    fun stop() {
        stopped = true
        main.removeCallbacks(resume)
        player.removeListener(listener)
        audio.unregisterAudioPlaybackCallback(others)
    }

    private companion object {
        /** How long the other sound may have lasted for this to still be a game's advertisement and not another player. */
        const val LONGEST_MS = 5 * 60_000L

        /** How long nothing else plays before this does (advertisements have gaps). */
        const val QUIET_MS = 1_500L

        /** How long after a disconnect a loss of the sound is not answered with playing. */
        const val HOLD_MS = 15_000L
    }
}

/**
 * Stops the music when what it was heard through goes away: a Bluetooth speaker or headset, a wired or USB output, a car. The
 * player's own "becoming noisy" handling covers only some of these (it is a broadcast the system sends for some disconnects),
 * so this watches the outputs themselves. Android Auto leaving is seen by the service (its controller disconnects).
 */
class OutputWatch(context: Context, private val player: Player, private val main: Handler, private val focus: FocusResume) {
    private val audio = context.getSystemService(AudioManager::class.java)

    private val callback = object : AudioDeviceCallback() {
        override fun onAudioDevicesRemoved(removedDevices: Array<out AudioDeviceInfo>) {
            if (removedDevices.any { it.isSink && it.type in EXTERNAL }) stopMusic("an output went away")
        }
    }

    /** Pauses what plays, and keeps it from starting again by itself. */
    fun stopMusic(why: String) {
        if (!player.playWhenReady) return
        android.util.Log.i("SongbookOutput", "$why: pausing")
        focus.hold()
        player.pause()
    }

    fun start() = audio.registerAudioDeviceCallback(callback, main)
    fun stop() = audio.unregisterAudioDeviceCallback(callback)

    private companion object {
        val EXTERNAL = setOf(
            AudioDeviceInfo.TYPE_BLUETOOTH_A2DP, AudioDeviceInfo.TYPE_BLUETOOTH_SCO, AudioDeviceInfo.TYPE_BLE_HEADSET,
            AudioDeviceInfo.TYPE_BLE_SPEAKER, AudioDeviceInfo.TYPE_HEARING_AID, AudioDeviceInfo.TYPE_WIRED_HEADPHONES,
            AudioDeviceInfo.TYPE_WIRED_HEADSET, AudioDeviceInfo.TYPE_USB_DEVICE, AudioDeviceInfo.TYPE_USB_HEADSET,
            AudioDeviceInfo.TYPE_USB_ACCESSORY, AudioDeviceInfo.TYPE_LINE_ANALOG, AudioDeviceInfo.TYPE_LINE_DIGITAL,
            AudioDeviceInfo.TYPE_HDMI, AudioDeviceInfo.TYPE_AUX_LINE, AudioDeviceInfo.TYPE_DOCK,
        )
    }
}
