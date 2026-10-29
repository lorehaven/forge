package dev.lorehaven.forklift

import android.app.Notification
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import kotlin.concurrent.thread

/**
 * The backup with no time limit: a foreground service that runs the same background run as the jobs ([Backup.run]) and
 * shows an ongoing notification with a Stop button. It is started when the window goes away with files still waiting,
 * and ends when there is nothing left to do (or the person stops it, or the system's own limit for this kind of service
 * is reached: the files in flight wait again, and the jobs carry on).
 */
class BackupService : Service() {
    @Volatile
    private var worker: Thread? = null

    @Volatile
    private var stopped = false

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == STOP) {
            stopped = true
            Backup.requestStop()
            // Nothing was running: nothing to keep.
            if (worker?.isAlive != true) stopSelf(startId)
            return START_NOT_STICKY
        }
        // A second start while it runs is the same service: nothing to add.
        if (worker?.isAlive == true) return START_NOT_STICKY
        val notification = Backup.progressNotification(this, "Backing up", true)
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(Backup.FOREGROUND, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            startForeground(Backup.FOREGROUND, notification)
        }
        stopped = false
        worker = thread(name = "backup-service") {
            try {
                // Another run may have the backup for a while (the window's program as it leaves, or a job, which the system
                // stops after its time): ask again until this one has it, or there is nothing left to send.
                while (!stopped && !Backup.foreground) {
                    if (Backup.run(applicationContext) { headline, _ -> show(headline) }) break
                    if (!Backup.workWaiting(applicationContext)) break
                    Thread.sleep(3_000)
                }
            } catch (e: Exception) {
                Log.w(Backup.TAG, "the foreground backup failed: ${e.message}")
            } finally {
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
            }
        }
        return START_NOT_STICKY
    }

    private fun show(headline: String) {
        val notification: Notification = Backup.progressNotification(this, headline, true)
        getSystemService(NotificationManager::class.java)?.notify(Backup.FOREGROUND, notification)
    }

    // Android 15 limits how long a data-sync service may run: stop in an orderly way, the jobs carry on.
    override fun onTimeout(startId: Int, fgsType: Int) {
        stopped = true
        Backup.requestStop()
        stopSelf()
    }

    companion object {
        const val STOP = "dev.lorehaven.forklift.STOP_BACKUP"
    }
}
