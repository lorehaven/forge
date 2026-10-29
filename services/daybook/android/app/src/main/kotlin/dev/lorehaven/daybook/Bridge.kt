package dev.lorehaven.daybook

import android.app.Activity
import android.content.pm.PackageManager
import android.net.Uri
import android.util.Log
import android.util.SparseArray
import android.webkit.WebMessage
import android.webkit.WebMessagePort
import android.webkit.WebView
import org.json.JSONException
import org.json.JSONObject

/**
 * The page's way to the plugins. The host gives the page one `MessagePort`, and only if the page is
 * the app's own (the bundle's origin, or the dev server in a debug build): `postWebMessage` is
 * addressed to that origin, so a page from anywhere else never receives it, and [connect] does not
 * even try. Each message is a JSON envelope, `{"id","plugin","method","args"}`, answered by
 * `{"id","ok"}` or `{"id","err":{"code","message"}}`.
 */
class Bridge(
    internal val activity: Activity,
    private val web: WebView?,
    plugins: List<Plugin>,
    /** Where answers and events go when there is no page: a program that draws the screen (see `SurfaceHost`). */
    private val sink: ((JSONObject) -> Unit)? = null,
    private val allowed: (Uri) -> Boolean,
) {
    companion object {
        const val TAG = "quench-bridge"

        /** Added to the WebView's user agent: how the page knows a host is there. */
        const val USER_AGENT = "QuenchHost/1"

        /** The largest message taken; the rest of the app is not meant to move data this way. */
        const val MAX_MESSAGE = 1 shl 20

        fun origin(uri: Uri) = "${uri.scheme}://${uri.authority}"
    }

    private val byName = plugins.associateBy { it.name }

    init {
        plugins.forEach { it.bridge = this }
    }
    private var port: WebMessagePort? = null
    private val permissionCallbacks = SparseArray<(Boolean) -> Unit>()
    private var nextRequest = 1

    /** Opens the port to the page that just loaded at [url], if the page is the app's. */
    fun connect(url: String?) {
        disconnect()
        val web = web ?: return
        val uri = Uri.parse(url ?: return)
        if (!allowed(uri)) {
            Log.w(TAG, "refused origin ${origin(uri)}: the page gets no bridge")
            return
        }
        try {
            val ports = web.createWebMessageChannel()
            ports[0].setWebMessageCallback(object : WebMessagePort.WebMessageCallback() {
                override fun onMessage(port: WebMessagePort, message: WebMessage) = handle(message.data)
            })
            web.postWebMessage(WebMessage("quench-bridge", arrayOf(ports[1])), Uri.parse(origin(uri)))
            port = ports[0]
            Log.i(TAG, "connected to ${origin(uri)}")
        } catch (e: Exception) {
            Log.w(TAG, "could not open the bridge: ${e.message}")
        }
    }

    /** Closes the port: a page that is going away must not be answered. */
    fun disconnect() {
        port?.close()
        port = null
        byName.values.forEach { it.disconnected() }
    }

    fun onActivityResult(requestCode: Int, resultCode: Int, data: android.content.Intent?) =
        byName.values.forEach { it.onActivityResult(requestCode, resultCode, data) }

    fun onIntent(intent: android.content.Intent) = byName.values.forEach { it.onIntent(intent) }

    /** One call, as the JSON envelope a page sends (a program that draws the screen sends the same). */
    fun handle(text: String?) {
        if (text == null || text.length > MAX_MESSAGE) return
        val message = try {
            JSONObject(text)
        } catch (_: JSONException) {
            return // no id to answer to
        }
        val id = message.opt("id") ?: return
        val call = Call(this, id)
        val plugin = byName[message.optString("plugin")]
            ?: return call.reject("unknown_plugin", "no plugin `${message.optString("plugin")}`")
        try {
            plugin.call(message.optString("method"), message.optJSONObject("args") ?: JSONObject(), call)
        } catch (e: JSONException) {
            call.reject("invalid_args", e.message ?: "the arguments are not what the method needs")
        } catch (e: Exception) {
            Log.e(TAG, "${plugin.name}.${message.optString("method")} failed", e)
            call.reject("internal", e.message ?: e.javaClass.simpleName)
        }
    }

    internal fun reply(message: JSONObject) {
        sink?.let { return it(message) }
        // Answers may come from any thread; the port is used on the main one.
        activity.runOnUiThread { port?.postMessage(WebMessage(message.toString())) }
    }

    internal fun emit(plugin: String, event: String, data: JSONObject) {
        reply(JSONObject().put("event", event).put("plugin", plugin).put("data", data))
    }

    /** Asks for runtime permissions (the system shows its dialog unless they are granted already). */
    internal fun requestPermissions(names: Array<out String>, then: (Boolean) -> Unit) {
        if (names.all { activity.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED }) {
            then(true)
            return
        }
        val code = nextRequest++
        permissionCallbacks.put(code, then)
        activity.requestPermissions(arrayOf(*names), code)
    }

    /** From the activity's `onRequestPermissionsResult`. */
    fun onPermissionResult(code: Int, results: IntArray) {
        val then = permissionCallbacks.get(code) ?: return
        permissionCallbacks.remove(code)
        then(results.isNotEmpty() && results.all { it == PackageManager.PERMISSION_GRANTED })
    }
}

/** The answer to one call: [resolve] or [reject], once; later calls are ignored. */
class Call internal constructor(private val bridge: Bridge, private val id: Any) {
    private var answered = false

    fun resolve(value: Any? = null) = answer(JSONObject().put("ok", JSONObject.wrap(value) ?: JSONObject.NULL))

    fun reject(code: String, message: String) =
        answer(JSONObject().put("err", JSONObject().put("code", code).put("message", message)))

    /** Asks for permissions, then runs [then] with whether every one was granted. */
    fun permissions(vararg names: String, then: (Boolean) -> Unit) = bridge.requestPermissions(names, then)

    private fun answer(body: JSONObject) {
        synchronized(this) {
            if (answered) return
            answered = true
        }
        bridge.reply(body.put("id", id))
    }
}