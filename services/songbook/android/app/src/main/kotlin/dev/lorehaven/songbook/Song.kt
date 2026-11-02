package dev.lorehaven.songbook

import org.json.JSONArray
import org.json.JSONObject

/** One song, as far as the phone needs to know it (a kept song is listed without the server). */
data class Song(
    val id: Long,
    val title: String,
    val artists: List<String>,
    val album: String,
    val albumId: Long,
    val durationSecs: Int,
) {
    val artist: String get() = artists.joinToString(", ")

    /** The song as it is kept on the device. */
    fun toJson(): String = JSONObject().put("id", id).put("title", title).put("artist", artist).put("album", album).put("album_id", albumId).put("duration", durationSecs).toString()

    /** The song as the Rust program reads it (the shape of the service's `SongInfo`). */
    fun toInfo(): JSONObject = JSONObject()
        .put("id", id).put("title", title).put("artists", JSONArray(artists)).put("album", album)
        .put("album_id", albumId).put("year", "").put("duration", durationSecs)

    companion object {
        fun fromJson(text: String): Song? = try {
            val o = JSONObject(text)
            Song(o.getLong("id"), o.getString("title"), o.optString("artist").split(", ").filter { it.isNotEmpty() }, o.optString("album"), o.optLong("album_id"), o.optInt("duration"))
        } catch (e: Exception) {
            null
        }

        /** From the service's `SongInfo`. */
        fun fromInfo(o: JSONObject): Song {
            val artists = o.optJSONArray("artists")
            return Song(o.getLong("id"), o.getString("title"), (0 until (artists?.length() ?: 0)).map { artists!!.getString(it) }, o.optString("album"), o.optLong("album_id"), o.optInt("duration"))
        }
    }
}

/** The service could not be reached, or answered something that is not an answer. */
class Unreachable(message: String) : java.io.IOException(message)

/** The session is over: the refresh token is refused, or nobody is signed in. */
class SignedOut(message: String = "signed out") : java.io.IOException(message)
