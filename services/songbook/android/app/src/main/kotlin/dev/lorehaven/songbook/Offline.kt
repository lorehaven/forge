package dev.lorehaven.songbook

import android.app.Notification
import android.content.Context
import android.net.Uri
import androidx.media3.database.StandaloneDatabaseProvider
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.datasource.ResolvingDataSource
import androidx.media3.datasource.cache.Cache
import androidx.media3.datasource.cache.CacheDataSource
import androidx.media3.datasource.cache.CacheWriter
import androidx.media3.datasource.cache.LeastRecentlyUsedCacheEvictor
import androidx.media3.datasource.cache.NoOpCacheEvictor
import androidx.media3.datasource.cache.SimpleCache
import androidx.media3.exoplayer.offline.Download
import androidx.media3.exoplayer.offline.DownloadManager
import androidx.media3.exoplayer.offline.DownloadNotificationHelper
import androidx.media3.exoplayer.offline.DownloadRequest
import androidx.media3.exoplayer.offline.DownloadService
import androidx.media3.exoplayer.scheduler.Scheduler
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.Executors

/**
 * Where the songs and pictures are read from, and the songs kept on the device.
 *
 * Three layers, in the order they are tried: the songs that were *kept* (never evicted), the songs that were played (the
 * oldest go when the space is full), and the network (with the token). A kept song plays with no network at all. The
 * download manager works in the background, with a notification, through [SongDownloadService].
 */
class Offline private constructor(private val context: Context) {
    private val session = Session.of(context)
    private val database = StandaloneDatabaseProvider(context)

    /** Every request to the service carries the token (renewed when it runs out). */
    val network = ResolvingDataSource.Factory(DefaultHttpDataSource.Factory().setConnectTimeoutMs(15_000).setAllowCrossProtocolRedirects(true)) { spec: DataSpec ->
        val token = try { session.token() } catch (e: java.io.IOException) { null }
        if (token != null) spec.withAdditionalHeaders(mapOf("Authorization" to "Bearer $token")) else spec
    }

    private val keptCache: Cache = SimpleCache(File(context.filesDir, "kept"), NoOpCacheEvictor(), database)
    private val playedCache: Cache = SimpleCache(File(context.cacheDir, "audio"), LeastRecentlyUsedCacheEvictor(PLAYED_BYTES), database)
    private val artCache: Cache = SimpleCache(File(context.cacheDir, "art"), LeastRecentlyUsedCacheEvictor(ART_BYTES), database)

    private val playedLayer = CacheDataSource.Factory().setCache(playedCache).setUpstreamDataSourceFactory(network).setFlags(CacheDataSource.FLAG_IGNORE_CACHE_ON_ERROR)

    /** What the player reads songs through: kept songs first (read only), then played ones, then the network. */
    val playback: CacheDataSource.Factory = CacheDataSource.Factory().setCache(keptCache).setCacheWriteDataSinkFactory(null).setUpstreamDataSourceFactory(playedLayer).setFlags(CacheDataSource.FLAG_IGNORE_CACHE_ON_ERROR)

    /** What pictures are read through: those seen before need no network. */
    val pictures: CacheDataSource.Factory = CacheDataSource.Factory().setCache(artCache).setUpstreamDataSourceFactory(network).setFlags(CacheDataSource.FLAG_IGNORE_CACHE_ON_ERROR)

    private val io = Executors.newFixedThreadPool(2)
    private val listeners = CopyOnWriteArrayList<() -> Unit>()

    val manager: DownloadManager = DownloadManager(
        context,
        database,
        keptCache,
        CacheDataSource.Factory().setCache(keptCache).setUpstreamDataSourceFactory(network),
        Executors.newFixedThreadPool(2),
    ).apply {
        maxParallelDownloads = 2
        addListener(object : DownloadManager.Listener {
            override fun onDownloadChanged(downloadManager: DownloadManager, download: Download, finalException: Exception?) = changed()
            override fun onDownloadRemoved(downloadManager: DownloadManager, download: Download) = changed()
        })
    }

