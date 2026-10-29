package dev.lorehaven.hoist

import android.app.Activity
import android.content.res.Configuration
import android.os.Bundle

/**
 * An activity whose whole screen is drawn by the app's Rust program ([QuenchSurfaceView]), with no WebView. It is
 * exported so it can be started with `adb shell am start -n dev.lorehaven.hoist/.SurfaceActivity`; make it the app's
 * launcher by moving the `LAUNCHER` intent filter of the manifest from `.MainActivity` to it.
 */
class SurfaceActivity : Activity() {
    private lateinit var surface: QuenchSurfaceView
    private lateinit var bridge: Bridge

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        surface = QuenchSurfaceView(this)
        surface.exitHandler = { finish() }
        // The program calls the app's plugins as a page does; their answers and events go back to it.
        bridge = Bridge(this, null, plugins(this), { surface.reply(it.toString()) }) { false }
        surface.bridge = bridge
        setContentView(surface)
        surface.setDark(isDark())
        surface.start()
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
        bridge.onIntent(intent)
    }

    override fun onDestroy() {
        surface.stop()
        super.onDestroy()
    }
}