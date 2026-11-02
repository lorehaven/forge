package dev.lorehaven.songbook

import android.content.ComponentName
import android.content.Context
import androidx.media3.session.MediaBrowser
import androidx.media3.session.SessionToken
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.TimeUnit
import kotlin.math.abs

/** A song fades into the next: both are heard for a while, the sum is as loud as one song, and the fade ends cleanly. */
@RunWith(AndroidJUnit4::class)
class CrossfadeTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context: Context = instrumentation.targetContext
    private lateinit var browser: MediaBrowser

    private fun <T> onMain(block: () -> T): T {
        var result: Result<T>? = null
        instrumentation.runOnMainSync { result = runCatching(block) }
        return result!!.getOrThrow()
    }

    @Before
    fun connect() {
        val session = Session.of(context)
        session.signOut()
        session.login("http://10.0.2.2:18080", "", "")
        val token = SessionToken(context, ComponentName(context, PlaybackService::class.java))
        browser = onMain { MediaBrowser.Builder(context, token).buildAsync() }.get(30, TimeUnit.SECONDS)
    }

    @After
    fun disconnect() {
        Settings.of(context).crossfadeSecs = 0
        onMain { browser.pause() }
        onMain { browser.release() }
    }

    private fun playFirstSong() {
        val songs = onMain { browser.getChildren("popular", 0, 50, null) }.get(30, TimeUnit.SECONDS).value!!
        onMain { browser.shuffleModeEnabled = false; browser.setMediaItem(songs[0]); browser.prepare(); browser.play() }
        waitFor("playing") { onMain { browser.isPlaying && browser.currentPosition > 500 && browser.duration > 30_000 } }
    }

    @Test
    fun aSongFadesIntoTheNext() {
        Settings.of(context).crossfadeSecs = 3
        playFirstSong()
        assertEquals(0, onMain { browser.currentMediaItemIndex })
        val duration = onMain { browser.duration }
        onMain { browser.seekTo(duration - 8_000) }

        waitFor("the fade starts", 30) { Crossfader.state.fading }
        // The queue has moved on at once (the screens show the new song) and the old one is still heard.
        assertEquals(1, onMain { browser.currentMediaItemIndex })
        val samples = mutableListOf<Crossfader.State>()
        val end = System.currentTimeMillis() + 6_000
        while (Crossfader.state.fading && System.currentTimeMillis() < end) {
            samples += Crossfader.state
            Thread.sleep(100)
        }
        assertTrue("sampled the fade: ${samples.size}", samples.size >= 10)
        samples.filter { it.secondaryReady }.forEach {
            val power = it.mainVolume * it.mainVolume + it.secondaryVolume * it.secondaryVolume
            assertTrue("as loud as one song: $it", abs(power - 1f) < 0.1f)
        }
        assertTrue("the new song comes in", samples.last().mainVolume > samples.first().mainVolume)
        assertTrue("both are heard at some moment", samples.any { it.mainVolume > 0.25f && it.secondaryVolume > 0.25f })

        waitFor("the fade ends") { !Crossfader.state.fading }
        assertFalse(Crossfader.state.secondaryReady)
        assertEquals(1f, Crossfader.state.mainVolume, 0.001f)
        waitFor("the new song plays") { onMain { browser.isPlaying && browser.currentMediaItemIndex == 1 && browser.currentPosition > 500 } }
    }

    @Test
    fun withNoCrossfadeTheSongsJustFollowEachOther() {
        Settings.of(context).crossfadeSecs = 0
        playFirstSong()
        val duration = onMain { browser.duration }
        onMain { browser.seekTo(duration - 4_000) }
        var faded = false
        waitFor("the next song", 30) {
            faded = faded || Crossfader.state.fading
            onMain { browser.currentMediaItemIndex } == 1
        }
        assertFalse("nothing faded", faded)
    }

    @Test
    fun skippingWithAButtonCutsAtOnce() {
        Settings.of(context).crossfadeSecs = 5
        playFirstSong()
        onMain { browser.seekToNext() }
        waitFor("the next song") { onMain { browser.currentMediaItemIndex } == 1 }
        Thread.sleep(1_000)
        assertFalse("a skip is not faded", Crossfader.state.fading)
    }

    private fun waitFor(what: String, seconds: Long = 40, condition: () -> Boolean) {
        val end = System.currentTimeMillis() + seconds * 1000
        while (System.currentTimeMillis() < end) {
            if (condition()) return
            Thread.sleep(100)
        }
        throw AssertionError("timed out: $what")
    }
}
