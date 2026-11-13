package dev.lorehaven.songbook

import android.net.Uri
import android.os.Bundle
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
    private fun item(o: JSONObject, small: Boolean = false): MediaItem {
        val playable = o.optBoolean("playable")
        // The car reads pictures through our provider (it has no token); in lists a smaller picture is enough and loads faster.
        val art = o.optString("art").takeIf { it.isNotEmpty() }?.let { ArtProvider.uri(if (small) it.replace("w=320", "w=160") else it) }
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
        // How Android Auto lays it out: the heading of the section it is in, and a card in a grid or a row in a list.
        val group = o.optString("group")
        if (group.isNotEmpty() || o.optBoolean("grid")) {
            val extras = Bundle()
            if (group.isNotEmpty()) extras.putString(GROUP_TITLE_HINT, group)
            extras.putInt(SINGLE_ITEM_HINT, if (o.optBoolean("grid")) STYLE_GRID else STYLE_LIST)
            metadata.setExtras(extras)
        }
        return MediaItem.Builder()
            .setMediaId(o.getString("id"))
            .apply { o.optString("uri").takeIf { it.isNotEmpty() }?.let { setUri(it) } }
            .setMediaMetadata(metadata.build())
            .build()
    }

    private fun items(list: org.json.JSONArray?, small: Boolean = false): List<MediaItem> = (0 until (list?.length() ?: 0)).map { item(list!!.getJSONObject(it), small) }

    private fun folder(id: String, title: String): MediaItem = item(JSONObject().put("id", id).put("title", title).put("browsable", true).put("kind", "folder"))

    /** A song kept on the device: listed without the program, so it works with no network. */
    fun songItem(place: String, s: Song, small: Boolean = false): MediaItem = MediaItem.Builder()
        .setMediaId("s:$place:${s.id}")
        .setUri(streamUri(s.id))
        .setMediaMetadata(
            MediaMetadata.Builder()
                .setTitle(s.title)
                .setArtist(s.artist)
                .setAlbumTitle(s.album)
                .setArtworkUri(ArtProvider.uri("${session.api}/art/song/${s.id}?w=${if (small) 160 else 320}"))
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

    /** "Play all" and "Shuffle" at the top of the list of kept songs, as the program's lists have. */
    private fun keptEntries(): List<MediaItem> = listOf("play" to "Play all", "shuffle" to "Shuffle").map { (mode, title) ->
        item(JSONObject().put("id", "$mode/kept").put("title", title).put("playable", true).put("kind", "play"))
    }

    fun children(parent: String): List<MediaItem> = when (parent) {
        "kept" -> keptEntries() + offline.kept().map { songItem("kept", it, true) }
        // The tabs of the car: Home, Artists, Playlists, and last the songs kept on the device.
        ROOT -> items(brain.call("children", "parent" to parent).optJSONArray("items"), true) +
            (if (offline.kept().isNotEmpty()) listOf(folder("kept", "On this device")) else emptyList())
        else -> items(brain.call("children", "parent" to parent).optJSONArray("items"), true)
    }

    /** A folder or a song by its id; null for one that is not there. */
    fun item(id: String): MediaItem? = when {
        id == ROOT -> root()
        id == "play/kept" || id == "shuffle/kept" -> keptEntries().firstOrNull { it.mediaId == id }
        id.startsWith("s:kept:") -> offline.kept().firstOrNull { "s:kept:${it.id}" == id }?.let { songItem("kept", it) }
        else -> brain.call("item", "target" to id).optJSONObject("item")?.let { item(it) }
    }

    /** The queue for a chosen song: its place's songs, started at it. Null when the id is not a song of this tree. */
    fun queue(id: String): Queue? {
        if (id.startsWith("s:kept:") || id == "play/kept" || id == "shuffle/kept") {
            val songs = offline.kept()
            val at = songs.indexOfFirst { "s:kept:${it.id}" == id }
            return if (songs.isEmpty()) null else Queue(songs.map { songItem("kept", it) }, at.coerceAtLeast(0), id == "shuffle/kept")
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
                .setArtworkUri(ArtProvider.uri("${session.api}/art/song/$id?w=320"))
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

        /** Android Auto's content style extras (`androidx.media.utils.MediaConstants`, written out so as not to need the library). */
        const val CONTENT_STYLE_SUPPORTED = "android.media.browse.CONTENT_STYLE_SUPPORTED"
        const val BROWSABLE_HINT = "android.media.browse.CONTENT_STYLE_BROWSABLE_HINT"
        const val PLAYABLE_HINT = "android.media.browse.CONTENT_STYLE_PLAYABLE_HINT"
        const val SINGLE_ITEM_HINT = "android.media.browse.CONTENT_STYLE_SINGLE_ITEM_HINT"
        const val GROUP_TITLE_HINT = "android.media.browse.CONTENT_STYLE_GROUP_TITLE_HINT"
        const val STYLE_LIST = 1
        const val STYLE_GRID = 2

        /** What the car is told about the root: it may use grid and list layouts; rows unless an item says it is a card. */
        fun rootStyle(): Bundle = Bundle().apply {
            putBoolean(CONTENT_STYLE_SUPPORTED, true)
            putInt(BROWSABLE_HINT, STYLE_LIST)
            putInt(PLAYABLE_HINT, STYLE_LIST)
        }

        /** The song a list or queue item stands for (`s:<place>:<id>`), as far as its metadata says. */
        fun songOf(item: MediaItem): Song? {
            val id = item.mediaId.split(':', limit = 3).takeIf { it.size == 3 && it[0] == "s" }?.get(2)?.toLongOrNull() ?: return null
            val m = item.mediaMetadata
            return Song(id, m.title?.toString() ?: return null, (m.artist?.toString() ?: "").split(", ").filter { it.isNotEmpty() }, m.albumTitle?.toString() ?: "", 0, ((m.durationMs ?: 0) / 1000).toInt())
        }
    }
}
