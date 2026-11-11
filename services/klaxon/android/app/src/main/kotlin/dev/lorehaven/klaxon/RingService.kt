package dev.lorehaven.klaxon

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaPlayer
import android.media.RingtoneManager
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.util.Log
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * An alarm that is ringing: a foreground service that plays the alarm sound and vibrates until the person snoozes or
 * dismisses it (from [RingActivity] or the notification), or [LIMIT] has passed. The notification carries the
 * full-screen intent that opens [RingActivity] over the lock screen.
 */
class RingService : Service() {
    private var player: MediaPlayer? = null
    private var focus: AudioFocusRequest? = null
    private val handler = Handler(Looper.getMainLooper())
    private var ring = Ring(0, "", 10, true, false, "")

    /** The alarm volume before an extra loud alarm raised it, or -1 when nothing was changed. */
    private var savedVolume = -1
    private val giveUp = Runnable { missed() }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            SNOOZE -> {
                Klaxon.snooze(this, ring)
                finish()
            }
            DISMISS -> finish()
            else -> {
                ring = Ring.of(intent)
                ring()
            }
        }
        return START_NOT_STICKY
    }

    private fun time(): String = SimpleDateFormat("HH:mm", Locale.getDefault()).format(Date())

    private fun ring() {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, "Ringing alarm", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "An alarm that is ringing now"
                // The service plays the sound itself, on the alarm volume.
                setSound(null, null)
                enableVibration(false)
                lockscreenVisibility = Notification.VISIBILITY_PUBLIC
            },
        )
        val notification = notification(time())
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(RINGING, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
        } else {
            startForeground(RINGING, notification)
        }
        play()
        if (ring.vibrate) vibrate()
        handler.removeCallbacks(giveUp)
        handler.postDelayed(giveUp, LIMIT)
    }

    private fun screen(): PendingIntent = PendingIntent.getActivity(
        this, 1,
        Intent(this, RingActivity::class.java)
            .putExtra("label", ring.label).putExtra("snooze", ring.snooze)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    private fun action(name: String, text: String, code: Int): Notification.Action {
        val pending = PendingIntent.getService(
            this, code, Intent(this, RingService::class.java).setAction(name),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        return Notification.Action.Builder(null, text, pending).build()
    }

    private fun notification(at: String): Notification =
        Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat_alarm)
            .setContentTitle(if (ring.label.isEmpty()) "Alarm" else ring.label)
            .setContentText(at)
            .setCategory(Notification.CATEGORY_ALARM)
            .setOngoing(true)
            .setContentIntent(screen())
            .setFullScreenIntent(screen(), true)
            .addAction(action(SNOOZE, "Snooze ${ring.snooze} min", 2))
            .addAction(action(DISMISS, "Dismiss", 3))
            .build()

    private fun play() {
        stopSound()
        try {
            raiseVolume()
            val attributes = AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_ALARM)
                .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                .build()
            // An alarm takes the audio from whatever plays (music pauses) and gives it back when it ends.
            focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_EXCLUSIVE)
                .setAudioAttributes(attributes)
                .build()
                .also { getSystemService(AudioManager::class.java)?.requestAudioFocus(it) }
            player = MediaPlayer().apply {
                try {
                    load(ring.sound)
                } catch (e: Exception) {
                    // The chosen sound is gone (deleted, or its storage is): the default still wakes the person.
                    Log.w(Klaxon.TAG, "could not use the sound `${ring.sound}`: ${e.message}")
                    load("")
                }
                isLooping = true
                start()
            }
        } catch (e: Exception) {
            Log.w(Klaxon.TAG, "could not play the alarm sound: ${e.message}")
        }
    }

    /** Points the player at the alarm's own sound (or, for an empty [uri], the phone's default alarm sound). */
    private fun MediaPlayer.load(uri: String) {
        reset()
        val source = if (uri.isNotEmpty()) Uri.parse(uri)
        else RingtoneManager.getActualDefaultRingtoneUri(this@RingService, RingtoneManager.TYPE_ALARM)
            ?: RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM)
            ?: RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE)
        setAudioAttributes(
            AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_ALARM)
                .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                .build(),
        )
        setDataSource(this@RingService, source)
        prepare()
    }

    /** Extra loud: the alarm stream goes to its top, and what it was is put back when the ringing ends. */
    private fun raiseVolume() {
        if (!ring.loud || savedVolume >= 0) return
        val audio = getSystemService(AudioManager::class.java) ?: return
        savedVolume = audio.getStreamVolume(AudioManager.STREAM_ALARM)
        audio.setStreamVolume(AudioManager.STREAM_ALARM, audio.getStreamMaxVolume(AudioManager.STREAM_ALARM), 0)
    }

    private fun restoreVolume() {
        if (savedVolume < 0) return
        getSystemService(AudioManager::class.java)?.setStreamVolume(AudioManager.STREAM_ALARM, savedVolume, 0)
        savedVolume = -1
    }

    private fun stopSound() {
        try {
            player?.stop()
        } catch (_: Exception) {
        }
        player?.release()
        player = null
        focus?.let { getSystemService(AudioManager::class.java)?.abandonAudioFocusRequest(it) }
        focus = null
        restoreVolume()
    }

    private fun vibrator(): Vibrator? =
        if (Build.VERSION.SDK_INT >= 31) getSystemService(VibratorManager::class.java)?.defaultVibrator
        else @Suppress("DEPRECATION") getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator

    private fun vibrate() {
        vibrator()?.vibrate(
            VibrationEffect.createWaveform(longArrayOf(0, 700, 500), 0),
            AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ALARM).build(),
        )
    }

    /** Nobody answered for [LIMIT]: stop, and leave a note that it was missed. */
    private fun missed() {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(MISSED_CHANNEL, "Missed alarms", NotificationManager.IMPORTANCE_DEFAULT))
        manager.notify(
            MISSED,
            Notification.Builder(this, MISSED_CHANNEL)
                .setSmallIcon(R.drawable.ic_stat_alarm)
                .setContentTitle("Missed alarm")
                .setContentText(if (ring.label.isEmpty()) time() else "${ring.label} · ${time()}")
                .setContentIntent(PendingIntent.getActivity(this, 4, Intent(this, SurfaceActivity::class.java), PendingIntent.FLAG_IMMUTABLE))
                .setAutoCancel(true)
                .build(),
        )
        finish()
    }

    private fun finish() {
        handler.removeCallbacks(giveUp)
        stopSound()
        vibrator()?.cancel()
        sendBroadcast(Intent(ENDED).setPackage(packageName))
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    override fun onDestroy() {
        handler.removeCallbacks(giveUp)
        stopSound()
        vibrator()?.cancel()
        super.onDestroy()
    }

    companion object {
        const val SNOOZE = "dev.lorehaven.klaxon.SNOOZE"
        const val DISMISS = "dev.lorehaven.klaxon.DISMISS"

        /** Broadcast when the ringing is over, so [RingActivity] closes. */
        const val ENDED = "dev.lorehaven.klaxon.ENDED"
        private const val CHANNEL = "ring"
        private const val MISSED_CHANNEL = "missed"
        private const val RINGING = 1
        private const val MISSED = 2
        private const val LIMIT = 10 * 60 * 1000L

        fun start(context: Context, ring: Ring) {
            context.startForegroundService(ring.into(Intent(context, RingService::class.java)))
        }

        fun send(context: Context, action: String) {
            context.startService(Intent(context, RingService::class.java).setAction(action))
        }
    }
}
