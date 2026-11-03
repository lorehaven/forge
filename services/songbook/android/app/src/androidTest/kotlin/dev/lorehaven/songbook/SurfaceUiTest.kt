package dev.lorehaven.songbook

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import org.junit.After
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.Callable
import java.util.concurrent.FutureTask
import java.util.concurrent.TimeUnit

/**
 * The phone's screens (drawn by the Rust program, read here through their accessibility tree) driven the way a person does:
 * what is on the home page, play it, the bar and its button, the other pages, Back.
 */
@RunWith(AndroidJUnit4::class)
class SurfaceUiTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context: Context = instrumentation.targetContext
    private val device = UiDevice.getInstance(instrumentation)
    private var controller: MediaController? = null

    private fun <T> onMain(block: () -> T): T {
        val task = FutureTask(Callable { block() })
        Handler(Looper.getMainLooper()).post(task)
        return task.get(30, TimeUnit.SECONDS)
    }

    private fun text(value: String, seconds: Long = 20) = device.wait(Until.findObject(By.text(value)), seconds * 1000)

    /** A tab of the bar at the bottom: the screen is a picture, so the place is worked out from its size (five equal tabs). */
    private fun tab(n: Int) {
        device.click(device.displayWidth * (2 * n + 1) / 10, device.displayHeight - 130)
    }

    /** Touches the element with this text; the screen is redrawn often, so a stale one is looked up again. */
    private fun tap(value: String) {
        repeat(5) {
            try {
                val bounds = text(value)!!.visibleBounds
                device.click(bounds.centerX(), bounds.centerY())
                return
            } catch (e: androidx.test.uiautomator.StaleObjectException) {
                Thread.sleep(300)
            }
        }
        throw AssertionError("could not touch $value")
    }

    @Before
    fun open() {
        instrumentation.uiAutomation.grantRuntimePermission(context.packageName, "android.permission.POST_NOTIFICATIONS")
        Offline.of(context).removeAll()
        Settings.of(context).crossfadeSecs = 0
        // A development server that asks for no sign-in: the person is in, with no password.
        Session.of(context).signOut()
        Session.of(context).login("http://10.0.2.2:18080", "", "")
        val intent = context.packageManager.getLaunchIntentForPackage(context.packageName)!!.apply {
            addFlags(Intent.FLAG_ACTIVITY_CLEAR_TASK or Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        context.startActivity(intent)
        device.wait(Until.hasObject(By.pkg(context.packageName).depth(0)), 15_000)
    }

    @After
    fun close() {
        controller?.let { c -> onMain { c.pause(); c.release() } }
        Offline.of(context).removeAll()
    }

    private fun session(): MediaController {
        val token = SessionToken(context, ComponentName(context, PlaybackService::class.java))
        return onMain { MediaController.Builder(context, token).buildAsync() }.get(30, TimeUnit.SECONDS).also { controller = it }
    }

    private fun waitFor(what: String, seconds: Long = 30, condition: () -> Boolean) {
        val end = System.currentTimeMillis() + seconds * 1000
        while (System.currentTimeMillis() < end) {
            if (condition()) return
            Thread.sleep(250)
        }
        throw AssertionError("timed out: $what")
    }

    @Test
    fun theHomePagePlaysAndTheBarShowsAndControlsIt() {
        assertNotNull("the home page", text("Play everything"))
        assertNotNull(text("Shuffle everything"))
        val c = session()
        tap("Play everything")
        waitFor("playing") { onMain { c.isPlaying && c.currentPosition > 500 } }
        val title = onMain { c.currentMediaItem?.mediaMetadata?.title?.toString() }
        assertTrue("the session knows the song's title: $title", !title.isNullOrEmpty())
        // The bar names the song that plays (asked again each time: the song may have changed meanwhile).
        waitFor("the bar shows what plays ($title)", 40) {
            val now = onMain { c.currentMediaItem?.mediaMetadata?.title?.toString() }
            now != null && device.hasObject(By.text(now))
        }
        // The bar's button pauses and plays it.
        tap("Play or pause")
        waitFor("paused") { onMain { !c.isPlaying } }
        tap("Play or pause")
        waitFor("playing again") { onMain { c.isPlaying } }
        // What the engine plays is the session's: a queue with more than the one song.
        assertTrue("a queue", onMain { c.mediaItemCount } > 1)
    }

    @Test
    fun theOtherPagesAreReachedFromTheTabsAndBackGoesBackThenLeaves() {
        assertNotNull(text("Play everything"))
        tab(1)
        assertNotNull("the search page", device.wait(Until.findObject(By.clazz("android.widget.EditText")), 20_000))
        // (The first Back may only put the keyboard away.)
        device.pressBack()
        if (device.wait(Until.findObject(By.text("Play everything")), 3_000) == null) device.pressBack()
        assertNotNull("back to the home page", text("Play everything"))
        tab(2)
        assertNotNull("playlists", device.wait(Until.findObject(By.textContains("playlist")), 20_000))
        tab(3)
        assertNotNull("saved songs", device.wait(Until.findObject(By.textContains("device")), 20_000))
        // With nothing left to go back to, Back leaves the app.
        repeat(4) { device.pressBack() }
        assertTrue("the app is gone", device.wait(Until.gone(By.pkg(context.packageName).depth(0)), 10_000))
    }
}
