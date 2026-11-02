package dev.lorehaven.songbook

import android.app.Activity
import android.content.ComponentName
import android.content.Intent
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.EditorInfo
import android.widget.BaseAdapter
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.ScrollView
import android.widget.SeekBar
import android.widget.TextView
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaBrowser
import androidx.media3.session.SessionToken
import java.util.concurrent.Executor

/**
 * The phone's screens, over the same media session the car uses: browse the library (the very tree the car shows) and search it,
 * the mini player, the player, and the settings (crossfade, the songs kept on the device).
 */
class Phone(private val activity: Activity, private val onSignedOut: () -> Unit) {
    private val session = Session.of(activity)
    private val offline = Offline.of(activity)
    private val images = Images(activity)
    private val settings = Settings.of(activity)
    private val handler = Handler(Looper.getMainLooper())
    private val onMain = Executor { handler.post(it) }
    private var browser: MediaBrowser? = null
    private var unsubscribe: (() -> Unit)? = null

    private class Page(val id: String, val title: String)

    private val stack = ArrayDeque<Page>()
    private var items: List<MediaItem> = emptyList()

    // ── The views ───────────────────────────────────────────────────────────

    val root = FrameLayout(activity).apply { setBackgroundColor(Ui.BACK) }

    private val title = Ui.text(activity, "Songbook", 22f, bold = true)
    private val back = Ui.text(activity, "‹", 28f).apply { setPadding(0, 0, Ui.dp(activity, 12), 0); setOnClickListener { back() } }
    private val search = EditText(activity).apply {
        hint = "Search songs, albums, artists"
        setTextColor(Ui.TEXT)
        setHintTextColor(Ui.DIM)
        setSingleLine()
        inputType = InputType.TYPE_CLASS_TEXT
        imeOptions = EditorInfo.IME_ACTION_SEARCH
        setOnEditorActionListener { _, action, event ->
            // The keyboard's search key, or Enter on a hardware keyboard.
            val submitted = action == EditorInfo.IME_ACTION_SEARCH || (event != null && event.keyCode == android.view.KeyEvent.KEYCODE_ENTER && event.action == android.view.KeyEvent.ACTION_DOWN)
            if (submitted) searchNow()
            submitted
        }
    }
    private fun searchNow() {
        val words = search.text.toString().trim()
        if (words.isNotEmpty()) open(Page("search:$words", "Search: $words"))
    }

    private val searchBar = Ui.row(activity).apply {
        addView(search, Ui.weight())
        addView(Ui.button(activity, "Search") { searchNow() })
    }
    private val status = Ui.text(activity, "", 15f, Ui.DIM).apply { gravity = Gravity.CENTER; setPadding(Ui.dp(activity, 24), Ui.dp(activity, 48), Ui.dp(activity, 24), 0) }
    private val keepAll = Ui.button(activity, "Keep all on this device") { keepEverythingShown() }.apply { show(false) }
    private val list = ListView(activity).apply {
        divider = null
        setBackgroundColor(Ui.BACK)
        adapter = object : BaseAdapter() {
            override fun getCount() = items.size
            override fun getItem(i: Int) = items[i]
            override fun getItemId(i: Int) = i.toLong()
            override fun getView(i: Int, convert: View?, parent: ViewGroup): View = rowFor(items[i])
        }
        setOnItemClickListener { _, _, i, _ -> chosen(items[i]) }
    }

    private val miniCover = images.cover(activity, 44)
    private val miniTitle = Ui.text(activity, "", 14f, bold = true).apply { setSingleLine() }
    private val miniArtist = Ui.text(activity, "", 12f, Ui.DIM).apply { setSingleLine() }
    private val miniPlay = icon(android.R.drawable.ic_media_play) { toggle() }
    private val mini = Ui.row(activity).apply {
        setBackgroundColor(Ui.PANEL)
        setPadding(Ui.dp(activity, 12), Ui.dp(activity, 8), Ui.dp(activity, 12), Ui.dp(activity, 8))
        addView(miniCover)
        addView(Ui.column(activity).apply { addView(miniTitle); addView(miniArtist); setPadding(Ui.dp(activity, 12), 0, 0, 0) }, Ui.weight())
        addView(miniPlay)
        addView(icon(android.R.drawable.ic_media_next) { browser?.seekToNext() })
        setOnClickListener { showPlaying(true) }
        show(false)
    }

