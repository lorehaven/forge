package dev.lorehaven.songbook

import android.net.Uri
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import org.json.JSONObject

/** The queue to play when a song is chosen in a list: the songs of that list, and where in them it is. */
class Queue(val items: List<MediaItem>, val start: Int, val shuffled: Boolean)

/**
 * What Android Auto (and any other media browser) sees of the library: a tree of folders and songs. The Rust program ([Brain])
 * makes it; this turns what it answers into `MediaItem`s, and adds the one folder only the device knows: the kept songs.
 * Every id says where it is: `artist/Nate%20Dogg`, `album/7`; a song's is `s:<place>:<song>`, so that choosing it can play the
 * songs of its place, not just that one.
 *
 * Blocking: call it off the main thread.
 */
class Tree(private val session: Session, private val brain: Brain, private val offline: Offline) {
    // ── Items ────────────────────────────────────────────────────────────────

    private fun folderType(kind: String) = when (kind) {
        "album" -> MediaMetadata.MEDIA_TYPE_ALBUM
        "artist" -> MediaMetadata.MEDIA_TYPE_ARTIST
        "artists" -> MediaMetadata.MEDIA_TYPE_FOLDER_ARTISTS
        "playlist" -> MediaMetadata.MEDIA_TYPE_PLAYLIST
        "playlists" -> MediaMetadata.MEDIA_TYPE_FOLDER_PLAYLISTS
        else -> MediaMetadata.MEDIA_TYPE_FOLDER_MIXED
    }

    /** An item of the program's JSON as a `MediaItem`. */
    private fun item(o: JSONObject): MediaItem {
        val playable = o.optBoolean("playable")
        val art = o.optString("art").takeIf { it.isNotEmpty() }?.let { Uri.parse(it) }
        val metadata = MediaMetadata.Builder()
            .setTitle(o.optString("title"))
            .setArtworkUri(art)
            .setIsBrowsable(o.optBoolean("browsable"))
            .setIsPlayable(playable)
            .setMediaType(if (playable) MediaMetadata.MEDIA_TYPE_MUSIC else folderType(o.optString("kind")))
        if (playable) {
            metadata.setArtist(o.optString("artist")).setAlbumTitle(o.optString("album")).setDurationMs(o.optLong("duration_ms"))
        } else {
            metadata.setSubtitle(o.optString("subtitle").takeIf { it.isNotEmpty() })
        }
        return MediaItem.Builder()
            .setMediaId(o.getString("id"))
            .apply { o.optString("uri").takeIf { it.isNotEmpty() }?.let { setUri(it) } }
            .setMediaMetadata(metadata.build())
            .build()
    }

    private fun items(list: org.json.JSONArray?): List<MediaItem> = (0 until (list?.length() ?: 0)).map { item(list!!.getJSONObject(it)) }

    private fun folder(id: String, title: String): MediaItem = item(JSONObject().put("id", id).put("title", title).put("browsable", true).put("kind", "folder"))

    /** A song kept on the device: listed without the program, so it works with no network. */
    fun songItem(place: String, s: Song): MediaItem = MediaItem.Builder()
        .setMediaId("s:$place:${s.id}")
        .setUri(streamUri(s.id))
        .setMediaMetadata(
            MediaMetadata.Builder()
                .setTitle(s.title)
                .setArtist(s.artist)
                .setAlbumTitle(s.album)
                .setArtworkUri(Uri.parse("${session.api}/art/song/${s.id}?w=320"))
                .setDurationMs(s.durationSecs * 1000L)
                .setIsBrowsable(false)
                .setIsPlayable(true)
                .setMediaType(MediaMetadata.MEDIA_TYPE_MUSIC)
                .build(),
        )
        .build()

    fun streamUri(id: Long): Uri = Uri.parse("${session.api}/stream/$id")

    // ── The tree ─────────────────────────────────────────────────────────────

    fun root(): MediaItem = folder(ROOT, "Songbook")

