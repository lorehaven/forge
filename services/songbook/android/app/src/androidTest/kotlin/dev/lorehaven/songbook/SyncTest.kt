package dev.lorehaven.songbook

import android.content.ComponentName
import android.content.Context
import androidx.media3.session.MediaBrowser
import androidx.media3.session.SessionToken
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.TimeUnit

/** Where the person stopped is the same on every device: another device's place is taken (paused), and ours is written. */
@RunWith(AndroidJUnit4::class)
class SyncTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context: Context = instrumentation.targetContext
    private lateinit var browser: MediaBrowser
    private val brain get() = Brain.of(context)

    private fun <T> onMain(block: () -> T): T {
        var result: Result<T>? = null
        instrumentation.runOnMainSync { result = runCatching(block) }
        return result!!.getOrThrow()
    }

    private fun waitFor(what: String, seconds: Long = 40, condition: () -> Boolean) {
        val end = System.currentTimeMillis() + seconds * 1000
        while (System.currentTimeMillis() < end) {
            if (condition()) return
            Thread.sleep(200)
        }
        throw AssertionError("timed out: $what")
    }

    private val songs get() = Lib.popular(context).take(4).map { it.id }

    @Before
    fun setUp() {
        Session.of(context).signOut()
        Session.of(context).login("http://10.0.2.2:18080", "", "")
        Settings.of(context).crossfadeSecs = 0
        // Another device left its place (this one has played nothing yet).
        brain.call(
            "resume_put",
            "song_id" to songs[2], "position_ms" to 42_000L, "queue" to JSONArray(songs), "index" to 2, "device" to "another-device",
        )
        val token = SessionToken(context, ComponentName(context, PlaybackService::class.java))
        browser = onMain { MediaBrowser.Builder(context, token).buildAsync() }.get(30, TimeUnit.SECONDS)
    }

    @After
    fun tearDown() {
        onMain { browser.pause() }
        onMain { browser.release() }
    }

    @Test
    fun anotherDevicesPlaceIsTakenPausedAndOursIsWrittenOnceWePlay() {
        val ids = songs
        waitFor("the other device's list is in place, paused", 60) {
            onMain { browser.mediaItemCount == ids.size && browser.currentMediaItemIndex == 2 && !browser.isPlaying }
        }
        assertTrue("at its point: ${onMain { browser.currentPosition }}", onMain { browser.currentPosition } in 41_000..60_000)

        // Playing goes on from there, and what is written is this device's now.
        onMain { browser.play() }
        waitFor("playing") { onMain { browser.isPlaying && browser.currentPosition > 43_000 } }
        onMain { browser.pause() }
        waitFor("this device's place is the service's", 30) {
            val kept = brain.call("resume_get").optJSONObject("resume")
            kept != null && kept.optString("device").startsWith("android-") && kept.optLong("song_id") == ids[2] && kept.optLong("position_ms") > 43_000
        }
        val kept = brain.call("resume_get").getJSONObject("resume")
        assertEquals(ids, (0 until kept.getJSONArray("queue").length()).map { kept.getJSONArray("queue").getLong(it) })
    }
}