    private val screen = Ui.column(activity).apply {
        val bar = Ui.row(activity).apply {
            setPadding(Ui.dp(activity, 16), Ui.dp(activity, 8), Ui.dp(activity, 16), Ui.dp(activity, 8))
            addView(back)
            addView(title, Ui.weight())
            addView(Ui.button(activity, "Settings") { showSettings(true) })
        }
        addView(bar)
        addView(searchBar, LinearLayout.LayoutParams(-1, -2).apply { setMargins(Ui.dp(activity, 16), 0, Ui.dp(activity, 16), 0) })
        addView(keepAll, LinearLayout.LayoutParams(-2, -2).apply { setMargins(Ui.dp(activity, 16), Ui.dp(activity, 4), 0, Ui.dp(activity, 8)) })
        addView(FrameLayout(activity).apply { addView(list); addView(status) }, LinearLayout.LayoutParams(-1, 0, 1f))
        addView(mini)
    }

    // The player (full screen)
    private val bigCover = images.cover(activity, 280)
    private val bigTitle = Ui.text(activity, "", 22f, bold = true).apply { gravity = Gravity.CENTER }
    private val bigArtist = Ui.text(activity, "", 15f, Ui.DIM).apply { gravity = Gravity.CENTER }
    private val seek = SeekBar(activity)
    private val position = Ui.text(activity, "0:00", 12f, Ui.DIM)
    private val duration = Ui.text(activity, "0:00", 12f, Ui.DIM)
    private val bigPlay = icon(android.R.drawable.ic_media_play, 64) { toggle() }
    private val shuffle = Ui.button(activity, "Shuffle") { browser?.let { it.shuffleModeEnabled = !it.shuffleModeEnabled } }
    private val repeat = Ui.button(activity, "Repeat") { cycleRepeat() }
    private val keep = Ui.button(activity, "Keep on this device", primary = false) { keepCurrent() }
    private val playing = Ui.column(activity).apply {
        setBackgroundColor(Ui.BACK)
        gravity = Gravity.CENTER_HORIZONTAL
        setPadding(Ui.dp(activity, 24), Ui.dp(activity, 24), Ui.dp(activity, 24), Ui.dp(activity, 24))
        addView(Ui.row(activity).apply { addView(Ui.text(activity, "Now playing", 16f, Ui.DIM), Ui.weight()); addView(Ui.button(activity, "Close") { showPlaying(false) }) })
        addView(bigCover, LinearLayout.LayoutParams(Ui.dp(activity, 280), Ui.dp(activity, 280)).apply { setMargins(0, Ui.dp(activity, 24), 0, Ui.dp(activity, 24)) })
        addView(bigTitle)
        addView(bigArtist)
        addView(Ui.row(activity).apply { addView(position); addView(seek, Ui.weight()); addView(duration) }, LinearLayout.LayoutParams(-1, -2).apply { topMargin = Ui.dp(activity, 16) })
        addView(Ui.row(activity).apply {
            gravity = Gravity.CENTER
            addView(shuffle)
            addView(icon(android.R.drawable.ic_media_previous, 48) { browser?.seekToPrevious() })
            addView(bigPlay)
            addView(icon(android.R.drawable.ic_media_next, 48) { browser?.seekToNext() })
            addView(repeat)
        })
        addView(Ui.row(activity).apply {
            gravity = Gravity.CENTER
            addView(keep)
            addView(Ui.button(activity, "Queue") { showQueue(true) }, LinearLayout.LayoutParams(-2, -2).apply { leftMargin = Ui.dp(activity, 12) })
        }, LinearLayout.LayoutParams(-2, -2).apply { topMargin = Ui.dp(activity, 16) })
        show(false)
    }

