package dev.lorehaven.songbook

import android.content.Context
import java.net.HttpURLConnection
import java.net.URL

/** What a call to a stand-in server answered. */
class Answer(val code: Int, val body: String)

/** The tests' own plain HTTP, to steer the stand-in Gatehouse (`/__down`, `/__revoke`, `/__stats`). */
object Http {
    fun call(method: String, url: String, json: String? = null): Answer {
        val c = URL(url).openConnection() as HttpURLConnection
        return try {
            c.requestMethod = method
            if (json != null) {
                c.doOutput = true
                c.setRequestProperty("Content-Type", "application/json")
                c.outputStream.use { it.write(json.toByteArray()) }
            }
            val code = c.responseCode
            Answer(code, (if (code >= 400) c.errorStream else c.inputStream)?.bufferedReader()?.use { it.readText() } ?: "")
        } finally {
            c.disconnect()
        }
    }
}

/** The library as the program shows it, for tests that need songs to play. */
object Lib {
    private fun songs(context: Context, folder: String): List<Song> {
        val items = Brain.of(context).call("children", "parent" to folder).getJSONArray("items")
        return (0 until items.length()).map {
            val o = items.getJSONObject(it)
            Song(o.getString("id").substringAfterLast(':').toLong(), o.getString("title"), o.optString("artist").split(", ").filter { a -> a.isNotEmpty() }, o.optString("album"), 0, (o.optLong("duration_ms") / 1000).toInt())
        }
    }

    fun popular(context: Context): List<Song> = songs(context, "popular")

    /** The ids of what the service says was played last. */
    fun recent(context: Context): List<Long> = songs(context, "recent").map { it.id }
}
