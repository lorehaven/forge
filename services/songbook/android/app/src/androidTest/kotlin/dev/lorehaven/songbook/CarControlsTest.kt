package dev.lorehaven.songbook

import android.content.ComponentName
import android.content.Context
import android.os.Bundle
import androidx.media3.common.MediaItem
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaBrowser
import androidx.media3.session.SessionCommand
import androidx.media3.session.SessionToken
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.TimeUnit

/** What the car's screen adds: the crossfade button, "continue where you were", and a recently played that is the same everywhere. */
@RunWith(AndroidJUnit4::class)
class CarControlsTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context: Context = instrumentation.targetContext
    private lateinit var browser: MediaBrowser
    private val session get() = Session.of(context)

    private fun <T> onMain(block: () -> T): T {
        var result: Result<T>? = null
        instrumentation.runOnMainSync { result = runCatching(block) }
        return result!!.getOrThrow()
    }

    @Before
    fun connect() {
        session.signOut()
        session.login("http://10.0.2.2:18080", "", "")
        Settings.of(context).crossfadeSecs = 0
        val token = SessionToken(context, ComponentName(context, PlaybackService::class.java))
        browser = onMain { MediaBrowser.Builder(context, token).buildAsync() }.get(30, TimeUnit.SECONDS)
    }

    @After
    fun disconnect() {
        Settings.of(context).crossfadeSecs = 0
        onMain { browser.pause() }
        onMain { browser.release() }
    }

    private fun children(parent: String): List<MediaItem> =
        onMain { browser.getChildren(parent, 0, 100, null) }.get(30, TimeUnit.SECONDS).value!!

    private fun buttonName(): String? = onMain { browser.mediaButtonPreferences.firstOrNull()?.displayName?.toString() }

    @Test
    fun theCrossfadeButtonOfTheCarsPlayerStepsThroughTheLengths() {
        waitFor("the button") { buttonName() == "Crossfade: off" }
        val fade = SessionCommand(PlaybackService.FADE_COMMAND, Bundle.EMPTY)
        onMain { browser.sendCustomCommand(fade, Bundle.EMPTY) }.get(10, TimeUnit.SECONDS)
        waitFor("3 s") { buttonName() == "Crossfade: 3 s" }
        assertEquals(3, Settings.of(context).crossfadeSecs)
        onMain { browser.sendCustomCommand(fade, Bundle.EMPTY) }.get(10, TimeUnit.SECONDS)
        waitFor("6 s") { buttonName() == "Crossfade: 6 s" }
        // Changed on the phone, the car's button follows.
        Settings.of(context).crossfadeSecs = 11
        waitFor("11 s") { buttonName() == "Crossfade: 11 s" }
        // ... and the press after the last length goes back to off.
        Settings.of(context).crossfadeSecs = 12
        waitFor("12 s") { buttonName() == "Crossfade: 12 s" }
        onMain { browser.sendCustomCommand(fade, Bundle.EMPTY) }.get(10, TimeUnit.SECONDS)
        waitFor("off") { buttonName() == "Crossfade: off" }
    }

    @Test
    fun whatIsPlayedGoesToTheServiceAndComesBackAsRecentlyPlayed() {
        val songs = children("popular")
        val chosen = songs[3]
        onMain { browser.setMediaItem(chosen); browser.prepare(); browser.play() }
        waitFor("playing") { onMain { browser.isPlaying && browser.currentPosition > 500 } }
        val id = Tree.songOf(chosen)!!.id
        // Told to the service (the same list on the other devices of this person) ...
        waitFor("the service has it (wanted $id; has ${Lib.recent(context).take(5)})") { Lib.recent(context).take(5).contains(id) }
        // ... and the car's "Recently played" has it near the top.
        val recent = children("recent").take(5).map { it.mediaMetadata.title.toString() }
        assertTrue("$recent", chosen.mediaMetadata.title.toString() in recent)
    }

    @Test
    fun theCarContinuesWhereTheMusicWasLeft() {
        val songs = children("popular")
        onMain { browser.setMediaItem(songs[2]); browser.prepare(); browser.play() }
        waitFor("playing") { onMain { browser.isPlaying && browser.currentPosition > 500 } }
        onMain { browser.seekTo(20_000) }
        waitFor("at 20 s") { onMain { browser.currentPosition >= 20_000 } }
        onMain { browser.pause() }
        waitFor("paused and saved") { Resume.load(Brain.of(context))?.let { it.mediaId == songs[2].mediaId && it.positionMs >= 19_000 } == true }

        // The queue as it was: the same list, the same song, the same place.
        val saved = Resume.load(Brain.of(context))!!
        val tree = Tree(session, Brain.of(context), Offline.of(context))
        val resumed = Resume.queue(tree, saved)
        assertNotNull(resumed)
        assertTrue("the whole list", resumed!!.mediaItems.size >= songs.size)
        assertEquals(2, resumed.startIndex)
        assertEquals(saved.positionMs, resumed.startPositionMs)

        // What Android Auto asks for to offer "continue": a root of what was playing.
        val root = onMain { browser.getLibraryRoot(androidx.media3.session.MediaLibraryService.LibraryParams.Builder().setRecent(true).build()) }.get(30, TimeUnit.SECONDS)
        assertEquals(LibraryResult.RESULT_SUCCESS, root.resultCode)
        assertEquals(Tree.RECENT_ROOT, root.value!!.mediaId)
        val last = children(Tree.RECENT_ROOT)
        assertEquals(songs[2].mediaMetadata.title.toString(), last.single().mediaMetadata.title.toString())
        val suggested = onMain { browser.getLibraryRoot(androidx.media3.session.MediaLibraryService.LibraryParams.Builder().setSuggested(true).build()) }.get(30, TimeUnit.SECONDS)
        assertEquals(Tree.SUGGESTED_ROOT, suggested.value!!.mediaId)
        assertTrue(children(Tree.SUGGESTED_ROOT).isNotEmpty())
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
