package dev.lorehaven.songbook

import android.bluetooth.BluetoothClass
import android.bluetooth.BluetoothDevice
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import android.util.Log
import android.view.KeyEvent

/**
 * A car's Bluetooth connects (the hands-free of a car, or its audio): the music goes on where it was left. The playback service
 * is asked to play with the media button, which is how Media3 puts the player back as it was (`onPlaybackResumption`: the place
 * the service keeps, from any of the person's devices). Android Auto does the same by itself when it connects, if the person
 * asked it to start music; this is for the car that is only Bluetooth. Reading what kind of device it is needs the Bluetooth
 * permission (Android 12 and later); without it nothing is done.
 */
class CarConnectReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != BluetoothDevice.ACTION_ACL_CONNECTED) return
        val device = if (Build.VERSION.SDK_INT >= 33) {
            intent.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE, BluetoothDevice::class.java)
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE)
        } ?: return
        val isCar = try {
            device.bluetoothClass?.deviceClass in CAR_CLASSES
        } catch (e: SecurityException) {
            false
        }
        if (!isCar || !Session.of(context).signedIn) return
        Log.i(TAG, "a car connected: playing again")
        play(context)
    }

    companion object {
        private const val TAG = "SongbookCar"

        private val CAR_CLASSES = setOf(BluetoothClass.Device.AUDIO_VIDEO_CAR_AUDIO, BluetoothClass.Device.AUDIO_VIDEO_HANDSFREE)

        /** Tells the playback service to play, as a headset's button does. */
        fun play(context: Context) {
            val app = context.applicationContext
            val press = Intent(app, PlaybackService::class.java)
                .setAction(Intent.ACTION_MEDIA_BUTTON)
                .putExtra(Intent.EXTRA_KEY_EVENT, KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_MEDIA_PLAY))
            try {
                app.startForegroundService(press)
            } catch (e: Exception) {
                // The system may refuse to start a service from the background: then the person presses play.
                Log.w(TAG, "could not start the service: $e")
            }
        }
    }
}
