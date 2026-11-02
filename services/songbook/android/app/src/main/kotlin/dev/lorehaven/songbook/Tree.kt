package dev.lorehaven.songbook

import android.content.Context
import android.net.Uri
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import java.net.URLDecoder
import java.net.URLEncoder

/** The queue to play when a song is chosen in a list: the songs of that list, and where in them it is. */
class Queue(val items: List<MediaItem>, val start: Int, val shuffled: Boolean)

/**
 * What Android Auto (and any other media browser) sees of the library: a tree of folders and songs, made from the API when
 * asked for. Every id says where it is: `artist/Nate%20Dogg`, `album/7`; a song's is `s:<place>:<song>`, so that choosing
 * it can play the songs of its place, not just that one.
 *
 * Blocking: call it off the main thread.
 */
class Tree(private val context: Context, private val session: Session, private val library: Library, private val recents: Recents, private val offline: Offline) {
    private var home: Pair<Long, Home>? = null
    private var overview: Pair<Long, Overview>? = null

    private fun enc(text: String) = URLEncoder.encode(text, "UTF-8")
    private fun dec(text: String) = URLDecoder.decode(text, "UTF-8")

    @Synchronized
    private fun home(): Home {
        val now = System.currentTimeMillis()
        home?.takeIf { now - it.first < FRESH_MS }?.let { return it.second }
        return library.home().also { home = now to it }
    }

    @Synchronized
    private fun overview(): Overview {
        val now = System.currentTimeMillis()
        overview?.takeIf { now - it.first < FRESH_MS }?.let { return it.second }
        return library.overview().also { overview = now to it }
    }

    /** What was played last: as the service knows it (every device), else what this phone remembers. */
    private fun recentSongs(): List<Song> = try {
        val server = library.recent()
        server + recents.songs().filter { local -> server.none { it.id == local.id } }
    } catch (e: java.io.IOException) {
        recents.songs()
    }

    // ── Items ────────────────────────────────────────────────────────────────

    private fun folder(id: String, title: String, subtitle: String? = null, art: String? = null, type: Int = MediaMetadata.MEDIA_TYPE_FOLDER_MIXED): MediaItem =
        MediaItem.Builder()
            .setMediaId(id)
            .setMediaMetadata(
                MediaMetadata.Builder()
                    .setTitle(title)
                    .setSubtitle(subtitle)
                    .setArtworkUri(art?.let { Uri.parse(it) })
                    .setIsBrowsable(true)
                    .setIsPlayable(false)
                    .setMediaType(type)
                    .build(),
            )
            .build()

    fun streamUri(id: Long): Uri = Uri.parse("${session.api}/stream/$id")

    private fun artSong(id: Long) = "${session.api}/art/song/$id?w=320"
    private fun artAlbum(id: Long) = "${session.api}/art/album/$id?w=320"
    private fun artArtist(name: String) = "${session.api}/art/artist?name=${enc(name)}&w=320"

    fun songItem(place: String, s: Song): MediaItem =
        MediaItem.Builder()
            .setMediaId("s:$place:${s.id}")
            .setUri(streamUri(s.id))
            .setMediaMetadata(
                MediaMetadata.Builder()
                    .setTitle(s.title)
                    .setArtist(s.artist)
                    .setAlbumTitle(s.album)
                    .setArtworkUri(Uri.parse(artSong(s.id)))
                    .setDurationMs(s.durationSecs * 1000L)
                    .setIsBrowsable(false)
                    .setIsPlayable(true)
                    .setMediaType(MediaMetadata.MEDIA_TYPE_MUSIC)
                    .build(),
            )
            .build()

    private fun songItems(place: String, songs: List<Song>) = songs.map { songItem(place, it) }

    private fun albumFolder(a: Album) = folder("album/${a.id}", a.title, "${a.year.take(4)} · ${a.artist}".trim(' ', '·'), artAlbum(a.id), MediaMetadata.MEDIA_TYPE_ALBUM)
    private fun artistFolder(a: Artist) = folder("artist/${enc(a.name)}", a.name, "${a.songs} songs", artArtist(a.name), MediaMetadata.MEDIA_TYPE_ARTIST)

    // ── The tree ─────────────────────────────────────────────────────────────

    fun root(): MediaItem = folder(ROOT, "Songbook")
    fun recentRoot(): MediaItem = folder(RECENT_ROOT, "Recent")
    fun suggestedRoot(): MediaItem = folder(SUGGESTED_ROOT, "Suggested")