    private fun changed() = listeners.forEach { it() }

    /** Calls `listener` (on any thread) whenever a song is kept, finished or removed. */
    fun onChange(listener: () -> Unit): () -> Unit {
        listeners.add(listener)
        return { listeners.remove(listener) }
    }

    private fun id(songId: Long) = "song-$songId"

    fun keep(songs: List<Song>) {
        songs.filter { !isKept(it.id) }.forEach { song ->
            val uri = Uri.parse("${session.api}/stream/${song.id}")
            val request = DownloadRequest.Builder(id(song.id), uri).setData(song.toJson().toByteArray()).build()
            DownloadService.sendAddDownload(context, SongDownloadService::class.java, request, false)
            prefetchPicture(song)
        }
    }

    fun remove(songs: List<Song>) {
        songs.forEach { DownloadService.sendRemoveDownload(context, SongDownloadService::class.java, id(it.id), false) }
    }

    fun removeAll() {
        DownloadService.sendRemoveAllDownloads(context, SongDownloadService::class.java, false)
    }

    fun isKept(songId: Long): Boolean = state(songId) != null

    /** Where a kept song is: null if it is not kept, else the percent that is on the device (100: all of it). */
    fun state(songId: Long): Int? {
        val d = manager.downloadIndex.getDownload(id(songId)) ?: return null
        return when (d.state) {
            Download.STATE_COMPLETED -> 100
            Download.STATE_FAILED, Download.STATE_REMOVING, Download.STATE_STOPPED -> null
            else -> d.percentDownloaded.toInt().coerceIn(0, 99)
        }
    }

    /** The kept songs (also those on their way), oldest first. */
    fun kept(): List<Song> {
        val out = mutableListOf<Pair<Long, Song>>()
        manager.downloadIndex.getDownloads(Download.STATE_COMPLETED, Download.STATE_QUEUED, Download.STATE_DOWNLOADING).use { cursor ->
            while (cursor.moveToNext()) {
                val d = cursor.download
                Song.fromJson(String(d.request.data))?.let { out += d.startTimeMs to it }
            }
        }
        return out.sortedBy { it.first }.map { it.second }
    }

    /** How much room the kept songs take. */
    fun bytes(): Long = keptCache.cacheSpace

    /** The picture of a song is kept with it, so that the list looks the same without the network. */
    private fun prefetchPicture(song: Song) {
        io.execute {
            try {
                val source = pictures.createDataSource()
                CacheWriter(source, DataSpec(Uri.parse("${session.api}/art/song/${song.id}?w=320")), null, null).cache()
            } catch (e: Exception) {
                // No picture, or no network: the list shows a tile.
            }
        }
    }

    companion object {
        private const val PLAYED_BYTES = 1024L * 1024 * 1024
        private const val ART_BYTES = 100L * 1024 * 1024

        @Volatile private var instance: Offline? = null

        fun of(context: Context): Offline = instance ?: synchronized(this) {
            instance ?: Offline(context.applicationContext).also { instance = it }
        }
    }
}

/** Keeps the songs coming while the app is closed, with a notification that says how far it is. */
class SongDownloadService : DownloadService(NOTIFICATION_ID, DEFAULT_FOREGROUND_NOTIFICATION_UPDATE_INTERVAL, CHANNEL, R.string.downloads, 0) {
    override fun getDownloadManager(): DownloadManager = Offline.of(this).manager

    override fun getScheduler(): Scheduler? = null

    override fun getForegroundNotification(downloads: MutableList<Download>, notMetRequirements: Int): Notification =
        DownloadNotificationHelper(this, CHANNEL).buildProgressNotification(
            this,
            android.R.drawable.stat_sys_download,
            null,
            "${downloads.size} songs",
            downloads,
            notMetRequirements,
        )

    companion object {
        const val CHANNEL = "downloads"
        private const val NOTIFICATION_ID = 7
    }
}
