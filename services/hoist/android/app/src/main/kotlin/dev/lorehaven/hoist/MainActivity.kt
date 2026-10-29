package dev.lorehaven.hoist

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.view.ViewGroup
import android.view.WindowInsets
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout

/**
 * One WebView showing the app. In a release build it serves the bundled web app from its own https
 * origin. A debug build can instead load the dev server (`quench dev`): start the activity with the
 * extra `dev_url`, for instance
 *
 *     adb reverse tcp:3000 tcp:3000
 *     adb shell am start -n dev.lorehaven.hoist/.MainActivity --es dev_url http://127.0.0.1:3000
 *
 * The URL is remembered until a start with `--es dev_url off` clears it.
 */
class MainActivity : Activity() {
    private lateinit var web: WebView
    private lateinit var assets_: AppAssets
    private lateinit var bridge: Bridge

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        assets_ = AppAssets(assets)
        if (BuildConfig.DEBUG) WebView.setWebContentsDebuggingEnabled(true)

        web = WebView(this)
        web.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            allowFileAccess = false
            allowContentAccess = false
            mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
            // The app sets its own sizes; the system font scale reaches it through rem units.
            textZoom = 100
            // How the page knows it is shown by a host that has plugins.
            userAgentString = "$userAgentString ${Bridge.USER_AGENT}"
        }
        bridge = Bridge(this, web, plugins(this)) { url -> assets_.owns(url) || isDevServer(url) }
        web.webViewClient = Client()
        val root = FrameLayout(this)
        root.addView(web, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        setContentView(root)
        // From Android 15 the window is edge to edge: keep the page clear of the bars and the keyboard.
        root.setOnApplyWindowInsetsListener { view, insets ->
            if (android.os.Build.VERSION.SDK_INT >= 30) {
                val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.ime())
                view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
                WindowInsets.CONSUMED
            } else {
                insets
            }
        }

        // What started the app (a share, a file) is offered to the plugins that care.
        if (isOffered(intent)) bridge.onIntent(intent)
        if (savedInstanceState != null) web.restoreState(savedInstanceState) else load(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        if (intent.hasExtra(DEV_URL)) load(intent) else if (isOffered(intent)) bridge.onIntent(intent)
    }

    @Deprecated("The plugins use the framework's result callback.")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        bridge.onActivityResult(requestCode, resultCode, data)
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        web.saveState(outState)
    }

    @Deprecated("The activity opts out of predictive back, so this is the back button.")
    override fun onBackPressed() {
        if (web.canGoBack()) web.goBack() else super.onBackPressed()
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        bridge.onPermissionResult(requestCode, grantResults)
    }

    override fun onDestroy() {
        bridge.disconnect()
        web.destroy()
        super.onDestroy()
    }

    /** Whether [intent] came from another app with something to say: not a plain start of the app. */
    private fun isOffered(intent: Intent) = intent.action != null && intent.action != Intent.ACTION_MAIN

    private fun load(intent: Intent?) {
        web.loadUrl(startUrl(intent))
    }

    /** The page to show: the dev server in a debug build that was given one, else the bundle. */
    private fun startUrl(intent: Intent?): String {
        if (!BuildConfig.DEBUG) return "${AppAssets.ORIGIN}/"
        val prefs = getSharedPreferences("quench-dev", Context.MODE_PRIVATE)
        if (intent?.hasExtra(DEV_URL) == true) {
            // `off` (an empty argument cannot be passed through `adb shell`) goes back to the bundle.
            val url = intent.getStringExtra(DEV_URL).orEmpty()
            prefs.edit().putString(DEV_URL, if (url == "off") "" else url).apply()
        }
        val dev = prefs.getString(DEV_URL, "").orEmpty()
        return dev.ifEmpty { "${AppAssets.ORIGIN}/" }
    }

    private inner class Client : WebViewClient() {
        // A page gets the bridge when it has loaded, and loses it when it goes: one port per document.
        override fun onPageStarted(view: WebView, url: String?, favicon: android.graphics.Bitmap?) = bridge.disconnect()

        override fun onPageFinished(view: WebView, url: String?) = bridge.connect(url)

        override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? =
            if (assets_.owns(request.url)) assets_.serve(request.url) else null

        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
            val url = request.url
            if (assets_.owns(url) || isDevServer(url)) return false
            // Anything else is not the app: hand it to the system's browser.
            if (url.scheme == "https" || url.scheme == "http" || url.scheme == "mailto") {
                startActivity(Intent(Intent.ACTION_VIEW, url))
            }
            return true
        }
    }

    private fun isDevServer(url: Uri): Boolean {
        if (!BuildConfig.DEBUG) return false
        val dev = getSharedPreferences("quench-dev", Context.MODE_PRIVATE).getString(DEV_URL, "").orEmpty()
        return dev.isNotEmpty() && Uri.parse(dev).let { it.scheme == url.scheme && it.authority == url.authority }
    }

    companion object {
        const val DEV_URL = "dev_url"
    }
}