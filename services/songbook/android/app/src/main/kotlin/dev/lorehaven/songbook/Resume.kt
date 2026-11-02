package dev.lorehaven.songbook

import android.content.Context
import androidx.media3.common.MediaItem
import androidx.media3.session.MediaSession
import org.json.JSONObject

/**
 * Where the music was left: the song and how far into it. Android Auto (and the media buttons of a headset or a car) start
 * the app again with "play" when the person comes back, and the player is then put back as it was: the same list, the same
 * song, the same place.
 */
object Resume {
    class Saved(val mediaId: String, val positionMs: Long, val song: Song)

    fun save(context: Context, item: MediaItem, positionMs: Long) {
        val song = Tree.songOf(item) ?: return
        val json = JSONObject().put("id", item.mediaId).put("position", positionMs).put("song", song.toJson())
        context.getSharedPreferences("last", Context.MODE_PRIVATE).edit().putString("played", json.toString()).apply()
    }

    fun load(context: Context): Saved? = try {
        val o = JSONObject(context.getSharedPreferences("last", Context.MODE_PRIVATE).getString("played", null) ?: return null)
        Song.fromJson(o.getString("song"))?.let { Saved(o.getString("id"), o.optLong("position"), it) }
    } catch (e: Exception) {
        null
    }

    /** The queue as it was, started at the song and the place. Null when nothing was played (or the list is gone). */
    fun queue(tree: Tree, saved: Saved): MediaSession.MediaItemsWithStartPosition? {
        val queue = tree.queue(saved.mediaId) ?: return null
        return MediaSession.MediaItemsWithStartPosition(queue.items, queue.start, saved.positionMs)
    }
}
