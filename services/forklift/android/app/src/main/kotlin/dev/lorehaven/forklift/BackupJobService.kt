package dev.lorehaven.forklift

import android.app.job.JobParameters
import android.app.job.JobService
import android.util.Log
import kotlin.concurrent.thread

/**
 * The scheduled backup: the periodic catch-up, the job that wakes when a photo or video appears, and the one started when
 * the window closes. All three do the same thing ([Backup.run]), off the main thread. The system may stop a job at any
 * time (no network any more, its time is up): the run is told to stop, files in flight wait again, and the next run
 * carries on.
 */
class BackupJobService : JobService() {
    @Volatile
    private var worker: Thread? = null

    override fun onStartJob(params: JobParameters): Boolean {
        // The window has the backup while it is open; it hands over when it closes.
        if (Backup.foreground || !Backup.enabled(applicationContext)) {
            // Signed out or switched off: the status notification goes too.
            Backup.updateStatus(applicationContext)
            Backup.arm(applicationContext)
            return false
        }
        worker = thread(name = "backup") {
            try {
                Backup.run(applicationContext) { headline, _ -> Backup.showJobProgress(applicationContext, this, params, headline) }
            } catch (e: Exception) {
                Log.w(Backup.TAG, "the backup failed: ${e.message}")
            }
            // The content trigger is one-shot: look out for the next new photo.
            Backup.arm(applicationContext)
            jobFinished(params, false)
        }
        return true
    }

    override fun onStopJob(params: JobParameters): Boolean {
        Backup.requestStop()
        return true
    }
}
