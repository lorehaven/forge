package dev.lorehaven.songbook

import org.json.JSONArray
import org.json.JSONObject
import java.net.URLEncoder

data class Song(
    val id: Long,
    val title: String,
    val artists: List<String>,
    val album: String,
    val albumId: Long,
    val durationSecs: Int,
) {
    val artist: String get() = artists.joinToString(", ")

    /** The song as it is kept on the device (a kept song is listed without the server). */
    fun toJson(): String = JSONObject().put("id", id).put("title", title).put("artist", artist).put("album", album).put("album_id", albumId).put("duration", durationSecs).toString()

    companion object {
        fun fromJson(text: String): Song? = try {
            val o = JSONObject(text)
            Song(o.getLong("id"), o.getString("title"), o.optString("artist").split(", ").filter { it.isNotEmpty() }, o.optString("album"), o.optLong("album_id"), o.optInt("duration"))
        } catch (e: Exception) {
            null
        }
    }
}

data class Album(val id: Long, val title: String, val artist: String, val year: String, val songs: Int)
data class Artist(val name: String, val songs: Int, val albums: Int)
data class Category(val name: String, val songs: Int)
data class Mix(val name: String, val kind: String, val category: String, val songs: Int) {
    val shuffled: Boolean get() = kind == "mix"
}
data class Playlist(val id: Long, val name: String, val songs: Int)
data class Home(val popular: List<Song>, val mixes: List<Mix>, val artists: List<Artist>, val albums: List<Album>)
data class Overview(val artists: List<Artist>, val categories: List<Category>)
data class Found(val songs: List<Song>, val albums: List<Album>, val artists: List<Artist>)

/** What the songs of a place are asked for with (the fields of `GET /library/songs`). */
data class SongQuery(
    val ids: List<Long> = emptyList(),
    val playlist: Long = 0,
    val album: Long = 0,
    val artist: String = "",
    val category: String = "",
    val sort: String = "",
    val limit: Int = 0,
)

/** The library API of the service, for the signed-in person. Blocking: call it off the main thread. */
class Library(private val session: Session) {
    private fun enc(text: String) = URLEncoder.encode(text, "UTF-8")

    /** A GET with the token; a refused token is renewed once. */
    private fun get(path: String): String = call("GET", path, null)

    private fun call(method: String, path: String, json: String?): String {
        for (attempt in 0..1) {
            val answer = Http.call(method, session.api + path, json, token = session.token())
            when (answer.code) {
                200 -> return answer.body
                401 -> if (attempt == 0) session.expire() else throw SignedOut()
                403 -> throw SignedOut("this account may not use Songbook")
                else -> throw Unreachable("the server answered ${answer.code}")
            }
        }
        throw SignedOut()
    }

    private fun songs(list: JSONArray?): List<Song> = (0 until (list?.length() ?: 0)).map { song(list!!.getJSONObject(it)) }

    private fun song(o: JSONObject) = Song(
        id = o.getLong("id"),
        title = o.getString("title"),
        artists = strings(o.optJSONArray("artists")),
        album = o.optString("album"),
        albumId = o.optLong("album_id"),
        durationSecs = o.optInt("duration"),
    )

    private fun strings(list: JSONArray?): List<String> = (0 until (list?.length() ?: 0)).map { list!!.getString(it) }

    private fun album(o: JSONObject) = Album(o.getLong("id"), o.getString("title"), o.optString("artist"), o.optString("year"), o.optInt("songs"))
    private fun artist(o: JSONObject) = Artist(o.getString("name"), o.optInt("songs"), o.optInt("albums"))

    private fun <T> each(list: JSONArray?, make: (JSONObject) -> T): List<T> = (0 until (list?.length() ?: 0)).map { make(list!!.getJSONObject(it)) }

    fun overview(): Overview {
        val o = JSONObject(get("/library/overview"))
        return Overview(each(o.optJSONArray("artists"), ::artist), each(o.optJSONArray("categories")) { Category(it.getString("name"), it.optInt("songs")) })
    }

    fun home(): Home {
        val o = JSONObject(get("/library/home"))
        return Home(
            songs(o.optJSONArray("popular")),
            each(o.optJSONArray("mixes")) { Mix(it.getString("name"), it.optString("kind"), it.optString("category"), it.optInt("songs")) },
            each(o.optJSONArray("artists"), ::artist),
            each(o.optJSONArray("albums"), ::album),
        )
    }

    fun albums(artist: String = "", limit: Int = 0): List<Album> =
        each(JSONArray(get("/library/albums?" + listOfNotNull(artist.takeIf { it.isNotEmpty() }?.let { "artist=${enc(it)}" }, limit.takeIf { it > 0 }?.let { "limit=$it" }).joinToString("&"))), ::album)

    fun songs(q: SongQuery): List<Song> {
        val parts = buildList {
            if (q.ids.isNotEmpty()) add("ids=${enc(q.ids.joinToString(","))}")
            if (q.playlist != 0L) add("playlist=${q.playlist}")
            if (q.album != 0L) add("album=${q.album}")
            if (q.artist.isNotEmpty()) add("artist=${enc(q.artist)}")
            if (q.category.isNotEmpty()) add("category=${enc(q.category)}")
            if (q.sort.isNotEmpty()) add("sort=${enc(q.sort)}")
            if (q.limit > 0) add("limit=${q.limit}")
        }
        return songs(JSONObject(get("/library/songs" + if (parts.isEmpty()) "" else "?" + parts.joinToString("&"))).optJSONArray("songs"))
    }

    fun search(text: String): Found {
        val o = JSONObject(get("/library/search?q=${enc(text)}"))
        return Found(songs(o.optJSONArray("songs")), each(o.optJSONArray("albums"), ::album), each(o.optJSONArray("artists"), ::artist))
    }

    /** What this person played last on any of their devices (each song once, newest first). */
    fun recent(limit: Int = 40): List<Song> = songs(JSONObject(get("/library/recent?limit=$limit")).optJSONArray("songs"))

    /** Tells the service a song was started, so that every device's "recently played" has it. */
    fun played(songId: Long) {
        call("POST", "/history", JSONObject().put("song_id", songId).toString())
    }

    fun playlists(): List<Playlist> = each(JSONArray(get("/playlists"))) { Playlist(it.getLong("id"), it.getString("name"), it.optInt("songs")) }
}
