package dev.lorehaven.songbook

import android.os.Bundle
import android.os.Handler
import android.os.Looper
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.datasource.DataSourceBitmapLoader
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.session.CommandButton
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaLibraryService
import androidx.media3.session.MediaSession
import androidx.media3.session.SessionCommand
import androidx.media3.session.SessionResult
import com.google.common.collect.ImmutableList
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.ListeningExecutorService
import com.google.common.util.concurrent.MoreExecutors
import java.util.concurrent.Executors

/**
 * Plays the library and tells Android Auto, the lock screen and the notification what plays and what can be browsed.
 *
 * The player is Media3's ExoPlayer, reading the songs from the service with the person's token (and keeping what it has
 * read, so a song heard once plays again without the network). The session is a *library* session: a media browser (the car)
 * asks it for the tree of [Tree] and chooses songs from it.
 */
class PlaybackService : MediaLibraryService() {
    private var librarySession: MediaLibrarySession? = null
    private var crossfader: Crossfader? = null
    private val work: ListeningExecutorService = MoreExecutors.listeningDecorator(Executors.newCachedThreadPool())
    private val main = Handler(Looper.getMainLooper())

    override fun onCreate() {
        super.onCreate()
        val session = Session.of(this)
        val brain = Brain.of(this)
        val offline = Offline.of(this)
        val tree = Tree(session, brain, offline)

        val attributes = AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MUSIC).build()
        val sources = DefaultMediaSourceFactory(this).setDataSourceFactory(offline.playback)
        val player = ExoPlayer.Builder(this)
            .setMediaSourceFactory(sources)
            .setAudioAttributes(attributes, true)
            .setHandleAudioBecomingNoisy(true)
            .setWakeMode(C.WAKE_MODE_NETWORK)
            .build()
        // The fade of one song into the next uses a second player for the end of the song that is leaving.
        crossfader = Crossfader(player, Settings.of(this)) {
            ExoPlayer.Builder(this).setMediaSourceFactory(sources).setAudioAttributes(attributes, false).build()
        }.also { it.start() }
        player.addListener(object : Player.Listener {
            override fun onMediaItemTransition(mediaItem: MediaItem?, reason: Int) {
                val item = mediaItem ?: return
                // A song the player's screens queued is only an address: its title, artist and picture are looked up now, so
                // that the lock screen, the car and the history have them.
                if (item.mediaMetadata.title == null && item.mediaId.startsWith("s:")) {
                    work.execute {
                        val filled = try { tree.fill(item) } catch (e: java.io.IOException) { null } ?: return@execute
                        // Remembered now: replacing the item may or may not be reported as a new song.
                        remember(filled, reason, 0)
                        main.post {
                            val at = (0 until player.mediaItemCount).firstOrNull { player.getMediaItemAt(it).mediaId == item.mediaId }
                            if (at != null) player.replaceMediaItem(at, filled)
                        }
                    }
                    return
                }
                remember(item, reason, 0)
            }

            override fun onIsPlayingChanged(isPlaying: Boolean) {
                // Where the music was left, for when the person comes back.
                if (!isPlaying) player.currentMediaItem?.let { item -> val at = player.currentPosition; work.execute { Resume.save(brain, item, at) } }
            }
        })

