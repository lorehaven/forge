package dev.lorehaven.klaxon

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import kotlin.concurrent.thread

/**
 * What the system tells the app: an alarm is due ([Klaxon.RING]), or something that clears the system's alarms
 * happened (a restart, an update, the clock or the zone changed) and the next one is set again.
 */
class AlarmReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val app = context.applicationContext
        when (intent.action) {
            Klaxon.RING -> {
                val ring = Ring.of(intent)
                val id = ring.id
                val snoozed = intent.getBooleanExtra("snoozed", false)
                // The sound starts at once; the next alarm is worked out beside it.
                RingService.start(app, ring)
                val pending = goAsync()
                thread(name = "klaxon-next") {
                    try {
                        // A ring from a snooze is not the alarm's own time: nothing to switch off.
                        if (snoozed) Klaxon.reschedule(app) else Klaxon.reschedule(app, id)
                    } finally {
                        pending.finish()
                    }
                }
            }
            Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED,
            Intent.ACTION_TIME_CHANGED, Intent.ACTION_TIMEZONE_CHANGED -> {
                val pending = goAsync()
                thread(name = "klaxon-reschedule") {
                    try {
                        Klaxon.reschedule(app)
                    } finally {
                        pending.finish()
                    }
                }
            }
        }
    }
}