    fun children(parent: String): List<MediaItem> {
        val (kind, key) = parent.substringBefore('/') to parent.substringAfter('/', "")
        return when {
            parent == ROOT -> buildList {
                if (offline.kept().isNotEmpty()) add(folder("kept", "On this device"))
                add(folder("recent", "Recently played"))
                add(folder("mixes", "Mixes"))
                add(folder("popular", "Popular"))
                add(folder("artists", "Artists", type = MediaMetadata.MEDIA_TYPE_FOLDER_ARTISTS))
                add(folder("categories", "Categories"))
                add(folder("playlists", "Playlists", type = MediaMetadata.MEDIA_TYPE_FOLDER_PLAYLISTS))
            }
            parent == "kept" -> songItems("kept", offline.kept())
            parent == "recent" -> songItems("recent", recentSongs())
            // What Android Auto asks for to offer "continue": the song that was playing, and a few to start with.
            parent == RECENT_ROOT -> Resume.load(context)?.let { listOf(songItem("recent", it.song)) } ?: songItems("recent", recentSongs().take(1))
            parent == SUGGESTED_ROOT -> songItems("popular", home().popular.take(5))
            parent == "popular" -> songItems("popular", home().popular)
            parent == "mixes" -> home().mixes.mapIndexed { i, m ->
                folder("mix/$i", m.name, if (m.shuffled) "Shuffled · ${m.songs} songs" else "Top ${m.songs}")
            }
            parent == "artists" -> letters().map { folder("letter/$it", it) }
            kind == "letter" -> overview().artists.filter { letter(it.name) == key }.map(::artistFolder)
            kind == "artist" -> {
                val name = dec(key)
                listOf(folder("artistsongs/${enc(name)}", "All songs")) + library.albums(name).map(::albumFolder)
            }
            parent == "categories" -> overview().categories.map { folder("category/${enc(it.name)}", it.name, "${it.songs} songs") }
            parent == "playlists" -> library.playlists().map { folder("playlist/${it.id}", it.name, "${it.songs} songs", type = MediaMetadata.MEDIA_TYPE_PLAYLIST) }
            else -> {
                val place = placeOf(parent)
                songItems(place, songsOf(place).first)
            }
        }
    }

    /** The place whose songs a folder lists (`artistsongs/x` lists `artist/x`). */
    private fun placeOf(folderId: String) = if (folderId.startsWith("artistsongs/")) "artist/" + folderId.substringAfter('/') else folderId

    /** A folder or a song by its id; null for one that is not there. */
    fun item(id: String): MediaItem? = when {
        id == ROOT -> root()
        id.startsWith("s:") -> parseSong(id)?.let { (place, songId) -> songsOf(place).first.firstOrNull { it.id == songId }?.let { songItem(place, it) } }
        else -> null
    }

    private fun letter(name: String): String = name.trim().firstOrNull()?.let { if (it.isLetter()) it.uppercaseChar().toString() else "#" } ?: "#"
    private fun letters(): List<String> = overview().artists.map { letter(it.name) }.distinct().sorted()

    /** `s:<place>:<id>` as (place, id). */
    private fun parseSong(id: String): Pair<String, Long>? {
        val parts = id.split(':', limit = 3)
        return if (parts.size == 3 && parts[0] == "s") parts[2].toLongOrNull()?.let { parts[1] to it } else null
    }

    /** The songs of a place, and whether it is played shuffled (a mix). */
    private fun songsOf(place: String): Pair<List<Song>, Boolean> {
        val kind = place.substringBefore('/')
        val key = place.substringAfter('/', "")
        return when {
            place == "kept" -> offline.kept() to false
            place == "recent" -> recentSongs() to false
            place == "popular" -> home().popular to false
            kind == "mix" -> home().mixes.getOrNull(key.toIntOrNull() ?: -1)?.let {
                library.songs(SongQuery(category = it.category, sort = "popular", limit = it.songs)) to it.shuffled
            } ?: (emptyList<Song>() to false)
            kind == "album" -> library.songs(SongQuery(album = key.toLongOrNull() ?: 0)) to false
            kind == "artist" -> library.songs(SongQuery(artist = dec(key), limit = CAR_LIMIT)) to false
            kind == "category" -> library.songs(SongQuery(category = dec(key), sort = "popular", limit = 100)) to false
            kind == "playlist" -> library.songs(SongQuery(playlist = key.toLongOrNull() ?: 0, limit = CAR_LIMIT)) to false
            kind == "search" -> library.search(dec(key)).songs to false
            else -> emptyList<Song>() to false
        }
    }

    /** The queue for a chosen song: its place's songs, started at it. Null when the id is not a song of this tree. */
    fun queue(id: String): Queue? {
        val (place, songId) = parseSong(id) ?: return null
        val (songs, shuffled) = songsOf(place)
        val items = songItems(place, songs)
        val at = songs.indexOfFirst { it.id == songId }
        return if (items.isEmpty()) null else Queue(items, if (at >= 0) at else 0, shuffled)
    }

    /** The songs for words said or typed (the voice search of the car). */
    fun search(text: String): List<MediaItem> = songItems("search/${enc(text)}", library.search(text).songs)

    /** A song of a list added to the queue by itself: its address is all it lacks. */
    fun resolve(item: MediaItem): MediaItem {
        val song = parseSong(item.mediaId)?.second ?: return item
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

        /** How long what was asked of the service is used again (the car asks often). */
        private const val FRESH_MS = 2 * 60_000L

        /** A car shows a screenful: a place's queue is not the whole library. */
        private const val CAR_LIMIT = 300
    }
}
