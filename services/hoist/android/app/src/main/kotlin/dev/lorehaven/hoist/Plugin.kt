package dev.lorehaven.hoist

import org.json.JSONObject

/**
 * One native capability the web app can call, by [name]: the plugin name of its Rust client
 * (`#[quench_ui_web::plugin("echo")]`). A call arrives as a method name and a JSON object of
 * arguments; the answer goes through [Call], once, now or later (any thread).
 *
 * A plugin is listed in [plugins] to take part.
 */
abstract class Plugin(val name: String) {
    /** Set by the bridge: how [emit] reaches the page. */
    internal var bridge: Bridge? = null

    /** Pushes an event to the page (progress, a new intent); it is lost if no page is connected. */
    fun emit(event: String, data: JSONObject = JSONObject()) {
        bridge?.emit(name, event, data)
    }

    /** The page went away (reload, navigation): stop what only it was interested in. */
    open fun disconnected() {}

    /** The result of an activity this plugin started with `startActivityForResult`. */
    open fun onActivityResult(requestCode: Int, resultCode: Int, data: android.content.Intent?) {}

    /** The app was opened or brought to the front by [intent] (a share, a file, a link). */
    open fun onIntent(intent: android.content.Intent) {}

    abstract fun call(method: String, args: JSONObject, call: Call)
}