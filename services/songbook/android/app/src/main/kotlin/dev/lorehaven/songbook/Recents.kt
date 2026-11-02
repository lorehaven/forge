package dev.lorehaven.songbook

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject

/** The songs played last, newest first, kept on this device (the car's "Recently played"). */
class Recents(context: Context) {
    private val prefs = context.applicationContext.getSharedPreferences("recent", Context.MODE_PRIVATE)

    @Synchronized
    fun songs(): List<Song> = try {
        val list = JSONArray(prefs.getString("songs", "[]"))
        (0 until list.length()).map {
            val o = list.getJSONObject(it)
            Song(o.getLong("id"), o.getString("title"), o.optString("artist").split(", ").filter { a -> a.isNotEmpty() }, o.optString("album"), o.optLong("album_id"), o.optInt("duration"))
        }
    } catch (e: Exception) {
        emptyList()
    }

    @Synchronized
    fun add(song: Song) {
        val kept = songs().filter { it.id != song.id }
        val all = (listOf(song) + kept).take(MAX)
        val list = JSONArray()
        all.forEach {
            list.put(JSONObject().put("id", it.id).put("title", it.title).put("artist", it.artist).put("album", it.album).put("album_id", it.albumId).put("duration", it.durationSecs))
        }
        prefs.edit().putString("songs", list.toString()).apply()
    }

    companion object {
        const val MAX = 40
    }
}
