package dev.lorehaven.songbook

import android.content.ComponentName
import android.content.Context
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaBrowser
import androidx.media3.session.SessionToken
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.TimeUnit

/**
 * Plays the part of the car: connects to the playback service as a media browser, walks the tree, searches, and plays a song.
 * Needs a Songbook service that asks for no sign-in at `http://10.0.2.2:18080` (the emulator's name for this computer).
 */
@RunWith(AndroidJUnit4::class)
class CarBrowseTest {
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
        assertTrue("signed in", session.signedIn)
        val token = SessionToken(context, ComponentName(context, PlaybackService::class.java))
        val future = onMain { MediaBrowser.Builder(context, token).buildAsync() }
        browser = future.get(30, TimeUnit.SECONDS)
    }

    @After
    fun disconnect() {
        onMain { browser.release() }
    }

    private fun children(parent: String): List<MediaItem> {
        val result = onMain { browser.getChildren(parent, 0, 200, null) }.get(30, TimeUnit.SECONDS)
        assertEquals("children of $parent: ${result.resultCode}", LibraryResult.RESULT_SUCCESS, result.resultCode)
        return result.value!!
    }

    private fun titles(items: List<MediaItem>) = items.map { it.mediaMetadata.title.toString() }

    @Test
    fun theRootOffersWhatTheCarShows() {
        val root = onMain { browser.getLibraryRoot(null) }.get(30, TimeUnit.SECONDS)
        assertEquals(LibraryResult.RESULT_SUCCESS, root.resultCode)
        val names = titles(children(Tree.ROOT))
        assertTrue(names.toString(), names.containsAll(listOf("Mixes", "Popular", "Artists", "Categories", "Playlists")))
        assertTrue(children(Tree.ROOT).all { it.mediaMetadata.isBrowsable == true })
    }

    @Test
    fun songsAreListedWithWhatTheCarDraws() {
        val songs = children("popular")
        assertTrue("some songs: ${songs.size}", songs.size >= 5)
        songs.forEach {
            assertTrue(it.mediaMetadata.isPlayable == true)
            assertTrue("a title", it.mediaMetadata.title!!.isNotEmpty())
            assertTrue("an artist", it.mediaMetadata.artist!!.isNotEmpty())
            assertTrue("a picture address", it.mediaMetadata.artworkUri.toString().contains("/art/song/"))
        }
    }

    @Test
    fun mixesCategoriesArtistsAndAlbumsAreFoldersOfSongs() {
        val mixes = children("mixes")
        assertTrue("mixes: ${titles(mixes)}", mixes.size >= 3)
        assertTrue(children(mixes[0].mediaId).size >= 10)

        val categories = children("categories")
        assertTrue(categories.isNotEmpty())
        assertTrue(children(categories[0].mediaId).isNotEmpty())

        val letters = children("artists")
        assertTrue("letters: ${titles(letters)}", letters.size >= 5)
        // An artist who is only credited on others' songs has just "All songs"; one with albums has them too.
        val artists = letters.flatMap { children(it.mediaId) }
        assertTrue(artists.isNotEmpty())
        val withAlbums = artists.map { children(it.mediaId) }.first { it.size > 1 }
        assertEquals("All songs", withAlbums[0].mediaMetadata.title.toString())
        assertTrue(children(withAlbums[0].mediaId).isNotEmpty())
        val album = withAlbums.drop(1).firstOrNull { children(it.mediaId).isNotEmpty() }
        assertTrue("an album with songs", album != null)
    }

    @Test
    fun theLibraryCanBeSearched() {
        val query = "stan"
        onMain { browser.search(query, null) }.get(30, TimeUnit.SECONDS)
        val found = onMain { browser.getSearchResult(query, 0, 50, null) }.get(30, TimeUnit.SECONDS)
        assertEquals(LibraryResult.RESULT_SUCCESS, found.resultCode)
        assertTrue(titles(found.value!!).toString(), titles(found.value!!).any { it.contains("Stan", ignoreCase = true) })
    }

    @Test
    fun choosingASongPlaysItsListFromThere() {
        val songs = children("popular")
        val chosen = songs[2]
        onMain {
            browser.setMediaItem(chosen)
            browser.prepare()
            browser.play()
        }
        // The queue is the whole list, started at the song.
        waitFor("the queue of the list") { onMain { browser.mediaItemCount } >= songs.size }
        assertEquals(chosen.mediaId, onMain { browser.currentMediaItem?.mediaId })
        assertEquals(2, onMain { browser.currentMediaItemIndex })
        // It plays: the position moves.
        waitFor("playing") { onMain { browser.isPlaying && browser.currentPosition > 1_500 } }
        // Next is the next song of the list, and the car's buttons work.
        val before = onMain { browser.currentMediaItem?.mediaId }
        onMain { browser.seekToNext() }
        waitFor("the next song") { onMain { browser.currentMediaItem?.mediaId } != before }
        assertEquals(songs[3].mediaId, onMain { browser.currentMediaItem?.mediaId })
        onMain { browser.pause() }
        waitFor("paused") { !onMain { browser.isPlaying } }
        waitFor("ready") { onMain { browser.playbackState } == Player.STATE_READY }
        assertNotEquals(0, onMain { browser.currentMediaItem?.mediaMetadata?.durationMs ?: 0L })
    }

    @Test
    fun aMixIsPlayedShuffled() {
        val mixes = children("mixes")
        val mix = mixes.first { it.mediaMetadata.subtitle.toString().startsWith("Shuffled") }
        val songs = children(mix.mediaId)
        onMain {
            browser.setMediaItem(songs[0])
            browser.prepare()
            browser.play()
        }
        waitFor("the mix") { onMain { browser.mediaItemCount } >= songs.size }
        waitFor("shuffle on") { onMain { browser.shuffleModeEnabled } }
        onMain { browser.pause() }
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
