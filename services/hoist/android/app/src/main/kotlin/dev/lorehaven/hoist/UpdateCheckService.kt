package dev.lorehaven.hoist

import android.app.job.JobParameters
import android.app.job.JobService
import android.util.Log
import kotlin.concurrent.thread

/** The scheduled job: a look for updates ([UpdateCheck]), off the main thread, now and then while the app is closed. */
class UpdateCheckService : JobService() {
    @Volatile
    private var worker: Thread? = null

    override fun onStartJob(params: JobParameters): Boolean {
        worker = thread(name = "update-check") {
            try {
                UpdateCheck.run(applicationContext)
            } catch (e: Exception) {
                Log.w("hoist-check", "the look for updates failed: ${e.message}")
            }
            jobFinished(params, false)
        }
        return true
    }

    // The system took the job away (no network any more, the device is busy): try again when it can.
    override fun onStopJob(params: JobParameters): Boolean {
        worker?.interrupt()
        return true
    }
}
