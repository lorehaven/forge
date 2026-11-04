package dev.lorehaven.daybook

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build

/**
 * "You are signed out": said when the app, with no window open, finds that its session is over (the refresh token was refused),
 * so that the person knows to sign in again. Said once: until the app is opened again nothing more is posted, however many times
 * the background work finds the same. Tapping it opens the app. The four apps (Daybook, Forklift, Hoist, Songbook) do this the same way.
 */
object SignedOutNotice {
    private const val CHANNEL = "signed-out"
    private const val ID = 7001
    private const val PREFS = "signed-out-notice"
    private const val SAID = "said"

    /** What the notice says under its title. */
    private const val TEXT = "Open Daybook and sign in to see your tasks and get your reminders."

    fun show(context: Context) {
        val app = context.applicationContext
        val prefs = app.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        if (prefs.getBoolean(SAID, false)) return
        if (Build.VERSION.SDK_INT >= 33 && app.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        val manager = app.getSystemService(NotificationManager::class.java) ?: return
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, "Sign-in", NotificationManager.IMPORTANCE_DEFAULT).apply {
                description = "Tells you when you have to sign in again"
            },
        )
        val open = PendingIntent.getActivity(
            app,
            0,
            app.packageManager.getLaunchIntentForPackage(app.packageName)?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        manager.notify(
            ID,
            Notification.Builder(app, CHANNEL)
                .setSmallIcon(android.R.drawable.stat_sys_warning)
                .setContentTitle("You are signed out")
                .setContentText(TEXT)
                .setStyle(Notification.BigTextStyle().bigText(TEXT))
                .setContentIntent(open)
                .setAutoCancel(true)
                .build(),
        )
        prefs.edit().putBoolean(SAID, true).apply()
    }

    /** The app was opened: the notice is taken back, and a later sign-out is said again. */
    fun clear(context: Context) {
        val app = context.applicationContext
        app.getSystemService(NotificationManager::class.java)?.cancel(ID)
        app.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(SAID, false).apply()
    }
}
