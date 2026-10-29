package dev.lorehaven.forklift

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Keeps the quiet status notification ([Backup.updateStatus]) where the person left it: it is put back when they swipe
 * it away, after a restart, and after the app is updated. (Android lets people dismiss even an ongoing notification;
 * mathom's stayed until the person signed out or switched automatic backup off, and so does this one. The way to be
 * rid of it for good is the channel's switch in the system's notification settings.)
 */
class BackupReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
            STATUS_DISMISSED -> Backup.updateStatus(context)
            Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED -> {
                // The jobs are kept across a restart; the notification and the wait for new photos are not.
                Backup.schedule(context)
            }
        }
    }

    companion object {
        const val STATUS_DISMISSED = "dev.lorehaven.forklift.STATUS_DISMISSED"
    }
}