        val settings = Settings.of(this)
        librarySession = MediaLibrarySession.Builder(this, player, Callback(session, tree, brain, player, settings))
            .setBitmapLoader(DataSourceBitmapLoader.Builder(this).setDataSourceFactory(offline.pictures).build())
            .build()
        // The car's button for the crossfade follows the setting, wherever it is changed.
        stopWatching = settings.onChange { main.post { librarySession?.setMediaButtonPreferences(ImmutableList.of(fadeButton(settings))) } }
    }

    private var stopWatching: (() -> Unit)? = null

    /** The song last told to the service, so that filling in its details does not count as playing it twice. */
    private var lastTold: String? = null

    /** Kept on this phone, and told to the service so that every device's "recently played" has it; then the screens hear of it. */
    private fun remember(item: MediaItem, reason: Int, at: Long) {
        if (reason == Player.MEDIA_ITEM_TRANSITION_REASON_PLAYLIST_CHANGED && lastTold == item.mediaId) return
        val song = Tree.songOf(item) ?: return
        lastTold = item.mediaId
        val brain = Brain.of(this)
        work.execute {
            try { brain.call("played", "song" to song.toInfo()) } catch (e: java.io.IOException) { /* later, on the next play */ }
            Resume.save(brain, item, at)
            main.post { onPlayed?.invoke() }
        }
    }

    /** The button on the car's player: the crossfade now, and a press moves to the next length. */
    private fun fadeButton(settings: Settings): CommandButton {
        val secs = settings.crossfadeSecs
        return CommandButton.Builder(CommandButton.ICON_UNDEFINED)
            .setCustomIconResId(R.drawable.ic_crossfade)
            .setDisplayName(if (secs == 0) "Crossfade: off" else "Crossfade: $secs s")
            .setSessionCommand(SessionCommand(FADE_COMMAND, Bundle.EMPTY))
            .build()
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaLibrarySession? = librarySession

    override fun onDestroy() {
        stopWatching?.invoke()
        librarySession?.player?.let { p -> p.currentMediaItem?.let { Resume.save(Brain.of(this), it, p.currentPosition) } }
        crossfader?.release()
        librarySession?.run {
            player.release()
            release()
        }
        librarySession = null
        work.shutdown()
        super.onDestroy()
    }

    private inner class Callback(private val session: Session, private val tree: Tree, private val brain: Brain, private val player: Player, private val settings: Settings) : MediaLibrarySession.Callback {
        /** Runs `block` off the main thread; a person who is not signed in, or a server that is not there, is an error result. */
        private fun <T : Any> asynchronous(block: () -> LibraryResult<T>): ListenableFuture<LibraryResult<T>> = work.submit<LibraryResult<T>> {
            if (!session.signedIn) return@submit LibraryResult.ofError<T>(LibraryResult.RESULT_ERROR_SESSION_AUTHENTICATION_EXPIRED)
            try {
                block()
            } catch (e: SignedOut) {
                session.refresh()
                SignedOutNotice.show(this@PlaybackService)
                LibraryResult.ofError<T>(LibraryResult.RESULT_ERROR_SESSION_AUTHENTICATION_EXPIRED)
            } catch (e: java.io.IOException) {
                LibraryResult.ofError<T>(LibraryResult.RESULT_ERROR_IO)
            }
        }

        override fun onConnect(session: MediaSession, controller: MediaSession.ControllerInfo): MediaSession.ConnectionResult =
            MediaSession.ConnectionResult.AcceptedResultBuilder(session)
                .setAvailableSessionCommands(MediaSession.ConnectionResult.DEFAULT_SESSION_AND_LIBRARY_COMMANDS.buildUpon().add(SessionCommand(FADE_COMMAND, Bundle.EMPTY)).build())
                .setMediaButtonPreferences(ImmutableList.of(fadeButton(settings)))
                .build()

        /** The car's crossfade button: the next length of the list. */
        override fun onCustomCommand(session: MediaSession, controller: MediaSession.ControllerInfo, customCommand: SessionCommand, args: Bundle): ListenableFuture<SessionResult> {
            if (customCommand.customAction == FADE_COMMAND) {
                val now = settings.crossfadeSecs
                settings.crossfadeSecs = Settings.PRESETS.firstOrNull { it > now } ?: 0
            }
            return com.google.common.util.concurrent.Futures.immediateFuture(SessionResult(SessionResult.RESULT_SUCCESS))
        }

        /** The person comes back (the car connects, a headset button is pressed): the music as it was left. */
        override fun onPlaybackResumption(mediaSession: MediaSession, controller: MediaSession.ControllerInfo, isForPlayback: Boolean): ListenableFuture<MediaSession.MediaItemsWithStartPosition> =
            work.submit<MediaSession.MediaItemsWithStartPosition> {
                val saved = Resume.load(brain) ?: throw UnsupportedOperationException("nothing was played yet")
                Resume.queue(tree, saved) ?: throw UnsupportedOperationException("the list is gone")
            }

        override fun onGetLibraryRoot(session: MediaLibrarySession, browser: MediaSession.ControllerInfo, params: LibraryParams?): ListenableFuture<LibraryResult<MediaItem>> =
            if (params?.isRecent == true) com.google.common.util.concurrent.Futures.immediateFuture(LibraryResult.ofItem(tree.recentRoot(), params))
            else if (params?.isSuggested == true) com.google.common.util.concurrent.Futures.immediateFuture(LibraryResult.ofItem(tree.suggestedRoot(), params))
            // Always answered, signed in or not: a media app whose root is refused is left out of Android Auto's list of apps (and
            // is not offered as a shortcut), and it is not asked again when the person signs in. Signed out, the root holds one
            // line that says what to do (see `Tree.children`).
            else com.google.common.util.concurrent.Futures.immediateFuture(LibraryResult.ofItem(tree.root(), params))

        override fun onGetChildren(session: MediaLibrarySession, browser: MediaSession.ControllerInfo, parentId: String, page: Int, pageSize: Int, params: LibraryParams?): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> =
            if (parentId == Tree.ROOT && !this.session.signedIn) com.google.common.util.concurrent.Futures.immediateFuture(LibraryResult.ofItemList(ImmutableList.of(tree.signInNotice()), params))
            else asynchronous {
                val all = tree.children(parentId)
                val from = (page * pageSize).coerceAtMost(all.size)
                LibraryResult.ofItemList(ImmutableList.copyOf(all.subList(from, (from + pageSize).coerceAtMost(all.size))), params)
            }

        override fun onGetItem(session: MediaLibrarySession, browser: MediaSession.ControllerInfo, mediaId: String): ListenableFuture<LibraryResult<MediaItem>> =
            asynchronous { tree.item(mediaId)?.let { LibraryResult.ofItem(it, null) } ?: LibraryResult.ofError(LibraryResult.RESULT_ERROR_BAD_VALUE) }

        override fun onSearch(session: MediaLibrarySession, browser: MediaSession.ControllerInfo, query: String, params: LibraryParams?): ListenableFuture<LibraryResult<Void>> =
            asynchronous {
                session.notifySearchResultChanged(browser, query, tree.search(query).size, params)
                LibraryResult.ofVoid()
            }

        override fun onGetSearchResult(session: MediaLibrarySession, browser: MediaSession.ControllerInfo, query: String, page: Int, pageSize: Int, params: LibraryParams?): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> =
            asynchronous {
                val all = tree.search(query)
                val from = (page * pageSize).coerceAtMost(all.size)
                LibraryResult.ofItemList(ImmutableList.copyOf(all.subList(from, (from + pageSize).coerceAtMost(all.size))), params)
            }

        /** A song chosen in a list plays that list from there; words said to the assistant play what they find. */
        override fun onSetMediaItems(mediaSession: MediaSession, controller: MediaSession.ControllerInfo, mediaItems: MutableList<MediaItem>, startIndex: Int, startPositionMs: Long): ListenableFuture<MediaSession.MediaItemsWithStartPosition> =
            work.submit<MediaSession.MediaItemsWithStartPosition> {
                val first = mediaItems.getOrNull(startIndex.coerceAtLeast(0))
                val words = first?.requestMetadata?.searchQuery
                val queue = when {
                    first == null -> null
                    first.mediaId.startsWith("s:") -> tree.queue(first.mediaId)
                    !words.isNullOrBlank() -> tree.search(words).takeIf { it.isNotEmpty() }?.let { Queue(it, 0, false) }
                    else -> null
                }
                if (queue != null) {
                    // A list plays in order; only a mix plays shuffled.
                    main.post { player.shuffleModeEnabled = queue.shuffled }
                    MediaSession.MediaItemsWithStartPosition(queue.items, queue.start, C.TIME_UNSET)
                } else {
                    MediaSession.MediaItemsWithStartPosition(mediaItems.map { tree.resolve(it) }, startIndex, startPositionMs)
                }
            }

        override fun onAddMediaItems(mediaSession: MediaSession, controller: MediaSession.ControllerInfo, mediaItems: MutableList<MediaItem>): ListenableFuture<MutableList<MediaItem>> =
            work.submit<MutableList<MediaItem>> { mediaItems.map { tree.resolve(it) }.toMutableList() }
    }

    companion object {
        /** The session command of the crossfade button on the car's player. */
        const val FADE_COMMAND = "songbook.crossfade"

        /** Told (on the main thread) each time a song was written to the history: the screens' "recently played" asks again. */
        @Volatile
        var onPlayed: (() -> Unit)? = null

    }
}
