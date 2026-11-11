package dev.lorehaven.klaxon

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.util.Log
import org.json.JSONObject
import java.io.File
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId
import java.time.ZonedDateTime
import java.util.concurrent.TimeUnit

/**
 * The system's side of the alarm clock. All the deciding (which alarm rings next, on which day, and what a one-time
 * alarm does once it has rung) is the Rust program's: it is started here with `--next` or `--fired <id>`, and answers
 * with one line of JSON. This object turns that answer into a system alarm ([AlarmManager.setAlarmClock], which Doze
 * lets through and the status bar shows) and a snooze into another one.
 */
object Klaxon {
    const val TAG = "klaxon"
    private const val PROGRAM = "libquench_surface.so"
    private const val REGULAR = 100
    private const val SNOOZED = 101

    /** The alarm that rings (broadcast by the system, or by a snooze). */
    const val RING = "dev.lorehaven.klaxon.RING"

    /** Runs the program with [args] and returns its one line of JSON, or null if it could not be run. */
    fun ask(context: Context, vararg args: String): JSONObject? {
        val program = File(context.applicationInfo.nativeLibraryDir, PROGRAM)
        if (!program.exists()) return null
        return try {
            val process = ProcessBuilder(listOf(program.absolutePath) + args)
                .directory(context.filesDir)
                .redirectErrorStream(false)
                .apply { environment()["HOME"] = context.filesDir.absolutePath }
                .start()
            val line = process.inputStream.bufferedReader().readLine()
            if (!process.waitFor(10, TimeUnit.SECONDS)) process.destroyForcibly()
            line?.let { JSONObject(it) }
        } catch (e: Exception) {
            Log.w(TAG, "could not run the program: ${e.message}")
            null
        }
    }

    /** Sets the system's alarm for the next ring, or removes it when none is on. [fired] is an alarm that just rang. */
    fun reschedule(context: Context, fired: Int? = null) {
        val next = (if (fired != null) ask(context, "--fired", fired.toString()) else ask(context, "--next")) ?: return
        schedule(context, next)
    }

    /** Sets the alarm from the program's JSON (`{"none":true}` removes it). */
    fun schedule(context: Context, next: JSONObject) {
        val alarms = context.getSystemService(AlarmManager::class.java) ?: return
        val operation = ringIntent(context, REGULAR, next)
        if (next.optBoolean("none", false)) {
            alarms.cancel(operation)
            return
        }
        val day = LocalDate.now().plusDays(next.getLong("days_ahead"))
        // The wall clock's time on that day, in the zone the phone is in then: a change of clocks is the zone's rule.
        val at = ZonedDateTime.of(day, LocalTime.of(next.getInt("hour"), next.getInt("minute")), ZoneId.systemDefault())
        set(context, alarms, at.toInstant().toEpochMilli(), operation)
    }

    /** Rings the alarm again in [minutes], the same way: [Ring] is what it does. */
    fun snooze(context: Context, ring: Ring) {
        val alarms = context.getSystemService(AlarmManager::class.java) ?: return
        val minutes = ring.snooze
        val next = ring.toJson().put("snoozed", true)
        set(context, alarms, System.currentTimeMillis() + TimeUnit.MINUTES.toMillis(minutes.toLong()), ringIntent(context, SNOOZED, next))
    }

    private fun set(context: Context, alarms: AlarmManager, at: Long, operation: PendingIntent) {
        try {
            // The "show" intent is what the system's clock icon opens.
            val show = PendingIntent.getActivity(
                context, 0, Intent(context, SurfaceActivity::class.java),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
            alarms.setAlarmClock(AlarmManager.AlarmClockInfo(at, show), operation)
        } catch (e: SecurityException) {
            Log.w(TAG, "the system refused the alarm: ${e.message}")
        }
    }

    private fun ringIntent(context: Context, code: Int, next: JSONObject): PendingIntent {
        val intent = Ring.of(next).into(Intent(context, AlarmReceiver::class.java).setAction(RING))
            .putExtra("snoozed", next.optBoolean("snoozed", false))
        return PendingIntent.getBroadcast(context, code, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }
}

/** What an alarm does when it rings, as it travels from the program's JSON to the intents and back. */
data class Ring(
    val id: Int,
    val label: String,
    val snooze: Int,
    val vibrate: Boolean,
    val loud: Boolean,
    /** The sound's URI, or empty for the phone's default alarm sound. */
    val sound: String,
) {
    fun toJson(): JSONObject = JSONObject().put("id", id).put("label", label).put("snooze", snooze)
        .put("vibrate", vibrate).put("loud", loud).put("sound", sound)

    fun into(intent: Intent): Intent = intent.putExtra("id", id).putExtra("label", label).putExtra("snooze", snooze)
        .putExtra("vibrate", vibrate).putExtra("loud", loud).putExtra("sound", sound)

    companion object {
        fun of(json: JSONObject) = Ring(
            json.optInt("id", 0), json.optString("label", ""), json.optInt("snooze", 10),
            json.optBoolean("vibrate", true), json.optBoolean("loud", false), json.optString("sound", ""),
        )

        fun of(intent: Intent?) = Ring(
            intent?.getIntExtra("id", 0) ?: 0, intent?.getStringExtra("label") ?: "", intent?.getIntExtra("snooze", 10) ?: 10,
            intent?.getBooleanExtra("vibrate", true) ?: true, intent?.getBooleanExtra("loud", false) ?: false,
            intent?.getStringExtra("sound") ?: "",
        )
    }
}
