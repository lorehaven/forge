package dev.lorehaven.forklift

import android.Manifest
import android.app.Activity
import android.content.pm.PackageManager
import android.os.Build
import android.content.res.Configuration
import android.os.Bundle

/**
 * An activity whose whole screen is drawn by the app's Rust program ([QuenchSurfaceView]), with no WebView. It is
 * exported so it can be started with `adb shell am start -n dev.lorehaven.forklift/.SurfaceActivity`; make it the app's
 * launcher by moving the `LAUNCHER` intent filter of the manifest from `.MainActivity` to it.
 */
class SurfaceActivity : Activity() {
    private lateinit var surface: QuenchSurfaceView
    private lateinit var bridge: Bridge

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // A background run that is going gives way to the window before the program starts.
        Backup.windowOpened(this)
        surface = QuenchSurfaceView(this)
        surface.exitHandler = { finish() }
        // The program calls the app's plugins as a page does; their answers and events go back to it.
        bridge = Bridge(this, null, plugins(this), { surface.reply(it.toString()) }) { false }
        surface.bridge = bridge
        setContentView(surface)
        surface.setDark(isDark())
        surface.start()
        // The backup that goes on while the window is closed: the catch-up, and the wait for new photos.
        Backup.schedule(this)
        askToNotify()
    }

    /** From Android 13 a notification needs the person's yes: asked once, when the app starts. */
    private fun askToNotify() {
        val asked = getPreferences(MODE_PRIVATE)
        if (Build.VERSION.SDK_INT >= 33 &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED &&
            !asked.getBoolean("asked_to_notify", false)
        ) {
            asked.edit().putBoolean("asked_to_notify", true).apply()
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), NOTIFICATIONS)
        }
    }

    private fun isDark(): Boolean =
        (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        surface.setDark(isDark())
    }

    override fun onStart() {
        super.onStart()
        Backup.windowOpened(this)
    }

    // Out of sight: what is left is the job's from here on.
    override fun onStop() {
        Backup.windowClosed(this)
        super.onStop()
    }

    override fun onResume() {
        super.onResume()
        surface.setVisible(true)
    }

    override fun onPause() {
        surface.setVisible(false)
        super.onPause()
        // Still in front for a moment: the one time a foreground service may be started for what is left.
        Backup.windowPaused(this)
    }

    private companion object {
        const val NOTIFICATIONS = 4101
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
        bridge.onIntent(intent)
    }

    override fun onDestroy() {
        surface.stop()
        super.onDestroy()
    }
}