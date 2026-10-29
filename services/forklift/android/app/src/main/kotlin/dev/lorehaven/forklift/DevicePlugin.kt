package dev.lorehaven.forklift

import android.app.Activity
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.os.BatteryManager
import android.os.PowerManager
import org.json.JSONObject

/**
 * What the device is doing, for an app that decides when to work: the network (is there one, what
 * kind, is it metered), the battery and power saving. `state` answers once; `watch` pushes a `state`
 * event whenever any of it changes, until `unwatch` or the page goes.
 */
class DevicePlugin(private val activity: Activity) : Plugin("device") {
    private val context = activity.applicationContext
    private var callback: ConnectivityManager.NetworkCallback? = null
    private var battery: BroadcastReceiver? = null
    private var last: String? = null

    override fun call(method: String, args: JSONObject, call: Call) {
        when (method) {
            "state" -> call.resolve(state())
            "watch" -> { watch(); call.resolve() }
            "unwatch" -> { unwatch(); call.resolve() }
            else -> call.reject("unknown_method", "device has no method `$method`")
        }
    }

    override fun disconnected() = unwatch()

    private fun state(): JSONObject {
        val cm = context.getSystemService(ConnectivityManager::class.java)
        val caps = cm.getNetworkCapabilities(cm.activeNetwork)
        val sticky = context.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        val level = sticky?.getIntExtra(BatteryManager.EXTRA_LEVEL, -1) ?: -1
        val scale = sticky?.getIntExtra(BatteryManager.EXTRA_SCALE, 100) ?: 100
        val percent = if (level >= 0) level * 100 / scale else -1
        val charging = (sticky?.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) ?: 0) != 0
        return JSONObject()
            .put("online", caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) == true)
            .put(
                "transport",
                when {
                    caps == null -> "none"
                    caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "wifi"
                    caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "cellular"
                    caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "ethernet"
                    else -> "other"
                }
            )
            .put("metered", cm.isActiveNetworkMetered)
            .put("battery_percent", percent)
            .put("charging", charging)
            .put("battery_low", percent in 0..15 && !charging)
            .put("power_save", context.getSystemService(PowerManager::class.java).isPowerSaveMode)
    }

    private fun watch() {
        unwatch()
        // Several callbacks report one change: only a different state is news.
        val push = {
            val now = state()
            if (now.toString() != last) {
                last = now.toString()
                emit("state", now)
            }
        }
        val cm = context.getSystemService(ConnectivityManager::class.java)
        callback = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) = push()
            override fun onLost(network: Network) = push()
            override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) = push()
        }.also { cm.registerDefaultNetworkCallback(it) }
        battery = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) = push()
        }.also {
            val filter = IntentFilter(Intent.ACTION_BATTERY_CHANGED).apply {
                addAction(PowerManager.ACTION_POWER_SAVE_MODE_CHANGED)
            }
            context.registerReceiver(it, filter)
        }
    }

    private fun unwatch() {
        callback?.let { context.getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(it) }
        battery?.let { context.unregisterReceiver(it) }
        callback = null
        battery = null
        last = null
    }
}