    /** What the root holds when nobody is signed in: one line that says what to do (the car cannot ask for a password). */
    fun signInNotice(): MediaItem = item(JSONObject().put("id", "sign-in").put("title", "Open Songbook on your phone and sign in").put("browsable", false).put("kind", "folder"))
    fun recentRoot(): MediaItem = folder(RECENT_ROOT, "Recent")
    fun suggestedRoot(): MediaItem = folder(SUGGESTED_ROOT, "Suggested")

    fun children(parent: String): List<MediaItem> = when (parent) {
        "kept" -> offline.kept().map { songItem("kept", it) }
        ROOT -> (if (offline.kept().isNotEmpty()) listOf(folder("kept", "On this device")) else emptyList()) +
            items(brain.call("children", "parent" to parent).optJSONArray("items"))
        else -> items(brain.call("children", "parent" to parent).optJSONArray("items"))
    }

    /** A folder or a song by its id; null for one that is not there. */
    fun item(id: String): MediaItem? = when {
        id == ROOT -> root()
        id.startsWith("s:kept:") -> offline.kept().firstOrNull { "s:kept:${it.id}" == id }?.let { songItem("kept", it) }
        else -> brain.call("item", "target" to id).optJSONObject("item")?.let { item(it) }
    }

    /** The queue for a chosen song: its place's songs, started at it. Null when the id is not a song of this tree. */
    fun queue(id: String): Queue? {
        if (id.startsWith("s:kept:")) {
            val songs = offline.kept()
            val at = songs.indexOfFirst { "s:kept:${it.id}" == id }
            return if (songs.isEmpty()) null else Queue(songs.map { songItem("kept", it) }, at.coerceAtLeast(0), false)
        }
        val q = brain.call("queue", "target" to id).optJSONObject("queue") ?: return null
        return Queue(items(q.optJSONArray("items")), q.optInt("start"), q.optBoolean("shuffled"))
    }

    /** The songs for words said or typed (the voice search of the car). */
    fun search(text: String): List<MediaItem> = items(brain.call("search", "q" to text).optJSONArray("items"))

    /** A song of the player's queue by its id alone (`s:q:<id>`: an address and a picture; its details are looked up when it plays). */
    fun bareItem(id: Long): MediaItem = MediaItem.Builder()
        .setMediaId("s:q:$id")
        .setUri(streamUri(id))
        .setMediaMetadata(
            MediaMetadata.Builder()
                .setArtworkUri(Uri.parse("${session.api}/art/song/$id?w=320"))
                .setIsPlayable(true)
                .setIsBrowsable(false)
                .setMediaType(MediaMetadata.MEDIA_TYPE_MUSIC)
                .build(),
        )
        .build()

    /** A queue item the player's screens made (`s:q:<id>`: an address and nothing else) with what the service knows of the song. */
    fun fill(item: MediaItem): MediaItem? {
        val id = item.mediaId.substringAfterLast(':').toLongOrNull() ?: return null
        val found = brain.call("songs", "ids" to org.json.JSONArray(listOf(id))).optJSONArray("items") ?: return null
        return if (found.length() == 0) null else items(found)[0]
    }

    /** A song of a list added to the queue by itself: its address is all it lacks. */
    fun resolve(item: MediaItem): MediaItem {
        val song = songOf(item)?.id ?: return item
        return if (item.localConfiguration != null) item else item.buildUpon().setUri(streamUri(song)).build()
    }

    companion object {
        const val ROOT = "root"
        const val RECENT_ROOT = "recent-root"
        const val SUGGESTED_ROOT = "suggested-root"

        /** The song a list or queue item stands for (`s:<place>:<id>`), as far as its metadata says. */
        fun songOf(item: MediaItem): Song? {
            val id = item.mediaId.split(':', limit = 3).takeIf { it.size == 3 && it[0] == "s" }?.get(2)?.toLongOrNull() ?: return null
            val m = item.mediaMetadata
            return Song(id, m.title?.toString() ?: return null, (m.artist?.toString() ?: "").split(", ").filter { it.isNotEmpty() }, m.albumTitle?.toString() ?: "", 0, ((m.durationMs ?: 0) / 1000).toInt())
        }
    }
}
