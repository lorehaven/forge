package dev.lorehaven.songbook

import androidx.media3.common.MediaItem
import androidx.media3.session.MediaSession

/**
 * Where the music was left: the song and how far into it. Android Auto (and the media buttons of a headset or a car) start
 * the app again with "play" when the person comes back, and the player is then put back as it was: the same list, the same
 * song, the same place. The Rust program ([Brain]) keeps it.
 */
object Resume {
    class Saved(val mediaId: String, val positionMs: Long, val song: Song)

    fun save(brain: Brain, item: MediaItem, positionMs: Long) {
        val song = Tree.songOf(item) ?: return
        try {
            brain.call("last_save", "queue_id" to item.mediaId, "position_ms" to positionMs, "song" to song.toInfo())
        } catch (e: java.io.IOException) {
            // Not kept this time.
        }
    }

    fun load(brain: Brain): Saved? = try {
        brain.call("last_load").optJSONObject("last")?.let { Saved(it.getString("id"), it.optLong("position_ms"), Song.fromInfo(it.getJSONObject("song"))) }
    } catch (e: java.io.IOException) {
        null
    }

    /** The queue as it was, started at the song and the place. Null when nothing was played (or the list is gone). */
    fun queue(tree: Tree, saved: Saved): MediaSession.MediaItemsWithStartPosition? {
        val queue = tree.queue(saved.mediaId) ?: return null
        return MediaSession.MediaItemsWithStartPosition(queue.items, queue.start, saved.positionMs)
    }
}