    // The queue: what is coming, the song playing marked; a tap goes there, a cross takes one out
    private val queueList = ListView(activity).apply {
        divider = null
        adapter = object : BaseAdapter() {
            override fun getCount() = browser?.mediaItemCount ?: 0
            override fun getItem(i: Int) = browser?.getMediaItemAt(i)
            override fun getItemId(i: Int) = i.toLong()
            override fun getView(i: Int, convert: View?, parent: ViewGroup): View = queueRow(i)
        }
        setOnItemClickListener { _, _, i, _ -> browser?.run { seekToDefaultPosition(i); play() } }
    }
    private val queueView = Ui.column(activity).apply {
        setBackgroundColor(Ui.BACK)
        setPadding(Ui.dp(activity, 24), Ui.dp(activity, 24), Ui.dp(activity, 24), Ui.dp(activity, 8))
        addView(Ui.row(activity).apply { addView(Ui.text(activity, "Queue", 22f, bold = true), Ui.weight()); addView(Ui.button(activity, "Close") { showQueue(false) }) })
        addView(queueList, LinearLayout.LayoutParams(-1, 0, 1f).apply { topMargin = Ui.dp(activity, 12) })
        show(false)
    }

    // The settings
    private val fadeLabel = Ui.text(activity, "", 16f, bold = true)
    private val fade = SeekBar(activity).apply { max = Settings.MAX_FADE }
    private val keptLabel = Ui.text(activity, "", 15f, Ui.DIM)
    private val settingsView = ScrollView(activity).apply {
        setBackgroundColor(Ui.BACK)
        addView(Ui.column(activity).apply {
            setPadding(Ui.dp(activity, 24), Ui.dp(activity, 24), Ui.dp(activity, 24), Ui.dp(activity, 24))
            addView(Ui.row(activity).apply { addView(Ui.text(activity, "Settings", 22f, bold = true), Ui.weight()); addView(Ui.button(activity, "Close") { showSettings(false) }) })
            addView(fadeLabel, LinearLayout.LayoutParams(-1, -2).apply { topMargin = Ui.dp(activity, 24) })
            addView(fade)
            addView(Ui.text(activity, "The next song starts while this one ends, and they overlap this long.", 13f, Ui.DIM))
            addView(Ui.text(activity, "On this device", 16f, bold = true), LinearLayout.LayoutParams(-1, -2).apply { topMargin = Ui.dp(activity, 28) })
            addView(keptLabel)
            addView(Ui.button(activity, "Remove all kept songs") { offline.removeAll() }, LinearLayout.LayoutParams(-2, -2).apply { topMargin = Ui.dp(activity, 8) })
            addView(Ui.text(activity, "Signed in", 16f, bold = true), LinearLayout.LayoutParams(-1, -2).apply { topMargin = Ui.dp(activity, 28) })
            addView(Ui.text(activity, if (session.user.isEmpty()) session.server else "${session.user} on ${session.server}", 15f, Ui.DIM))
            addView(Ui.button(activity, "Open the player in the browser") { activity.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(session.player))) }, LinearLayout.LayoutParams(-2, -2).apply { topMargin = Ui.dp(activity, 8) })
            addView(Ui.button(activity, "Sign out") { signOut() }, LinearLayout.LayoutParams(-2, -2).apply { topMargin = Ui.dp(activity, 8) })
        })
        show(false)
    }

    init {
        root.addView(screen)
        root.addView(playing)
        root.addView(queueView)
        root.addView(settingsView)
        fade.setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
            override fun onProgressChanged(bar: SeekBar, value: Int, fromUser: Boolean) {
                if (fromUser) settings.crossfadeSecs = value
                fadeLabel.text = if (value == 0) "Crossfade: off" else "Crossfade: $value s"
            }
            override fun onStartTrackingTouch(bar: SeekBar) {}
            override fun onStopTrackingTouch(bar: SeekBar) {}
        })
        seek.setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
            override fun onProgressChanged(bar: SeekBar, value: Int, fromUser: Boolean) {
                if (fromUser) browser?.seekTo(value * 1000L)
            }
            override fun onStartTrackingTouch(bar: SeekBar) {}
            override fun onStopTrackingTouch(bar: SeekBar) {}
        })
    }

    private fun icon(resource: Int, size: Int = 40, click: () -> Unit) = ImageView(activity).apply {
        setImageResource(resource)
        setColorFilter(Ui.TEXT)
        setPadding(Ui.dp(activity, 6), Ui.dp(activity, 6), Ui.dp(activity, 6), Ui.dp(activity, 6))
        layoutParams = LinearLayout.LayoutParams(Ui.dp(activity, size + 12), Ui.dp(activity, size + 12))
        setOnClickListener { click() }
    }

    // ── The session ─────────────────────────────────────────────────────────

    fun start() {
        val token = SessionToken(activity, ComponentName(activity, PlaybackService::class.java))
        val future = MediaBrowser.Builder(activity, token).buildAsync()
        future.addListener({
            val b = try { future.get() } catch (e: Exception) { return@addListener status("Cannot start the player.") }
            browser = b
            b.addListener(object : Player.Listener {
                override fun onEvents(player: Player, events: Player.Events) = refreshPlayer()
            })
            unsubscribe = offline.onChange { handler.post { (list.adapter as BaseAdapter).notifyDataSetChanged(); refreshKept() } }
            if (stack.isEmpty()) open(Page(Tree.ROOT, "Songbook")) else load(stack.last())
            refreshPlayer()
            handler.post(ticker)
        }, onMain)
    }

    fun stop() {
        handler.removeCallbacks(ticker)
        unsubscribe?.invoke()
        browser?.release()
        browser = null
    }

    private val ticker = object : Runnable {
        override fun run() {
            progress()
            handler.postDelayed(this, 500)
        }
    }

    // ── The library ─────────────────────────────────────────────────────────

    private fun open(page: Page) {
        stack.addLast(page)
        load(page)
    }

    /** Back one step: the box that is open, else the list before. False when there is nothing to go back to. */
    fun back(): Boolean {
        if (settingsView.visibility == View.VISIBLE) { showSettings(false); return true }
        if (queueView.visibility == View.VISIBLE) { showQueue(false); return true }
        if (playing.visibility == View.VISIBLE) { showPlaying(false); return true }
        if (stack.size > 1) {
            stack.removeLast()
            load(stack.last())
            return true
        }
        return false
    }

    private fun load(page: Page) {
        val b = browser ?: return
        title.text = page.title
        back.show(stack.size > 1)
        searchBar.show(stack.size == 1)
        items = emptyList()
        (list.adapter as BaseAdapter).notifyDataSetChanged()
        keepAll.show(false)
        status("Loading…")
        if (page.id.startsWith("search:")) {
            // The search is made first; its results are asked for when it has answered.
            val q = page.id.removePrefix("search:")
            val searching = b.search(q, null)
            searching.addListener({
                val results = b.getSearchResult(q, 0, 100, null)
                results.addListener({ shown(page, results.get()) }, onMain)
            }, onMain)
        } else {
            val children = b.getChildren(page.id, 0, 500, null)
            children.addListener({ shown(page, children.get()) }, onMain)
        }
    }

    private fun shown(page: Page, result: LibraryResult<com.google.common.collect.ImmutableList<MediaItem>>) {
        if (stack.lastOrNull() !== page) return
        when (result.resultCode) {
            LibraryResult.RESULT_SUCCESS -> {
                items = result.value ?: emptyList()
                (list.adapter as BaseAdapter).notifyDataSetChanged()
                status(if (items.isEmpty()) "Nothing here." else "")
                refreshKept()
            }
            LibraryResult.RESULT_ERROR_SESSION_AUTHENTICATION_EXPIRED -> {
                status("Your session ended. Sign in again.")
                signOut()
            }
            else -> status("Cannot reach the server. Songs kept on this device still play (see “On this device”).")
        }
    }

    private fun status(text: String) {
        status.text = text
        status.show(text.isNotEmpty())
    }

    private fun chosen(item: MediaItem) {
        if (item.mediaMetadata.isBrowsable == true) {
            open(Page(item.mediaId, item.mediaMetadata.title?.toString() ?: ""))
        } else {
            browser?.run { setMediaItem(item); prepare(); play() }
        }
    }

    private fun rowFor(item: MediaItem): View {
        val m = item.mediaMetadata
        val song = Tree.songOf(item)
        return Ui.row(activity).apply {
            setPadding(Ui.dp(activity, 16), Ui.dp(activity, 8), Ui.dp(activity, 16), Ui.dp(activity, 8))
            val cover = images.cover(activity, 52)
            images.load(m.artworkUri, cover)
            addView(cover)
            addView(Ui.column(activity).apply {
                setPadding(Ui.dp(activity, 12), 0, Ui.dp(activity, 8), 0)
                addView(Ui.text(activity, m.title?.toString() ?: "", 15f, bold = true).apply { setSingleLine() })
                val sub = if (song != null) listOfNotNull(m.artist?.toString(), m.albumTitle?.toString()).filter { it.isNotEmpty() }.joinToString(" · ") else m.subtitle?.toString() ?: ""
                if (sub.isNotEmpty()) addView(Ui.text(activity, sub, 13f, Ui.DIM).apply { setSingleLine() })
            }, Ui.weight())
            if (song != null) {
                val state = offline.state(song.id)
                addView(Ui.text(activity, when (state) { null -> "Keep"; 100 -> "On device"; else -> "$state%" }, 13f, if (state == 100) Ui.ACCENT else Ui.DIM, bold = true).apply {
                    setPadding(Ui.dp(activity, 12), Ui.dp(activity, 12), Ui.dp(activity, 4), Ui.dp(activity, 12))
                    setOnClickListener { if (state == null) offline.keep(listOf(song)) else offline.remove(listOf(song)) }
                })
            } else {
                addView(Ui.text(activity, "›", 22f, Ui.DIM))
            }
        }
    }

    private fun refreshKept() {
        val songs = items.mapNotNull { Tree.songOf(it) }
        val rest = songs.count { offline.state(it.id) == null }
        keepAll.text = if (rest == 0) "All on this device" else "Keep all on this device ($rest)"
        keepAll.show(songs.isNotEmpty() && stack.lastOrNull()?.id != "kept")
        val kept = offline.kept()
        keptLabel.text = "${kept.size} ${if (kept.size == 1) "song" else "songs"} kept, ${"%.0f".format(offline.bytes() / 1_048_576.0)} MB"
    }

    private fun keepEverythingShown() {
        offline.keep(items.mapNotNull { Tree.songOf(it) }.take(300))
        refreshKept()
    }

    // ── The player ──────────────────────────────────────────────────────────

    private fun toggle() {
        browser?.let { if (it.isPlaying) it.pause() else it.play() }
    }

    private fun cycleRepeat() {
        browser?.let {
            it.repeatMode = when (it.repeatMode) {
                Player.REPEAT_MODE_OFF -> Player.REPEAT_MODE_ALL
                Player.REPEAT_MODE_ALL -> Player.REPEAT_MODE_ONE
                else -> Player.REPEAT_MODE_OFF
            }
        }
    }

    private fun keepCurrent() {
        val song = browser?.currentMediaItem?.let { Tree.songOf(it) } ?: return
        if (offline.state(song.id) == null) offline.keep(listOf(song)) else offline.remove(listOf(song))
        handler.postDelayed({ refreshPlayer() }, 300)
    }

    private fun refreshPlayer() {
        val b = browser ?: return
        val item = b.currentMediaItem
        mini.show(item != null && playing.visibility != View.VISIBLE)
        if (item == null) return
        val m = item.mediaMetadata
        miniTitle.text = m.title
        miniArtist.text = m.artist
        bigTitle.text = m.title
        bigArtist.text = m.artist
        images.load(m.artworkUri, miniCover)
        images.load(m.artworkUri, bigCover)
        val resource = if (b.isPlaying) android.R.drawable.ic_media_pause else android.R.drawable.ic_media_play
        miniPlay.setImageResource(resource)
        bigPlay.setImageResource(resource)
        shuffle.setTextColor(if (b.shuffleModeEnabled) Ui.ACCENT else Ui.TEXT)
        repeat.text = when (b.repeatMode) { Player.REPEAT_MODE_ONE -> "Repeat 1"; else -> "Repeat" }
        repeat.setTextColor(if (b.repeatMode != Player.REPEAT_MODE_OFF) Ui.ACCENT else Ui.TEXT)
        val song = Tree.songOf(item)
        keep.text = when (song?.let { offline.state(it.id) }) { null -> "Keep on this device"; 100 -> "On this device ✓"; else -> "Keeping…" }
        if (queueView.visibility == View.VISIBLE) (queueList.adapter as BaseAdapter).notifyDataSetChanged()
        progress()
    }

    private fun progress() {
        val b = browser ?: return
        val total = (b.duration.takeIf { it > 0 } ?: 0L) / 1000
        seek.max = total.toInt().coerceAtLeast(1)
        seek.progress = (b.currentPosition / 1000).toInt()
        position.text = clock(b.currentPosition / 1000)
        duration.text = clock(total)
    }

    private fun clock(seconds: Long) = "%d:%02d".format(seconds / 60, seconds % 60)

    private fun showPlaying(on: Boolean) {
        playing.show(on)
        refreshPlayer()
    }

    private fun showQueue(on: Boolean) {
        queueView.show(on)
        (queueList.adapter as BaseAdapter).notifyDataSetChanged()
        if (on) queueList.setSelection(browser?.currentMediaItemIndex ?: 0)
    }

    private fun queueRow(i: Int): View {
        val b = browser
        val item = b?.getMediaItemAt(i)
        val current = b != null && i == b.currentMediaItemIndex
        return Ui.row(activity).apply {
            setPadding(0, Ui.dp(activity, 8), 0, Ui.dp(activity, 8))
            addView(Ui.column(activity).apply {
                addView(Ui.text(activity, item?.mediaMetadata?.title?.toString() ?: "", 15f, if (current) Ui.ACCENT else Ui.TEXT, bold = true).apply { setSingleLine() })
                addView(Ui.text(activity, item?.mediaMetadata?.artist?.toString() ?: "", 13f, Ui.DIM).apply { setSingleLine() })
            }, Ui.weight())
            if (!current) {
                addView(Ui.text(activity, "✕", 18f, Ui.DIM).apply {
                    setPadding(Ui.dp(activity, 16), Ui.dp(activity, 8), Ui.dp(activity, 4), Ui.dp(activity, 8))
                    setOnClickListener { browser?.removeMediaItem(i) }
                })
            }
        }
    }

    private fun showSettings(on: Boolean) {
        settingsView.show(on)
        if (on) {
            fade.progress = settings.crossfadeSecs
            fadeLabel.text = if (settings.crossfadeSecs == 0) "Crossfade: off" else "Crossfade: ${settings.crossfadeSecs} s"
            refreshKept()
        }
    }

    private fun signOut() {
        browser?.run { pause(); clearMediaItems() }
        session.signOut()
        onSignedOut()
    }
}
