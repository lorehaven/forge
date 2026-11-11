package dev.lorehaven.klaxon

import android.Manifest
import android.app.Activity
import android.app.NotificationManager
import android.content.Intent
import android.content.pm.PackageManager
import android.media.RingtoneManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import org.json.JSONObject

/**
 * The program's way to the system's alarm clock: [schedule] sets the next ring from what the program worked out,
 * [status] says whether an alarm can show over the lock screen, and [allow] asks for what is missing.
 */
class AlarmsPlugin(private val activity: Activity) : Plugin("alarms") {
    override fun call(method: String, args: JSONObject, call: Call) {
        when (method) {
            "schedule" -> {
                Klaxon.schedule(activity, args)
                call.resolve(true)
            }
            "status" -> call.resolve(JSONObject().put("alerts", alertsAllowed()))
            "allow" -> allow(call)
            "pick_sound" -> pickSound(args.optString("current", ""), call)
            else -> call.reject("unknown_method", "alarms has no method `$method`")
        }
    }

    private fun notificationsAllowed(): Boolean =
        Build.VERSION.SDK_INT < 33 ||
            activity.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    private fun fullScreenAllowed(): Boolean =
        Build.VERSION.SDK_INT < 34 ||
            activity.getSystemService(NotificationManager::class.java).canUseFullScreenIntent()

    private fun alertsAllowed() = notificationsAllowed() && fullScreenAllowed()

    private fun allow(call: Call) {
        if (!notificationsAllowed()) {
            call.permissions(Manifest.permission.POST_NOTIFICATIONS) { granted ->
                if (granted && !fullScreenAllowed()) openFullScreenSettings()
                call.resolve(granted)
            }
        } else {
            if (!fullScreenAllowed()) openFullScreenSettings()
            call.resolve(alertsAllowed())
        }
    }

    /** The call waiting for the system's sound picker. */
    private var picking: Call? = null

    /** Opens the system's alarm sound picker on [current] (empty: the default); answers `{uri, name}` or `{cancelled}`. */
    private fun pickSound(current: String, call: Call) {
        picking?.resolve(JSONObject().put("cancelled", true))
        picking = call
        val existing = if (current.isEmpty()) Settings.System.DEFAULT_ALARM_ALERT_URI else Uri.parse(current)
        val intent = Intent(RingtoneManager.ACTION_RINGTONE_PICKER)
            .putExtra(RingtoneManager.EXTRA_RINGTONE_TYPE, RingtoneManager.TYPE_ALARM)
            .putExtra(RingtoneManager.EXTRA_RINGTONE_TITLE, "Alarm sound")
            .putExtra(RingtoneManager.EXTRA_RINGTONE_SHOW_DEFAULT, true)
            .putExtra(RingtoneManager.EXTRA_RINGTONE_SHOW_SILENT, false)
            .putExtra(RingtoneManager.EXTRA_RINGTONE_DEFAULT_URI, Settings.System.DEFAULT_ALARM_ALERT_URI)
            .putExtra(RingtoneManager.EXTRA_RINGTONE_EXISTING_URI, existing)
        try {
            activity.startActivityForResult(intent, PICK_SOUND)
        } catch (e: Exception) {
            picking = null
            call.reject("no_picker", "this phone has no sound picker")
        }
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        if (requestCode != PICK_SOUND) return
        val call = picking ?: return
        picking = null
        @Suppress("DEPRECATION")
        val uri: Uri? = data?.getParcelableExtra(RingtoneManager.EXTRA_RINGTONE_PICKED_URI)
        if (resultCode != Activity.RESULT_OK || data == null) {
            call.resolve(JSONObject().put("cancelled", true))
            return
        }
        // The default (or "none", which cannot be chosen here) is kept as an empty URI: it follows the phone's setting.
        if (uri == null || uri == Settings.System.DEFAULT_ALARM_ALERT_URI) {
            call.resolve(JSONObject().put("uri", "").put("name", ""))
            return
        }
        val name = try {
            RingtoneManager.getRingtone(activity, uri)?.getTitle(activity) ?: ""
        } catch (e: Exception) {
            ""
        }
        call.resolve(JSONObject().put("uri", uri.toString()).put("name", name))
    }

    private fun openFullScreenSettings() {
        if (Build.VERSION.SDK_INT >= 34) {
            activity.startActivity(
                Intent(Settings.ACTION_MANAGE_APP_USE_FULL_SCREEN_INTENT, Uri.parse("package:${activity.packageName}")),
            )
        }
    }

    private companion object {
        const val PICK_SOUND = 4100
    }
}
