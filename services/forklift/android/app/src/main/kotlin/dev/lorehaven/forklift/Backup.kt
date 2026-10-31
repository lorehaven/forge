package dev.lorehaven.forklift

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.ContentResolver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.Uri
import android.os.BatteryManager
import android.os.Build
import android.os.Bundle
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit

/**
 * The backup while the window is closed. A job ([BackupJobService]) runs the app's program in its `--backup` mode and
 * does the device's side of what it asks over JSON lines: the media library, the folders the person picked, the network
 * and the battery, and streaming a file to the server. All of the deciding (what is new, what goes first, what waits
 * for Wi-Fi or the charger, what a refusal means) is the program's, the same code the window runs.
 *
 * One program at a time sends files: the window when it is open ([foreground]), else a job. The program also holds a
 * lock file, so a late job cannot run beside the window.
 */
object Backup {
    const val TAG = "forklift-backup"
    private const val PROGRAM = "libquench_surface.so"
    private const val PERIODIC = 4200
    private const val OBSERVER = 4201
    private const val ONCE = 4202
    private const val CHANNEL = "backup"
    private const val ONGOING = 1

    /** The id of the notification of the foreground service. */
    const val FOREGROUND = 3
    private const val RESULT = 2
    private const val STATUS = 4
    private const val CHANNEL_STATUS = "status"
    private val EVERY = TimeUnit.MINUTES.toMillis(15)

    /** True while the window is up: a job that starts then leaves the backup to it. */
    @Volatile
    var foreground = false

    @Volatile
    private var current: Process? = null

    private val running = java.util.concurrent.atomic.AtomicBoolean(false)

    /** `files/.local/share/forklift`: where the program keeps its things (it is started with `HOME` as `files`). */
    private fun dataDir(context: Context) = File(context.filesDir, ".local/share/forklift")

    /** Whether the person wants automatic backups and is signed in. The program decides the rest. */
    fun enabled(context: Context): Boolean {
        val dir = dataDir(context)
        if (!File(dir, "session").exists()) return false
        val settings = File(dir, "settings")
        if (!settings.exists()) return false
        // Switched on, and a storage chosen: nothing is sent before the person has said where it goes.
        val lines = settings.readLines()
        return lines.any { it == "auto=1" } && lines.any { it == "storage_confirmed=1" }
    }

