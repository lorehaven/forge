package dev.lorehaven.daybook

import android.app.NotificationManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.widget.Toast
import java.io.File
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread

/**
 * Marks a task done with no window: the reminder's *Done* button and the widget's check mark. The app's program does it
 * (`libquench_surface.so --complete <task id>`: it reads the tasks, sends the change, and writes the widget's snapshot); with the
 * services out of reach the change waits in the app's outbox and goes out the next time the app starts.
 */
object Complete {
    const val ACTION = "dev.lorehaven.daybook.COMPLETE"
    private const val TAG = "daybook-complete"

    /** Runs it on a thread of its own, and tells what happened to the notification, the widget and the person. */
    fun start(context: Context, taskId: String, notificationId: Int, then: () -> Unit = {}) {
        val app = context.applicationContext
        thread(name = "complete-task") {
            val outcome = runCatching { run(app, taskId) }.getOrElse {
                Log.w(TAG, "could not run the program: $it")
                "failed"
            }
            if (notificationId != 0) app.getSystemService(NotificationManager::class.java)?.cancel(notificationId)
            DaybookWidgetProvider.refreshAll(app)
            when (outcome) {
                "signed_out" -> SignedOutNotice.show(app)
                "queued" -> say(app, "Saved: it is sent when the network is back")
                "failed" -> say(app, "Could not mark the task done")
            }
            then()
        }
    }

    private fun say(context: Context, text: String) {
        Handler(Looper.getMainLooper()).post { Toast.makeText(context, text, Toast.LENGTH_LONG).show() }
    }

    private fun run(context: Context, taskId: String): String = exec(context, "--complete", taskId).lastOrNull().orEmpty()

    /** Runs the app's program with [args] and gives the lines it printed (none when it could not run). */
    fun exec(context: Context, vararg args: String): List<String> {
        val program = File(context.applicationInfo.nativeLibraryDir, "libquench_surface.so")
        if (!program.exists()) return emptyList()
        val started = ProcessBuilder(program.absolutePath, *args)
            .directory(context.filesDir)
            .redirectErrorStream(false)
            .apply {
                environment()["HOME"] = context.filesDir.absolutePath
                SessionKey.hex(context)?.let { environment()["FORGE_SESSION_KEY"] = it }
                environment()["TZ"] = java.util.TimeZone.getDefault().id
            }
            .start()
        val errors = thread(name = "complete-stderr") { runCatching { started.errorStream.bufferedReader().forEachLine { Log.w(TAG, it) } } }
        val output = started.inputStream.bufferedReader().readText().trim()
        if (!started.waitFor(40, TimeUnit.SECONDS)) started.destroy()
        errors.join(1000)
        Log.i(TAG, "${args.first()}: ${output.lineSequence().firstOrNull().orEmpty()}")
        return output.lines().filter { it.isNotBlank() }
    }
}

/** The *Done* button of a reminder. */
class CompleteReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val taskId = intent.getStringExtra("task_id") ?: return
        // The work takes a network round trip: the receiver is kept alive for it.
        val pending = goAsync()
        Complete.start(context, taskId, intent.getIntExtra("notification_id", 0)) { pending.finish() }
    }
}
