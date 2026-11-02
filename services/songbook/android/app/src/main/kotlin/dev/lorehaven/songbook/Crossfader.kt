package dev.lorehaven.songbook

import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.media3.common.C
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin

/**
 * Fades one song into the next. ExoPlayer plays a queue one song at a time, so the overlap is made with a second player:
 *
 * * a few seconds before the end of the song, the second player is given the same song, at the same place;
 * * when the fade starts, it takes over the end of that song (and fades out) while the main player, which keeps the queue
 *   and everything the screens show, moves on to the next song and fades in.
 *
 * Only a song that ends by itself fades: skipping with a button or choosing another song cuts at once, as it should. Equal
 * power curves, so the sum sounds as loud as one song. Runs on the main thread, where the players live.
 */
class Crossfader(
    private val player: ExoPlayer,
    private val settings: Settings,
    /** Makes the second player: the same sources, but it takes no audio focus (the main player holds it). */
    private val makeSecondary: () -> ExoPlayer,
) {
    private val handler = Handler(Looper.getMainLooper())
    private var secondary: ExoPlayer? = null
    private var preparedFor: String? = null
    private var fading = false
    private var startedAt = 0L
    private var lengthMs = 0L
    private var expectedItem: String? = null

    private val tick = object : Runnable {
        override fun run() {
            step()
            handler.postDelayed(this, if (fading) RAMP_MS else WATCH_MS)
        }
    }

    fun start() {
        handler.post(tick)
    }

    fun release() {
        handler.removeCallbacks(tick)
        stopSecondary()
        player.volume = 1f
    }

    private fun step() {
        if (fading) return ramp()
        val secs = settings.crossfadeSecs
        if (secs <= 0 || !player.isPlaying || player.repeatMode == Player.REPEAT_MODE_ONE || !player.hasNextMediaItem()) {
            if (secondary != null && !fading) stopSecondary()
            return
        }
        val duration = player.duration
        // A song shorter than a few fades is not faded out of: there would be nothing of it left.
        if (duration == C.TIME_UNSET || duration < secs * 3_000L) return
        val remaining = duration - player.currentPosition
        val item = player.currentMediaItem?.mediaId ?: return
        if (remaining <= secs * 1_000L + PREPARE_MS && preparedFor != item) prepare(item, duration - secs * 1_000L)
        if (remaining <= secs * 1_000L && preparedFor == item && secondary?.playbackState == Player.STATE_READY) begin(secs * 1_000L, item)
    }

    /** Gets the second player ready with the song that is about to end, at about the place where the fade will start. */
    private fun prepare(item: String, at: Long) {
        stopSecondary()
        val current = player.currentMediaItem ?: return
        secondary = makeSecondary().apply {
            volume = 0f
            setMediaItem(current)
            prepare()
            seekTo(max(0L, at))
            playWhenReady = false
        }
        preparedFor = item
        publish()
    }

    private fun begin(length: Long, item: String) {
        val second = secondary ?: return
        // It takes over the song where the main player is, and the main player goes on to the next one, silent at first.
        second.seekTo(player.currentPosition)
        second.volume = 1f
        second.playWhenReady = true
        player.volume = 0f
        lengthMs = length
        startedAt = SystemClock.elapsedRealtime()
        fading = true
        player.seekToNext()
        expectedItem = player.currentMediaItem?.mediaId
        publish()
    }

    private fun ramp() {
        val second = secondary
        // Someone chose another song, or stopped: the fade is over, cut cleanly.
        if (second == null || player.currentMediaItem?.mediaId != expectedItem) return finish()
        if (!player.playWhenReady) {
            second.playWhenReady = false
            return
        }
        second.playWhenReady = true
        val t = ((SystemClock.elapsedRealtime() - startedAt).toDouble() / lengthMs).coerceIn(0.0, 1.0)
        player.volume = sin(t * PI / 2).toFloat()
        second.volume = cos(t * PI / 2).toFloat()
        publish()
        if (t >= 1.0) finish()
    }

    private fun finish() {
        fading = false
        player.volume = 1f
        stopSecondary()
    }

    private fun stopSecondary() {
        secondary?.release()
        secondary = null
        preparedFor = null
        publish()
    }

    private fun publish() {
        state = State(fading, secondary != null, player.volume, secondary?.volume ?: 0f)
    }

    /** What is happening, for the tests. */
    data class State(val fading: Boolean = false, val secondaryReady: Boolean = false, val mainVolume: Float = 1f, val secondaryVolume: Float = 0f)

    companion object {
        @Volatile var state = State()

        private const val WATCH_MS = 250L
        private const val RAMP_MS = 40L

        /** How long before the fade starts the second player begins to load. */
        private const val PREPARE_MS = 4_000L
    }
}