    /**
     * The quiet notification that says the backup is looking after things: when it last looked and how many files wait.
     * It is there while automatic backup is on and someone is signed in, and gone otherwise. It is posted after every run,
     * and whenever the window leaves the screen; it is only ever as old as the last look.
     */
    fun updateStatus(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        if (!enabled(context)) {
            manager.cancel(STATUS)
            return
        }
        if (!canNotify(context)) return
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_STATUS, "Backup status", NotificationManager.IMPORTANCE_MIN).apply {
                description = "When the backup last looked for new files, and how many wait"
                setShowBadge(false)
            },
        )
        val dir = dataDir(context)
        val checked = File(dir, "checked").takeIf { it.exists() }?.readText()?.trim()?.toLongOrNull()
        val (sending, failed) = counts(context)
        val waiting = sending + failed
        val time = checked?.let { java.text.SimpleDateFormat("HH:mm", java.util.Locale.getDefault()).format(java.util.Date(it)) }
        val text = when {
            time == null -> "Waiting for the first look"
            waiting > 0 -> "Last checked $time \u00b7 $waiting waiting"
            else -> "Last checked $time \u00b7 everything is backed up"
        }
        manager.notify(
            STATUS,
            Notification.Builder(context, CHANNEL_STATUS)
                .setSmallIcon(R.drawable.ic_stat_backup)
                .setContentTitle("Forklift")
                .setContentText(text)
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .setShowWhen(false)
                .setContentIntent(open(context))
                // Swiped away: put back (see BackupReceiver).
                .setDeleteIntent(
                    PendingIntent.getBroadcast(
                        context, 2,
                        Intent(context, BackupReceiver::class.java).setAction(BackupReceiver.STATUS_DISMISSED),
                        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                    ),
                )
                .build(),
        )
    }

    /** Schedules the periodic catch-up (once) and the wait for the next new photo or video. */
    fun schedule(context: Context) {
        updateStatus(context)
        try {
            val scheduler = context.getSystemService(JobScheduler::class.java) ?: return
            if (scheduler.getPendingJob(PERIODIC) == null) {
                val job = JobInfo.Builder(PERIODIC, ComponentName(context, BackupJobService::class.java))
                    .setPeriodic(EVERY)
                    .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
                    .setPersisted(true)
                    .build()
                scheduler.schedule(job)
            }
            arm(context)
        } catch (e: Exception) {
            Log.w(TAG, "could not schedule the backup: ${e.message}")
        }
    }

    /** Waits for a new photo or video: when one appears (and the network is there) the job runs. One-shot: arm again. */
    fun arm(context: Context) {
        try {
            val scheduler = context.getSystemService(JobScheduler::class.java) ?: return
            if (scheduler.getPendingJob(OBSERVER) != null || !enabled(context)) return
            val job = JobInfo.Builder(OBSERVER, ComponentName(context, BackupJobService::class.java))
                .addTriggerContentUri(JobInfo.TriggerContentUri(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, JobInfo.TriggerContentUri.FLAG_NOTIFY_FOR_DESCENDANTS))
                .addTriggerContentUri(JobInfo.TriggerContentUri(MediaStore.Video.Media.EXTERNAL_CONTENT_URI, JobInfo.TriggerContentUri.FLAG_NOTIFY_FOR_DESCENDANTS))
                .setTriggerContentUpdateDelay(10_000)
                .setTriggerContentMaxDelay(120_000)
                .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
                .build()
            scheduler.schedule(job)
        } catch (e: Exception) {
            Log.w(TAG, "could not wait for new media: ${e.message}")
        }
    }

    /** The window's program watches this file: while it exists the window is in the background. */
    private fun hiddenMarker(context: Context) = File(dataDir(context), "hidden")

    /** The window is shown: whatever a job is doing stops, and the window takes over. */
    fun windowOpened(context: Context) {
        foreground = true
        hiddenMarker(context).delete()
        requestStop()
    }

    /**
     * How many files wait to be sent (pending or uploading) and how many failed: from `manifest.counts`, three lines
     * of text the program writes with every save of its manifest (the manifest itself is a database only the program
     * opens). Until the program has saved once after an upgrade, the old text manifest is read the old way.
     */
    private fun counts(context: Context): Pair<Int, Int> {
        val dir = dataDir(context)
        var sending = 0
        var failed = 0
        val summary = File(dir, "manifest.counts")
        if (summary.exists()) {
            summary.useLines { lines ->
                for (line in lines) {
                    val parts = line.split(' ')
                    val n = parts.getOrNull(1)?.toIntOrNull() ?: continue
                    when (parts[0]) {
                        "pending", "uploading" -> sending += n
                        "failed" -> failed += n
                    }
                }
            }
            return sending to failed
        }
        File(dir, "manifest").takeIf { it.exists() }?.useLines { lines ->
            for (line in lines.drop(1)) {
                when (line.split('\t').getOrNull(6)) {
                    "pending", "uploading" -> sending++
                    "failed" -> failed++
                }
            }
        }
        return sending to failed
    }

    /** Whether files are waiting to be sent. */
    fun workWaiting(context: Context): Boolean = counts(context).first > 0

    /**
     * The window is leaving the screen: with files still waiting, a foreground service takes the backup on (it has no
     * time limit like a job, and shows what it does). Allowed now, while the app is still in front.
     */
    fun windowPaused(context: Context) {
        // Out of the foreground from here on: the jobs and the service no longer give way to it.
        foreground = false
        if (!enabled(context) || !workWaiting(context)) return
        try {
            context.startForegroundService(Intent(context, BackupService::class.java))
        } catch (e: Exception) {
            // The system did not let it start: the job that follows picks the backup up instead.
            Log.w(TAG, "could not start the foreground backup: ${e.message}")
        }
    }

    /** The window closed: a job picks up what is left, now. */
    fun windowClosed(context: Context) {
        foreground = false
        updateStatus(context)
        // The window's program stops sending and lets go of the backup (it looks at this every couple of seconds).
        try {
            dataDir(context).mkdirs()
            hiddenMarker(context).writeText("")
        } catch (_: Exception) {
        }
        if (!enabled(context)) return
        try {
            val scheduler = context.getSystemService(JobScheduler::class.java) ?: return
            scheduler.schedule(
                JobInfo.Builder(ONCE, ComponentName(context, BackupJobService::class.java))
                    .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
                    // A moment for the window's program to let go of the backup (it holds a lock until it ends).
                    .setMinimumLatency(3_000)
                    .build(),
            )
        } catch (e: Exception) {
            Log.w(TAG, "could not start the backup: ${e.message}")
        }
    }

    /** Tells a running program to stop (it puts what was in flight back to waiting and ends). */
    fun requestStop() {
        val process = current ?: return
        try {
            synchronized(process) { process.outputStream.write("{\"event\":\"stop\"}\n".toByteArray()); process.outputStream.flush() }
        } catch (_: Exception) {
        }
        Thread {
            if (!process.waitFor(5, TimeUnit.SECONDS)) process.destroy()
        }.start()
    }

    private fun canReadMedia(context: Context, kind: String): Boolean {
        val permission = if (Build.VERSION.SDK_INT >= 33) {
            if (kind == "video") Manifest.permission.READ_MEDIA_VIDEO else Manifest.permission.READ_MEDIA_IMAGES
        } else {
            Manifest.permission.READ_EXTERNAL_STORAGE
        }
        return context.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED
    }

    private fun media(context: Context, kind: String, offset: Int): JSONArray {
        if (!canReadMedia(context, kind)) throw SecurityException("the media library is not allowed")
        val uri = if (kind == "video") MediaStore.Video.Media.EXTERNAL_CONTENT_URI else MediaStore.Images.Media.EXTERNAL_CONTENT_URI
        val query = Bundle().apply {
            putInt(ContentResolver.QUERY_ARG_LIMIT, 500)
            putInt(ContentResolver.QUERY_ARG_OFFSET, offset)
            putStringArray(ContentResolver.QUERY_ARG_SORT_COLUMNS, arrayOf(MediaStore.MediaColumns.DATE_MODIFIED))
            putInt(ContentResolver.QUERY_ARG_SORT_DIRECTION, ContentResolver.QUERY_SORT_DIRECTION_DESCENDING)
        }
        val columns = arrayOf(
            MediaStore.MediaColumns._ID, MediaStore.MediaColumns.DISPLAY_NAME, MediaStore.MediaColumns.SIZE,
            MediaStore.MediaColumns.DATE_MODIFIED,
        )
        val list = JSONArray()
        context.contentResolver.query(uri, columns, query, null)?.use { c ->
            while (c.moveToNext()) {
                list.put(
                    JSONObject()
                        .put("uri", android.content.ContentUris.withAppendedId(uri, c.getLong(0)).toString())
                        .put("name", c.getString(1)).put("size", c.getLong(2)).put("modified_ms", c.getLong(3) * 1000),
                )
            }
        }
        return list
    }

    private fun folder(context: Context, uri: Uri): JSONArray {
        val docId = if (DocumentsContract.isDocumentUri(context, uri)) DocumentsContract.getDocumentId(uri) else DocumentsContract.getTreeDocumentId(uri)
        val tree = DocumentsContract.buildTreeDocumentUri(uri.authority, DocumentsContract.getTreeDocumentId(uri))
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, docId)
        val columns = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE, DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED,
        )
        val list = JSONArray()
        context.contentResolver.query(children, columns, null, null, null)?.use { c ->
            while (c.moveToNext()) {
                list.put(
                    JSONObject()
                        .put("uri", DocumentsContract.buildDocumentUriUsingTree(tree, c.getString(0)).toString())
                        .put("name", c.getString(1))
                        .put("is_dir", c.getString(2) == DocumentsContract.Document.MIME_TYPE_DIR)
                        .put("size", if (c.isNull(3)) -1 else c.getLong(3))
                        .put("modified_ms", if (c.isNull(4)) 0 else c.getLong(4)),
                )
            }
        }
        return list
    }

    private fun device(context: Context): JSONObject {
        val cm = context.getSystemService(ConnectivityManager::class.java)
        val caps = cm.getNetworkCapabilities(cm.activeNetwork)
        val sticky = context.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        val transport = when {
            caps == null -> "none"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "wifi"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "ethernet"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "cellular"
            else -> "other"
        }
        return JSONObject()
            .put("online", caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) == true)
            .put("transport", transport)
            .put("charging", (sticky?.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) ?: 0) != 0)
    }

    private val uploads = ConcurrentHashMap<String, HttpURLConnection>()

    /** Files the server took in this run. */
    private val sent = java.util.concurrent.atomic.AtomicInteger()

    /** Streams one file to the server; the end of it is an event (`done` with the status, or `failed`). */
    private fun upload(context: Context, call: JSONObject, send: (JSONObject) -> Unit) {
        val id = call.getString("upload")
        val uri = Uri.parse(call.getString("uri"))
        val url = URL(call.getString("url"))
        val token = call.getString("token")
        Thread {
            var connection: HttpURLConnection? = null
            try {
                val total = context.contentResolver.query(uri, arrayOf(android.provider.OpenableColumns.SIZE), null, null, null)?.use {
                    if (it.moveToFirst() && !it.isNull(0)) it.getLong(0) else -1L
                } ?: -1L
                connection = (url.openConnection() as HttpURLConnection).apply {
                    requestMethod = "PUT"
                    doOutput = true
                    setRequestProperty("Authorization", "Bearer $token")
                    setRequestProperty("Content-Type", "application/octet-stream")
                    connectTimeout = 15_000
                    readTimeout = 120_000
                    if (total >= 0) setFixedLengthStreamingMode(total) else setChunkedStreamingMode(64 * 1024)
                }
                uploads[id] = connection
                (context.contentResolver.openInputStream(uri) ?: throw java.io.FileNotFoundException(uri.toString())).use { input ->
                    connection.outputStream.use { output -> input.copyTo(output, 64 * 1024) }
                }
                val status = connection.responseCode
                if (status in 200..299) sent.incrementAndGet()
                send(JSONObject().put("event", "done").put("id", id).put("status", status))
            } catch (e: Exception) {
                send(JSONObject().put("event", "failed").put("id", id).put("message", e.message ?: e.javaClass.simpleName))
            } finally {
                uploads.remove(id)
                connection?.disconnect()
            }
        }.start()
    }

    /**
     * One background run, on the calling thread: starts the program and serves it until it says it has finished.
     * `onStatus` is told how it goes (the headline and how many files wait). Returns false when another program has the
     * backup (the window, or another run), so nothing was done.
     */
    fun run(context: Context, onStatus: (String, Int) -> Unit): Boolean {
        val program = File(context.applicationInfo.nativeLibraryDir, PROGRAM)
        if (!program.exists()) {
            Log.w(TAG, "the program is not in this build")
            return true
        }
        // One run at a time in this process (a job and the service could otherwise start two).
        if (!running.compareAndSet(false, true)) return false
        var busy = false
        val process = try {
            ProcessBuilder(program.absolutePath, "--backup")
                .directory(context.filesDir)
                .apply { environment()["HOME"] = context.filesDir.absolutePath }
                .start()
        } catch (e: Exception) {
            running.set(false)
            throw e
        }
        current = process
        val errors = Thread {
            try {
                process.errorStream.bufferedReader().forEachLine { Log.w(TAG, it) }
            } catch (_: java.io.IOException) {
                // The program was stopped while this was reading: that is the end of its messages, not a failure.
            }
        }.apply { start() }
        val out = process.outputStream
        val send: (JSONObject) -> Unit = { message ->
            try {
                synchronized(out) { out.write((message.toString() + "\n").toByteArray()); out.flush() }
            } catch (_: Exception) {
            }
        }
        sent.set(0)
        try {
            process.inputStream.bufferedReader().forEachLine { line ->
                val message = try { JSONObject(line) } catch (e: org.json.JSONException) { return@forEachLine }
                when {
                    message.has("call") -> {
                        val id = message.getLong("id")
                        val reply = { value: Any -> send(JSONObject().put("id", id).put("ok", value)) }
                        val fail = { why: String -> send(JSONObject().put("id", id).put("err", why)) }
                        try {
                            when (message.getString("call")) {
                                "media" -> reply(media(context, message.getString("kind"), message.optInt("offset")))
                                "folder" -> reply(folder(context, Uri.parse(message.getString("uri"))))
                                "device" -> reply(device(context))
                                "upload" -> { upload(context, message, send); reply(JSONObject()) }
                                else -> fail("unknown call")
                            }
                        } catch (e: Exception) {
                            fail(e.message ?: e.javaClass.simpleName)
                        }
                    }
                    message.has("status") -> {
                        val status = message.getJSONObject("status")
                        onStatus(status.optString("headline"), status.optInt("waiting"))
                    }
                    message.has("signed_out") -> notifyResult(context, "Forklift", "Sign in again to continue backing up")
                    message.optBoolean("finished") -> busy = message.has("skipped")
                }
            }
        } catch (_: java.io.IOException) {
            // The program was stopped (the person, the window, or the system): what was in flight waits again.
        } finally {
            running.set(false)
            current = null
            uploads.values.forEach { it.disconnect() }
            process.waitFor(5, TimeUnit.SECONDS)
            process.destroy()
            errors.join(1000)
            context.getSystemService(NotificationManager::class.java)?.cancel(ONGOING)
        }
        Log.i(TAG, "run ended; ${sent.get()} file(s) sent")
        updateStatus(context)
        if (sent.get() > 0) notifyResult(context, "Forklift", "Backed up ${sent.get()} new file${if (sent.get() == 1) "" else "s"}")
        return !busy
    }

    private fun channel(context: Context): NotificationManager? {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return null
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, "Backup", NotificationManager.IMPORTANCE_LOW).apply { description = "Progress of the backup" },
        )
        return manager
    }

    private fun canNotify(context: Context) =
        Build.VERSION.SDK_INT < 33 || context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    private fun open(context: Context) = PendingIntent.getActivity(
        context, 0,
        Intent(context, SurfaceActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    /** The ongoing notification of a run; with `stoppable` it has a Stop button (for the foreground service). */
    fun progressNotification(context: Context, headline: String, stoppable: Boolean): Notification {
        channel(context)
        val builder = Notification.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat_backup)
            .setContentTitle("Forklift")
            .setContentText(headline)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(open(context))
        if (stoppable) {
            val stop = PendingIntent.getService(
                context, 1,
                Intent(context, BackupService::class.java).setAction(BackupService.STOP),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
            builder.addAction(Notification.Action.Builder(null, "Stop", stop).build())
        }
        return builder.build()
    }

    /** Shows how a job's run goes: as the job's own notification where the system has that, else as a plain one. */
    fun showJobProgress(context: Context, service: JobService, params: JobParameters, headline: String) {
        if (!canNotify(context)) return
        val manager = channel(context) ?: return
        val notification = progressNotification(context, headline, false)
        if (Build.VERSION.SDK_INT >= 31) {
            service.setNotification(params, ONGOING, notification, JobService.JOB_END_NOTIFICATION_POLICY_REMOVE)
        } else {
            manager.notify(ONGOING, notification)
        }
    }

    private fun notifyResult(context: Context, title: String, text: String) {
        if (!canNotify(context)) return
        val manager = channel(context) ?: return
        manager.notify(
            RESULT,
            Notification.Builder(context, CHANNEL)
                .setSmallIcon(R.drawable.ic_stat_backup)
                .setContentTitle(title)
                .setContentText(text)
                .setContentIntent(open(context))
                .setAutoCancel(true)
                .build(),
        )
    }
}
