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

    @Volatile
    private var stopping = false

    override fun onStartJob(params: JobParameters): Boolean {
        Backup.logJob(applicationContext, "start job ${params.jobId} foreground=${Backup.foreground} unrestricted=${Backup.unrestricted(applicationContext)}")
        // The window has the backup while it is open; it hands over when it closes.
        if (Backup.foreground || !Backup.enabled(applicationContext)) {
            // Signed out or switched off: the status notification goes too.
            Backup.updateStatus(applicationContext)
            Backup.arm(applicationContext)
            return false
        }
        stopping = false
        worker = thread(name = "backup") {
            var busy = false
            try {
                // The window's program may still hold the backup (a frozen or just-hidden app lets go only a moment after
                // it wakes): ask again for a little while instead of giving up and waiting for the next period.
                for (attempt in 1..10) {
                    if (Backup.foreground || stopping) break
                    busy = !Backup.run(applicationContext) { headline, _ -> Backup.showJobProgress(applicationContext, this, params, headline) }
                    if (!busy) break
                    Backup.logJob(applicationContext, "job ${params.jobId} busy (attempt $attempt)")
                    Thread.sleep(3_000)
                }
            } catch (e: Exception) {
                Log.w(Backup.TAG, "the backup failed: ${e.message}")
            }
            // The content trigger is one-shot: look out for the next new photo.
            Backup.arm(applicationContext)
            Backup.updateStatus(applicationContext)
            Backup.logJob(applicationContext, "end job ${params.jobId} busy=$busy")
            // Still busy: have the system run it again soon rather than at the next period.
            jobFinished(params, busy && !stopping)
        }
        return true
    }

    override fun onStopJob(params: JobParameters): Boolean {
        stopping = true
        Backup.logJob(applicationContext, "stopped job ${params.jobId} reason=${if (android.os.Build.VERSION.SDK_INT >= 31) params.stopReason else -1}")
        Backup.requestStop()
        return true
    }
}
