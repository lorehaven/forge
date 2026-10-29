package dev.lorehaven.daybook

import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.Context
import android.util.Log
import java.io.File
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread

/** Keeps the widget's snapshot fresh with the app closed: every 30 minutes, when there is a network. */
object WidgetRefresh {
    private const val JOB_ID = 2701
    private const val EVERY_MINUTES = 30L

    /** Schedules the job unless it is. A refusal is logged, never thrown: the widget just stays as fresh as the app made it. */
    fun schedule(context: Context) {
        runCatching {
            val scheduler = context.getSystemService(JobScheduler::class.java)
            if (scheduler.getPendingJob(JOB_ID) != null) return
            scheduler.schedule(
                JobInfo.Builder(JOB_ID, ComponentName(context, WidgetRefreshJobService::class.java))
                    .setPeriodic(TimeUnit.MINUTES.toMillis(EVERY_MINUTES))
                    .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
                    .setPersisted(true)
                    .build(),
            )
        }.onFailure { Log.w("daybook-widget", "cannot schedule the refresh: $it") }
    }

    fun cancel(context: Context) = context.getSystemService(JobScheduler::class.java).cancel(JOB_ID)
}

/** Runs the app's program with `--widget` (no window), then redraws the widgets. */
class WidgetRefreshJobService : JobService() {
    @Volatile private var process: Process? = null

    override fun onStartJob(params: JobParameters): Boolean {
        thread(name = "widget-refresh") {
            runCatching { run() }.onFailure { Log.w(TAG, "widget refresh failed: $it") }
            DaybookWidgetProvider.refreshAll(applicationContext)
            jobFinished(params, false)
        }
        return true
    }

    override fun onStopJob(params: JobParameters): Boolean {
        process?.destroy()
        return true
    }

    private fun run() {
        val program = File(applicationInfo.nativeLibraryDir, "libquench_surface.so")
        if (!program.exists()) return
        val started = ProcessBuilder(program.absolutePath, "--widget")
            .directory(filesDir)
            .redirectErrorStream(true)
            .apply {
                environment()["HOME"] = filesDir.absolutePath
                environment()["TZ"] = java.util.TimeZone.getDefault().id
            }
            .start()
        process = started
        val output = started.inputStream.bufferedReader().readText().trim()
        if (!started.waitFor(60, TimeUnit.SECONDS)) started.destroy()
        Log.i(TAG, "widget refresh: $output")
    }

    private companion object {
        const val TAG = "daybook-widget"
    }
}
