package dev.lorehaven.daybook

import android.Manifest
import android.app.Activity
import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import android.util.Log
import dev.quench.Native
import org.json.JSONArray
import org.json.JSONObject

/**
 * Reminders and notifications. A reminder is stored (so it survives a reboot), armed with the
 * alarm manager (exact when the app may), and posted when it fires; a repeating one is a clock time
 * in a named time zone ("08:00 in Europe/Warsaw"), and its next occurrence is worked out by the
 * native code (`tz.next`, quench-tz) each time, so daylight saving never shifts it.
 *
 * Methods: `channel {id, name, importance, description}`, `show {id, title, body, channel}`,
 * `schedule {id, at_ms, title, body, channel, repeat_zone, repeat_time}`, `cancel {id}`, `pending`,
 * `permission` (asks the person if needed).
 */
class NotificationsPlugin(private val activity: Activity) : Plugin("notifications") {
    private val context = activity.applicationContext

    override fun call(method: String, args: JSONObject, call: Call) {
        when (method) {
            "channel" -> {
                Reminders.channel(context, args.getString("id"), args.getString("name"), args.optInt("importance", 3), args.optString("description"))
                call.resolve()
            }
            "show" -> {
                val posted = Reminders.post(context, args)
                call.resolve(JSONObject().put("posted", posted))
            }
            "schedule" -> {
                val reminder = JSONObject(args.toString())
                if (reminder.optLong("at_ms", 0) == 0L) {
                    if (reminder.repeats()) reminder.put("at_ms", Reminders.next(context, reminder, System.currentTimeMillis()))
                    else return call.reject("invalid_args", "`at_ms` is needed unless the reminder repeats")
                }
                if (reminder.repeats() && !Native.available(context)) {
                    return call.reject("native_unavailable", "a repeating reminder needs the native program (`quench plugin add native`)")
                }
                Reminders.save(context, reminder)
                call.resolve(JSONObject().put("exact", Reminders.arm(context, reminder)).put("at_ms", reminder.getLong("at_ms")))
            }
            "cancel" -> { Reminders.cancel(context, args.getInt("id")); call.resolve() }
            "pending" -> {
                val all = Reminders.all(context)
                call.resolve(JSONArray().also { list -> all.keys().forEach { list.put(all.getJSONObject(it)) } })
            }
            "permission" -> call.permissions(Manifest.permission.POST_NOTIFICATIONS) { granted ->
                call.resolve(JSONObject().put("granted", granted))
            }
            else -> call.reject("unknown_method", "notifications has no method `$method`")
        }
    }
}

/** A reminder repeats when it names both a zone and a clock time (a typed client sends `null` for those it leaves out). */
private fun JSONObject.repeats() = !isNull("repeat_zone") && !isNull("repeat_time")

/** Storing, arming and posting reminders. Used by the plugin and by the receivers, with no page. */
object Reminders {
    private const val PREFS = "quench-reminders"
    private const val DEFAULT_CHANNEL = "reminders"
    private const val TAG = "quench-reminders"

    fun all(context: Context): JSONObject =
        JSONObject(context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString("all", "{}") ?: "{}")

    private fun write(context: Context, all: JSONObject) =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString("all", all.toString()).apply()

    fun save(context: Context, reminder: JSONObject) = write(context, all(context).put(reminder.getInt("id").toString(), reminder))

    fun channel(context: Context, id: String, name: String, importance: Int, description: String) {
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(id, name, importance).also { it.description = description })
    }

    /** The next time a repeating reminder is due after [after], by the native time-zone code. */
    fun next(context: Context, reminder: JSONObject, after: Long): Long {
        val input = JSONObject().put("zone", reminder.getString("repeat_zone")).put("time", reminder.getString("repeat_time")).put("after_ms", after)
        return JSONObject(String(Native.invoke(context, "tz.next", input.toString().toByteArray()))).getLong("at_ms")
    }

    private fun pending(context: Context, id: Int) = PendingIntent.getBroadcast(
        context, id, Intent(context, ReminderReceiver::class.java).putExtra("id", id),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    )

    /** Sets the alarm; true when it is exact (the app may schedule exact alarms). */
    fun arm(context: Context, reminder: JSONObject): Boolean {
        val alarms = context.getSystemService(AlarmManager::class.java)
        val at = reminder.getLong("at_ms")
        val intent = pending(context, reminder.getInt("id"))
        return if (Build.VERSION.SDK_INT < 31 || alarms.canScheduleExactAlarms()) {
            alarms.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, intent)
            true
        } else {
            alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, intent)
            false
        }
    }

    fun cancel(context: Context, id: Int) {
        context.getSystemService(AlarmManager::class.java).cancel(pending(context, id))
        write(context, all(context).also { it.remove(id.toString()) })
    }

    fun post(context: Context, args: JSONObject): Boolean {
        val manager = context.getSystemService(NotificationManager::class.java)
        val channel = args.optString("channel", DEFAULT_CHANNEL)
        if (manager.getNotificationChannel(channel) == null) channel(context, channel, "Reminders", NotificationManager.IMPORTANCE_DEFAULT, "")
        if (!manager.areNotificationsEnabled()) return false
        val open = PendingIntent.getActivity(
            context, 0, context.packageManager.getLaunchIntentForPackage(context.packageName),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )
        manager.notify(
            args.optInt("id"),
            android.app.Notification.Builder(context, channel)
                .setSmallIcon(android.R.drawable.ic_dialog_info)
                .setContentTitle(args.optString("title"))
                .setContentText(args.optString("body"))
                .setContentIntent(open)
                .setAutoCancel(true)
                .build()
        )
        return true
    }

    /** A reminder is due: post it, then drop it or arm the next occurrence. */
    fun fire(context: Context, id: Int) {
        val reminder = all(context).optJSONObject(id.toString()) ?: return
        post(context, reminder)
        Log.i(TAG, "fired $id: ${reminder.optString("title")}")
        if (reminder.repeats() && Native.available(context)) {
            reminder.put("at_ms", next(context, reminder, System.currentTimeMillis()))
            save(context, reminder)
            arm(context, reminder)
        } else {
            write(context, all(context).also { it.remove(id.toString()) })
        }
    }

    /** After a reboot (the alarms are gone) or a change of the clock or zone: arm everything again. */
    fun rearm(context: Context) {
        val all = all(context)
        val now = System.currentTimeMillis()
        all.keys().forEach { key ->
            val reminder = all.getJSONObject(key)
            when {
                reminder.repeats() && Native.available(context) -> {
                    reminder.put("at_ms", next(context, reminder, now))
                    save(context, reminder)
                    arm(context, reminder)
                }
                reminder.getLong("at_ms") <= now -> fire(context, reminder.getInt("id")) // missed while off
                else -> arm(context, reminder)
            }
        }
        Log.i(TAG, "re-armed ${all.length()} reminders")
    }
}

class ReminderReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        Reminders.fire(context.applicationContext, intent.getIntExtra("id", -1))
    }
}

/** Boot, and changes of the time or the zone, which invalidate the alarms. */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        Reminders.rearm(context.applicationContext)
    }
}