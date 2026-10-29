package dev.lorehaven.hoist

import android.os.Build
import org.json.JSONObject

/**
 * A plugin that exists to try the bridge: it returns what it is given, says what device it runs on,
 * adds, fails on request, and asks for a permission. Remove it from [plugins] when the app is real.
 */
class EchoPlugin : Plugin("echo") {
    override fun call(method: String, args: JSONObject, call: Call) {
        when (method) {
            "say" -> call.resolve(args.getString("text"))
            "device" -> call.resolve(JSONObject().put("sdk", Build.VERSION.SDK_INT).put("model", Build.MODEL))
            "add" -> call.resolve(args.getLong("a") + args.getLong("b"))
            "fail" -> call.reject(args.optString("code", "failed"), "asked to fail")
            // The permission flow: the system's dialog is shown if the permission is not granted yet.
            "notifications" -> call.permissions("android.permission.POST_NOTIFICATIONS") { granted ->
                if (granted) call.resolve(true)
                else call.reject("permission_denied", "notifications are not allowed")
            }
            else -> call.reject("unknown_method", "echo has no method `$method`")
        }
    }
}