package dev.lorehaven.hoist

import android.app.Activity
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/**
 * Installing and updating an app from an APK the app has downloaded (the way a store client does).
 * It needs the person's permission to install from this app (`can_install`, `open_install_settings`)
 * and the system asks them to confirm each install. Methods: `can_install`, `open_install_settings`,
 * `installed {package}`, `list` (the apps on the device that have a screen of their own: `{apps: [{package, label,
 * version_name, version_code, system}]}`), `launch {package}` (opens one), `install_from_url {url, headers}` (events `download {sent, total}`), and
 * `install {uri}` for an APK the person picked. The outcome arrives as an `install {status, package,
 * message}` event, `status` being `success`, `failure`, `cancelled` or `pending`.
 *
 * Installing apps outside a store is restricted by Google Play's policy: an app distributed through
 * Play may not do this unless it is a store itself. See docs/quench-android.md.
 */
class PackagesPlugin(private val activity: Activity) : Plugin("packages") {
    private val context = activity.applicationContext

    override fun call(method: String, args: JSONObject, call: Call) {
        when (method) {
            "can_install" -> call.resolve(JSONObject().put("allowed", context.packageManager.canRequestPackageInstalls()))
            "open_install_settings" -> {
                activity.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${context.packageName}")))
                call.resolve()
            }
            "installed" -> call.resolve(installed(args.getString("package")))
            "list" -> call.resolve(list())
            "launch" -> call.resolve(JSONObject().put("launched", launch(args.getString("package"))))
            "install_from_url" -> installFromUrl(args, call)
            "install" -> {
                call.resolve()
                Thread { install(Uri.parse(args.getString("uri"))) }.start()
            }
            else -> call.reject("unknown_method", "packages has no method `$method`")
        }
    }

    init {
        Packages.listener = { emit("install", it) }
    }

    private fun installed(name: String): JSONObject = try {
        val info = context.packageManager.getPackageInfo(name, 0)
        JSONObject().put("installed", true).put("version_name", info.versionName).put("version_code", info.longVersionCode)
    } catch (_: PackageManager.NameNotFoundException) {
        JSONObject().put("installed", false)
    }

    private fun list(): JSONObject {
        val manager = context.packageManager
        val apps = JSONArray()
        for (info in manager.getInstalledPackages(0)) {
            val app = info.applicationInfo ?: continue
            // What cannot be opened is not an app to the person.
            if (manager.getLaunchIntentForPackage(info.packageName) == null) continue
            val system = app.flags and ApplicationInfo.FLAG_SYSTEM != 0 && app.flags and ApplicationInfo.FLAG_UPDATED_SYSTEM_APP == 0
            apps.put(
                JSONObject().put("package", info.packageName).put("label", manager.getApplicationLabel(app).toString())
                    .put("version_name", info.versionName ?: "").put("version_code", info.longVersionCode).put("system", system)
            )
        }
        return JSONObject().put("apps", apps)
    }

    private fun launch(name: String): Boolean {
        val intent = context.packageManager.getLaunchIntentForPackage(name) ?: return false
        activity.startActivity(intent)
        return true
    }

    private fun installFromUrl(args: JSONObject, call: Call) {
        if (!context.packageManager.canRequestPackageInstalls()) {
            return call.reject("permission_denied", "this app may not install apps: open_install_settings")
        }
        call.resolve()
        Thread {
            try {
                val file = File(context.cacheDir, "download.apk")
                val connection = URL(args.getString("url")).openConnection() as HttpURLConnection
                args.optJSONObject("headers")?.let { h -> h.keys().forEach { connection.setRequestProperty(it, h.getString(it)) } }
                val total = connection.contentLengthLong
                var sent = 0L
                var last = 0L
                connection.inputStream.use { input ->
                    file.outputStream().use { output ->
                        val buffer = ByteArray(64 * 1024)
                        while (true) {
                            val n = input.read(buffer)
                            if (n < 0) break
                            output.write(buffer, 0, n)
                            sent += n
                            if (System.currentTimeMillis() - last >= 200) {
                                last = System.currentTimeMillis()
                                emit("download", JSONObject().put("sent", sent).put("total", total))
                            }
                        }
                    }
                }
                emit("download", JSONObject().put("sent", sent).put("total", total))
                install(Uri.fromFile(file))
            } catch (e: Exception) {
                emit("install", JSONObject().put("status", "failure").put("message", e.message ?: "download failed"))
            }
        }.start()
    }

    private fun install(apk: Uri) {
        try {
            val installer = context.packageManager.packageInstaller
            val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
            val id = installer.createSession(params)
            installer.openSession(id).use { session ->
                (context.contentResolver.openInputStream(apk) ?: throw java.io.FileNotFoundException(apk.toString())).use { input ->
                    session.openWrite("package", 0, -1).use { out ->
                        input.copyTo(out)
                        session.fsync(out)
                    }
                }
                val result = PendingIntent.getBroadcast(
                    context, id, Intent(Packages.ACTION).setPackage(context.packageName),
                    PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
                )
                session.commit(result.intentSender)
            }
        } catch (e: Exception) {
            Packages.listener?.invoke(JSONObject().put("status", "failure").put("message", e.message ?: "install failed"))
        }
    }
}

object Packages {
    const val ACTION = "dev.lorehaven.hoist.INSTALL_RESULT"
    var listener: ((JSONObject) -> Unit)? = null
}

/** The installer's answers: it asks to be allowed to show its confirmation, then reports the outcome. */
class InstallReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
        val message = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE) ?: ""
        if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
            val confirm = if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
            else @Suppress("DEPRECATION") intent.getParcelableExtra(Intent.EXTRA_INTENT)
            confirm?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)?.let { context.startActivity(it) }
            Packages.listener?.invoke(JSONObject().put("status", "pending").put("message", message))
            return
        }
        Packages.listener?.invoke(
            JSONObject()
                .put("status", when (status) { PackageInstaller.STATUS_SUCCESS -> "success"; PackageInstaller.STATUS_FAILURE_ABORTED -> "cancelled"; else -> "failure" })
                .put("package", intent.getStringExtra(PackageInstaller.EXTRA_PACKAGE_NAME) ?: "")
                .put("message", message)
        )
    }
}