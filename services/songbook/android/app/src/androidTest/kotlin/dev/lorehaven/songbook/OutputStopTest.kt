package dev.lorehaven.songbook

import android.media.AudioDeviceInfo
import android.os.Handler
import android.os.Looper
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/** When the output the music is heard through goes away (car, Bluetooth, wired), the music stops and stays stopped. */
@RunWith(AndroidJUnit4::class)
class OutputStopTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext
    private lateinit var player: ExoPlayer
    private lateinit var focus: FocusResume
    private lateinit var watch: OutputWatch

    private fun <T> onMain(block: () -> T): T {
        var r: Result<T>? = null
        instrumentation.runOnMainSync { r = runCatching(block) }
        return r!!.getOrThrow()
    }

    /** Ten seconds of silence as a WAV file. */
    private fun silence(): File {
        val rate = 8000
        val data = ByteArray(rate * 2 * 10)
        val f = File(context.cacheDir, "silence.wav")
        val h = java.nio.ByteBuffer.allocate(44).order(java.nio.ByteOrder.LITTLE_ENDIAN)
        h.put("RIFF".toByteArray()).putInt(36 + data.size).put("WAVEfmt ".toByteArray()).putInt(16).putShort(1).putShort(1)
            .putInt(rate).putInt(rate * 2).putShort(2).putShort(16).put("data".toByteArray()).putInt(data.size)
        f.writeBytes(h.array() + data)
        return f
    }

    @Before
    fun start() {
        onMain {
            player = ExoPlayer.Builder(context).build()
            val main = Handler(Looper.getMainLooper())
            focus = FocusResume(context, player, main).also { it.start() }
            watch = OutputWatch(context, player, main, focus)
            player.setMediaItem(MediaItem.fromUri(android.net.Uri.fromFile(silence())))
            player.prepare()
            player.play()
        }
        waitFor("playing") { onMain { player.isPlaying } }
    }

    @After
    fun end() = onMain { focus.stop(); player.release() }

    @Test
    fun aCarOrBluetoothOrWiredOutputGoingAwayStopsTheMusic() {
        for (type in listOf(AudioDeviceInfo.TYPE_BLUETOOTH_A2DP, AudioDeviceInfo.TYPE_USB_ACCESSORY, AudioDeviceInfo.TYPE_WIRED_HEADSET, AudioDeviceInfo.TYPE_BLE_SPEAKER)) {
            onMain { player.play() }
            waitFor("playing again") { onMain { player.isPlaying } }
            onMain { watch.removed(listOf(type)) }
            assertFalse("type $type", onMain { player.playWhenReady })
            // and it does not start again by itself
            Thread.sleep(2000)
            assertFalse("type $type stayed stopped", onMain { player.playWhenReady })
        }
    }

    @Test
    fun thePhonesOwnSpeakerGoingAwayDoesNothing() {
        onMain { watch.removed(listOf(AudioDeviceInfo.TYPE_BUILTIN_SPEAKER)) }
        assertTrue(onMain { player.playWhenReady })
    }

    @Test
    fun androidAutoLeavingStopsTheMusic() {
        onMain { watch.stopMusic("Android Auto disconnected") }
        assertFalse(onMain { player.playWhenReady })
        Thread.sleep(2000)
        assertFalse(onMain { player.playWhenReady })
    }

    private fun waitFor(what: String, seconds: Long = 30, condition: () -> Boolean) {
        val end = System.currentTimeMillis() + seconds * 1000
        while (System.currentTimeMillis() < end) { if (condition()) return; Thread.sleep(200) }
        throw AssertionError("timed out waiting for $what")
    }
}
