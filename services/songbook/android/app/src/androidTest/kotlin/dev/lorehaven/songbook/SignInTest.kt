package dev.lorehaven.songbook

import android.content.ComponentName
import android.content.Context
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaBrowser
import androidx.media3.session.SessionToken
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.TimeUnit

/**
 * The real sign-in path against a stand-in Gatehouse (`http://10.0.2.2:18090`, user `ann`, password `secret`) that hands out
 * access tokens that last six seconds and lets the Songbook API through only with one: so a song that is skipped to after
 * that must renew the token by itself, and a revoked session must end.
 */
@RunWith(AndroidJUnit4::class)
class SignInTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context: Context = instrumentation.targetContext
    private val server = "http://10.0.2.2:18090"
    private var browser: MediaBrowser? = null

    private fun <T> onMain(block: () -> T): T {
        var result: Result<T>? = null
        instrumentation.runOnMainSync { result = runCatching(block) }
        return result!!.getOrThrow()
    }

    private fun stats(): JSONObject = JSONObject(Http.call("GET", "$server/__stats").body)

    @Before
    fun fresh() {
        Session.of(context).signOut()
    }

    @After
    fun done() {
        browser?.let { b -> onMain { b.release() } }
        Session.of(context).signOut()
    }

    @Test
    fun aWrongPasswordIsRefusedInWords() {
        try {
            Session.of(context).login(server, "ann", "nope")
            fail("signed in with a wrong password")
        } catch (e: IllegalArgumentException) {
            assertEquals("The user name or password is wrong.", e.message)
        }
        assertFalse(Session.of(context).signedIn)
    }

    @Test
    fun aSignedInPersonBrowsesPlaysAndTheTokenRenewsItself() {
        val session = Session.of(context)
        session.login(server, "ann", "secret")
        assertTrue(session.signedIn)
        assertEquals("ann", session.user)
        val before = stats()

        val token = SessionToken(context, ComponentName(context, PlaybackService::class.java))
        val b = onMain { MediaBrowser.Builder(context, token).buildAsync() }.get(30, TimeUnit.SECONDS)
        browser = b
        val songs = onMain { b.getChildren("popular", 0, 50, null) }.get(30, TimeUnit.SECONDS).value!!
        assertTrue(songs.size >= 3)
        onMain { b.setMediaItem(songs[0]); b.prepare(); b.play() }
        waitFor("playing") { onMain { b.isPlaying && b.currentPosition > 1_000 } }

        // The first token has run out by now (six seconds): the next song needs a new one, and gets it.
        Thread.sleep(7_000)
        onMain { b.seekToNext() }
        waitFor("the next song plays") { onMain { b.currentMediaItemIndex == 1 && b.isPlaying && b.currentPosition > 1_000 } }

        val after = stats()
        assertTrue("the token was renewed: ${before} -> ${after}", after.getInt("refreshes") > before.getInt("refreshes"))
        assertEquals("no song was refused for want of a token", 0, after.getInt("stream_rejected"))
        assertTrue("songs were read with the token", after.getInt("stream_ok") >= 2)
        assertEquals("nor was anything else", 0, after.getInt("api_rejected"))
    }

    @Test
    fun aRevokedSessionEndsAndTheCarIsToldToSignInAgain() {
        val session = Session.of(context)
        session.login(server, "ann", "secret")
        val token = SessionToken(context, ComponentName(context, PlaybackService::class.java))
        val b = onMain { MediaBrowser.Builder(context, token).buildAsync() }.get(30, TimeUnit.SECONDS)
        browser = b
        assertEquals(LibraryResult.RESULT_SUCCESS, onMain { b.getChildren("popular", 0, 5, null) }.get(30, TimeUnit.SECONDS).resultCode)

        Http.call("POST", "$server/__revoke", "{}")
        Thread.sleep(6_500)
        session.expire()
        // (Playlists are asked for every time; the mixes of the home page are kept for two minutes.)
        val answer = onMain { b.getChildren("playlists", 0, 5, null) }.get(30, TimeUnit.SECONDS)
        assertEquals(LibraryResult.RESULT_ERROR_SESSION_AUTHENTICATION_EXPIRED, answer.resultCode)
        assertFalse("the session ended", session.signedIn)
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
