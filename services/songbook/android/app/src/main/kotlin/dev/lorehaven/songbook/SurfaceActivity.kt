package dev.lorehaven.songbook

import android.app.Activity
import android.content.res.Configuration
import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.window.OnBackInvokedDispatcher

/**
 * An activity whose whole screen is drawn by the app's Rust program ([QuenchSurfaceView]), with no WebView. It is
 * exported so it can be started with `adb shell am start -n dev.lorehaven.songbook/.SurfaceActivity`; make it the app's
 * launcher by moving the `LAUNCHER` intent filter of the manifest from `.MainActivity` to it.
 */
class SurfaceActivity : Activity() {
    private lateinit var surface: QuenchSurfaceView
    private lateinit var bridge: Bridge
    private var players: List<PlayerPlugin> = emptyList()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        surface = QuenchSurfaceView(this)
        surface.exitHandler = { finish() }
        // The program calls the app's plugins as a page does; their answers and events go back to it.
        val offered = plugins(this)
        players = offered.filterIsInstance<PlayerPlugin>()
        bridge = Bridge(this, null, offered, { surface.reply(it.toString()) }) { false }
        surface.bridge = bridge
        setContentView(surface)
        surface.setDark(isDark())
        surface.start()
        // The notification of what plays needs the person's leave (Android 13 and later).
        if (Build.VERSION.SDK_INT >= 33 && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
        }
        // Back is a callback from Android 13 on (and always for an app made for 16), not `onBackPressed`.
        if (Build.VERSION.SDK_INT >= 33) {
            onBackInvokedDispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT) { surface.back() }
        }
    }

    private fun isDark(): Boolean =
        (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        surface.setDark(isDark())
    }

    override fun onResume() {
        super.onResume()
        // The app is open: a "you are signed out" notice has done its work (a later sign-out is said again).
        SignedOutNotice.clear(this)
        surface.setVisible(true)
    }

    override fun onPause() {
        surface.setVisible(false)
        super.onPause()
    }

    // The program decides what Back does (close a sheet, go back a screen) and asks to leave when it has nothing left.
    @Deprecated("the platform's back handling")
    override fun onBackPressed() {
        if (Build.VERSION.SDK_INT < 33) surface.back()
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
        players.forEach { it.close() }
        surface.stop()
        super.onDestroy()
    }
}