package dev.lorehaven.songbook

import android.content.ComponentName
import android.content.Context
import androidx.media3.session.MediaBrowser
import androidx.media3.session.SessionToken
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.TimeUnit

/**
 * Songs kept on the device play with no server. The server is the stand-in at `10.0.2.2:18090`, which can be told to be down
 * (`/__down`), and counts what it is asked for.
 */
@RunWith(AndroidJUnit4::class)
class KeepTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context: Context = instrumentation.targetContext
    private val server = "http://10.0.2.2:18090"
    private lateinit var browser: MediaBrowser
    private val offline get() = Offline.of(context)

    private fun <T> onMain(block: () -> T): T {
        var result: Result<T>? = null
        instrumentation.runOnMainSync { result = runCatching(block) }
        return result!!.getOrThrow()
    }

    private fun stats() = JSONObject(Http.call("GET", "$server/__stats").body)

    @Before
    fun connect() {
        Http.call("GET", "$server/__up")
        offline.removeAll()
        waitFor("nothing kept") { offline.kept().isEmpty() }
        val session = Session.of(context)
        session.signOut()
        session.login(server, "ann", "secret")
        val token = SessionToken(context, ComponentName(context, PlaybackService::class.java))
        browser = onMain { MediaBrowser.Builder(context, token).buildAsync() }.get(30, TimeUnit.SECONDS)
    }

    @After
    fun disconnect() {
        Http.call("GET", "$server/__up")
        onMain { browser.pause() }
        onMain { browser.release() }
        offline.removeAll()
    }

    @Test
    fun keptSongsAreListedAndPlayWithNoServer() {
        val songs = Library(Session.of(context)).home().popular.take(2)
        assertEquals(2, songs.size)
        offline.keep(songs)
        waitFor("both are on the device", 90) { songs.all { offline.state(it.id) == 100 } }
        assertEquals(songs.map { it.id }.toSet(), offline.kept().map { it.id }.toSet())
        assertTrue("they take room", offline.bytes() > 100_000)

        // The server goes away.
        Http.call("GET", "$server/__down")
        val before = stats()
        val root = onMain { browser.getChildren(Tree.ROOT, 0, 50, null) }.get(30, TimeUnit.SECONDS).value!!
        assertEquals("On this device", root[0].mediaMetadata.title.toString())
        val kept = onMain { browser.getChildren("kept", 0, 50, null) }.get(30, TimeUnit.SECONDS).value!!
        assertEquals(2, kept.size)

        onMain { browser.setMediaItem(kept[1]); browser.prepare(); browser.play() }
        waitFor("it plays from the device") { onMain { browser.isPlaying && browser.currentPosition > 2_000 } }
        assertEquals("the whole list is queued", 2, onMain { browser.mediaItemCount })
        onMain { browser.seekToPrevious(); browser.seekToPreviousMediaItem() }
        waitFor("the other kept song plays") { onMain { browser.isPlaying && browser.currentPosition > 1_000 } }

        // The server answered 503 to everything while this played: what was heard came from the device.
        assertEquals("nothing was read from the server", before.getInt("stream_ok"), stats().getInt("stream_ok"))
    }

    @Test
    fun aKeptSongCanBeRemoved() {
        val songs = Library(Session.of(context)).home().popular.take(1)
        offline.keep(songs)
        waitFor("it is on the device", 90) { offline.state(songs[0].id) == 100 }
        offline.remove(songs)
        waitFor("it is gone") { offline.kept().isEmpty() && !offline.isKept(songs[0].id) }
    }

    private fun waitFor(what: String, seconds: Long = 40, condition: () -> Boolean) {
        val end = System.currentTimeMillis() + seconds * 1000
        while (System.currentTimeMillis() < end) {
            if (condition()) return
            Thread.sleep(200)
        }
        throw AssertionError("timed out: $what")
    }
}
