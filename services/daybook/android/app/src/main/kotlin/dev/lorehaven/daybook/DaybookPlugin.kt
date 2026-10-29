package dev.lorehaven.daybook

import android.app.Activity
import org.json.JSONObject

/** What the program asks of the app that only Kotlin can do: tell the home-screen widgets there is new content. */
class DaybookPlugin(private val activity: Activity) : Plugin("daybook") {
    override fun call(method: String, args: JSONObject, call: Call) {
        when (method) {
            "refresh_widgets" -> {
                DaybookWidgetProvider.refreshAll(activity.applicationContext)
                call.resolve()
            }
            else -> call.reject("unknown_method", "daybook has no method `$method`")
        }
    }
}
