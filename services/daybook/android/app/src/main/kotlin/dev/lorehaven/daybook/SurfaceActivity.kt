package dev.lorehaven.daybook

import android.app.Activity
import android.content.res.Configuration
import android.content.Intent
import android.os.Bundle
import java.io.File

/**
 * An activity whose whole screen is drawn by the app's Rust program ([QuenchSurfaceView]), with no WebView. It is
 * exported so it can be started with `adb shell am start -n dev.lorehaven.daybook/.SurfaceActivity`; make it the app's
 * launcher by moving the `LAUNCHER` intent filter of the manifest from `.MainActivity` to it.
 */
class SurfaceActivity : Activity() {
    private lateinit var surface: QuenchSurfaceView
    private lateinit var bridge: Bridge

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        handleLink(intent)
        surface = QuenchSurfaceView(this)
        surface.exitHandler = { finish() }
        // The program calls the app's plugins as a page does; their answers and events go back to it.
        bridge = Bridge(this, null, plugins(this), { surface.reply(it.toString()) }) { false }
        surface.bridge = bridge
        setContentView(surface)
        surface.setDark(isDark())
        surface.start()
        // Keeps the home-screen widget fresh with the app closed (a no-op while the job is already scheduled).
        WidgetRefresh.schedule(this)
    }

    /**
     * A tap on the widget (`daybook://add?list=...` or `daybook://open?list=...&task=...`) says what to show: the program
     * reads it from the `launch` file in its data directory, which the program deletes as it reads.
     */
    private fun handleLink(intent: Intent?) {
        val link = intent?.data ?: return
        if (link.scheme != "daybook") return
        val action = link.host ?: return
        if (action != "add" && action != "open") return
        val text = "$action\n${link.getQueryParameter("list") ?: "inbox"}\n${link.getQueryParameter("task") ?: ""}\n"
        val dir = WidgetData.dataDir(this)
        dir.mkdirs()
        File(dir, "launch.tmp").writeText(text)
        File(dir, "launch.tmp").renameTo(File(dir, "launch"))
    }

    private fun isDark(): Boolean =
        (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        surface.setDark(isDark())
    }

    override fun onResume() {
        super.onResume()
        surface.setVisible(true)
    }

    override fun onPause() {
        surface.setVisible(false)
        super.onPause()
    }

    // The program decides what Back does (close a sheet, go back a screen) and asks to leave when it has nothing left.
    @Deprecated("the platform's back handling")
    override fun onBackPressed() {
        surface.back()
    }

    @Deprecated("the platform's result handling")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: android.content.Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        bridge.onActivityResult(requestCode, resultCode, data)
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        bridge.onPermissionResult(requestCode, grantResults)
    }

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        handleLink(intent)
        bridge.onIntent(intent)
    }

    override fun onDestroy() {
        surface.stop()
        super.onDestroy()
    }
}