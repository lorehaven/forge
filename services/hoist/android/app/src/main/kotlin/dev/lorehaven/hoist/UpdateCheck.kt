package dev.lorehaven.hoist

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.job.JobInfo
import android.app.job.JobScheduler
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.os.Build
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.concurrent.TimeUnit

/**
 * The look for updates that runs while the app is closed: a scheduled job ([UpdateCheckService]) starts the app's
 * program in its `--check` mode, which signs in with the saved session, reads the registry and says what is newer than
 * what is installed. It says it once for each set of updates (the program remembers), and this posts the
 * notification. All of the deciding is the program's; this only asks the package manager what is installed and
 * shows what comes back.
 */
object UpdateCheck {
    private const val TAG = "hoist-check"
    private const val PROGRAM = "libquench_surface.so"
    const val JOB_ID = 4100
    private const val CHANNEL = "updates"
    private const val NOTIFICATION = 1
    private val EVERY = TimeUnit.HOURS.toMillis(6)

    /** Makes sure the job is scheduled (once: asking again would only move its next run). */
    fun schedule(context: Context) {
        // Whatever goes wrong here, the app opens: a missing look for updates is a smaller thing than a crash.
        try {
            val scheduler = context.getSystemService(JobScheduler::class.java) ?: return
            val pending = scheduler.getPendingJob(JOB_ID)
            if (pending != null && pending.isPeriodic && pending.intervalMillis == EVERY) return
            val job = JobInfo.Builder(JOB_ID, ComponentName(context, UpdateCheckService::class.java))
                .setPeriodic(EVERY)
                .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
                .setPersisted(true)
                .build()
            val result = scheduler.schedule(job)
            Log.i(TAG, "scheduled every ${EVERY / 3_600_000} h: ${if (result == JobScheduler.RESULT_SUCCESS) "ok" else "refused"}")
        } catch (e: Exception) {
            Log.w(TAG, "could not schedule the look for updates: ${e.message}")
        }
    }

    /** The apps the user installed, as the program wants them. */
    private fun installed(context: Context): String {
        val apps = JSONArray()
        for (info in context.packageManager.getInstalledPackages(0)) {
            val flags = info.applicationInfo?.flags ?: 0
            val system = flags and ApplicationInfo.FLAG_SYSTEM != 0 && flags and ApplicationInfo.FLAG_UPDATED_SYSTEM_APP == 0
            if (system) continue
            apps.put(
                JSONObject()
                    .put("package", info.packageName)
                    .put("version_code", info.longVersionCode)
                    .put("version_name", info.versionName ?: ""),
            )
        }
        return apps.toString()
    }

    /** One look. Runs on the job's own thread: it waits for the program and the network. */
    fun run(context: Context) {
        val program = File(context.applicationInfo.nativeLibraryDir, PROGRAM)
        if (!program.exists()) {
            Log.w(TAG, "the program is not in this build")
            return
        }
        val process = ProcessBuilder(program.absolutePath, "--check")
            .directory(context.filesDir)
            .apply { environment()["HOME"] = context.filesDir.absolutePath }
            .start()
        process.outputStream.use { it.write(installed(context).toByteArray()) }
        val errors = Thread { process.errorStream.bufferedReader().forEachLine { Log.w(TAG, it) } }.apply { start() }
        val line = process.inputStream.bufferedReader().readText().trim()
        if (!process.waitFor(60, TimeUnit.SECONDS)) process.destroy()
        errors.join(1000)
        val answer = try {
            JSONObject(line)
        } catch (e: org.json.JSONException) {
            Log.w(TAG, "no answer: '$line'")
            return
        }
        val status = answer.optString("status")
        Log.i(TAG, "checked: $status, ${answer.optInt("count")} update(s), notify=${answer.optBoolean("notify")}")
        if (status == "updates" && answer.optBoolean("notify")) {
            notify(context, "Hoist: " + answer.optString("title"), answer.optString("body"))
        }
    }

    private fun notify(context: Context, title: String, body: String) {
        if (Build.VERSION.SDK_INT >= 33 &&
            context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            Log.w(TAG, "updates found, but notifications are not allowed")
            return
        }
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, "Updates", NotificationManager.IMPORTANCE_DEFAULT).apply {
                description = "New versions of the apps you have installed"
            },
        )
        val open = PendingIntent.getActivity(
            context,
            0,
            Intent(context, SurfaceActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        manager.notify(
            NOTIFICATION,
            android.app.Notification.Builder(context, CHANNEL)
                .setSmallIcon(R.drawable.ic_stat_update)
                .setContentTitle(title)
                .setContentText(body)
                .setStyle(android.app.Notification.BigTextStyle().bigText(body))
                .setContentIntent(open)
                .setAutoCancel(true)
                .build(),
        )
    }
}
