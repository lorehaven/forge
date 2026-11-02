package dev.lorehaven.songbook

import android.content.Context
import android.content.Intent
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/** The phone's screens, driven the way a person does: browse, play, the player, keep a song, search, the settings. */
@RunWith(AndroidJUnit4::class)
class PhoneUiTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context: Context = instrumentation.targetContext
    private val device = UiDevice.getInstance(instrumentation)
    private val shots = File(context.getExternalFilesDir(null), "shots").apply { mkdirs() }

    private fun shot(name: String) {
        device.executeShellCommand("screencap -p /sdcard/Download/songbook-$name.png")
    }

    private fun visible() = device.findObjects(By.clazz("android.widget.TextView")).map { it.text ?: "" }.filter { it.isNotEmpty() }

    private fun text(value: String, seconds: Long = 20) = device.wait(Until.findObject(By.text(value)), seconds * 1000)
    private fun contains(value: String, seconds: Long = 20) = device.wait(Until.findObject(By.textContains(value)), seconds * 1000)

    @Before
    fun open() {
        instrumentation.uiAutomation.grantRuntimePermission(context.packageName, "android.permission.POST_NOTIFICATIONS")
        Offline.of(context).removeAll()
        Settings.of(context).crossfadeSecs = 0
        Session.of(context).signOut()
        val intent = context.packageManager.getLaunchIntentForPackage(context.packageName)!!.apply {
            addFlags(Intent.FLAG_ACTIVITY_CLEAR_TASK or Intent.FLAG_ACTIVITY_NEW_TASK)
            putExtra("server", "http://10.0.2.2:18080")
            putExtra("user", "")
            putExtra("password", "")
        }
        context.startActivity(intent)
        device.wait(Until.hasObject(By.pkg(context.packageName).depth(0)), 15_000)
    }

    @After
    fun close() {
        Offline.of(context).removeAll()
        Settings.of(context).crossfadeSecs = 0
    }

    @Test
    fun browsePlayKeepSearchAndSettings() {
        shot("0-start")
        // The library, as the car sees it.
        assertNotNull("the root", text("Mixes"))
        assertNotNull(text("Popular"))
        assertNotNull(text("Artists"))
        shot("1-root")

        // A list of songs: pictures, and what to do with each.
        text("Popular")!!.click()
        assertNotNull("songs", device.wait(Until.findObject(By.text("Keep")), 20_000))
        assertNotNull("the way back", text("‹"))
        shot("2-popular")

        // Choosing a song plays the list from it: the mini player shows what plays.
        val shown = visible()
        val firstSong = shown[shown.indexOfFirst { it.startsWith("Keep all") } + 1]
        device.findObject(By.text(firstSong)).click()
        val end = System.currentTimeMillis() + 20_000
        while (device.findObjects(By.text(firstSong)).size < 2 && System.currentTimeMillis() < end) Thread.sleep(300)
        shot("3-mini-player")
        assertTrue("the mini player for $firstSong; on screen: ${visible()}", device.findObjects(By.text(firstSong)).size >= 2)
        shot("3-mini-player")

        // The player.
        device.findObjects(By.text(firstSong)).last().click()
        assertNotNull("the player", text("Now playing"))
        assertNotNull(text("Shuffle"))
        assertNotNull(text("Keep on this device"))
        shot("4-now-playing")
        // The queue: what is next, and a tap goes there.
        val second = Library(Session.of(context)).home().popular[1].title
        text("Queue")!!.click()
        assertNotNull("the queue lists what is next ($second); on screen: ${visible()}", text(second))
        shot("4q-queue")
        device.findObject(By.text(second)).click()
        text("Close")!!.click()
        assertNotNull("it plays $second now", text(second))
        text("Keep on this device")!!.click()
        assertNotNull("it is kept", text("On this device ✓", 90))
        text("Close")!!.click()

        // What was kept is a folder of its own.
        device.pressBack() // out of "Popular"
        shot("4b-after-back")
        assertNotNull("kept songs have a folder; on screen: ${visible()}", text("On this device"))
        text("On this device")!!.click()
        assertNotNull("kept: ${firstSong}", device.wait(Until.findObject(By.text("On device")), 20_000))
        shot("5-kept")
        device.pressBack()

        // Search.
        val box = device.wait(Until.findObject(By.clazz("android.widget.EditText")), 10_000)
        box.click()
        box.text = "stan"
        text("Search")!!.click()
        shot("6a-typed")
        assertNotNull("results; on screen: ${visible()}", contains("Search: stan"))
        assertNotNull("a result", contains("Stan"))
        shot("6-search")
        device.pressBack()

        // The settings.
        text("Settings")!!.click()
        assertNotNull(text("Crossfade: off"))
        assertNotNull("kept songs counted", contains("kept"))
        shot("7-settings")
        assertEquals(0, Settings.of(context).crossfadeSecs)
    }
}
