package dev.lorehaven.daybook

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.widget.Toast
import org.json.JSONObject
import kotlin.concurrent.thread

/**
 * Makes a task with no window: the widget's quick-add dialog. The app's program does it (`libquench_surface.so --add ...`: it
 * reads the date in the title, sends the task, and writes the widget's snapshot). A task with a reminder comes back with the
 * arguments for the alarm, which are armed here (the program has no bridge to the notifications plugin outside the app).
 */
object QuickAdd {
    private const val TAG = "daybook-add"

    fun start(context: Context, title: String, list: String, due: String?, remind: String?) {
        val app = context.applicationContext
        thread(name = "add-task") {
            val args = mutableListOf("--add", "--title", title, "--list", list)
            if (due != null) args += listOf("--due", due)
            if (remind != null) args += listOf("--remind", remind)
            val lines = runCatching { Complete.exec(app, *args.toTypedArray()) }.getOrElse {
                Log.w(TAG, "could not run the program: $it")
                emptyList()
            }
            when (lines.firstOrNull()) {
                "added" -> lines.getOrNull(1)?.let { arm(app, it) }
                "queued" -> say(app, "Saved: it is sent when the network is back")
                "signed_out" -> SignedOutNotice.show(app)
                else -> say(app, "Could not add the task")
            }
            DaybookWidgetProvider.refreshAll(app)
        }
    }

    private fun arm(context: Context, json: String) {
        runCatching {
            val reminder = JSONObject(json)
            Reminders.save(context, reminder)
            Reminders.arm(context, reminder)
        }.onFailure { Log.w(TAG, "could not arm the reminder: $it") }
    }

    private fun say(context: Context, text: String) {
        Handler(Looper.getMainLooper()).post { Toast.makeText(context, text, Toast.LENGTH_LONG).show() }
    }
}
