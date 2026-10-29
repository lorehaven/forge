package dev.lorehaven.hoist

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Rect
import android.os.Build
import android.os.Bundle
import android.text.InputType
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.WindowInsets
import android.view.View
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityNodeProvider
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import java.nio.ByteBuffer

/**
 * The screen of an app drawn by its Rust program (see [SurfaceHost]): frames come in and are drawn, touches and
 * keys go out, the soft keyboard types into the program's text field through an [InputConnection], and the
 * program's accessibility tree is what TalkBack and other services read, through an [AccessibilityNodeProvider].
 */
class QuenchSurfaceView(context: Context) : SurfaceView(context), SurfaceHolder.Callback, SurfaceHost.Listener {
    private val host = SurfaceHost(context, this)
    private var bitmap: Bitmap? = null
    @Volatile private var surfaceReady = false

    // What the input method needs to know about the text field.
    private var imeText = ""
    private var imeCursor = 0
    private var imeAnchor = 0
    private var composing: String? = null

    private var tree: List<Node> = emptyList()
    private val provider = TreeProvider()

    /** The way to the app's plugins (set by the activity): the program's calls go to it. */
    var bridge: Bridge? = null

    private val clipboardManager = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
    private var clipboardFromProgram: String? = null
    private val clipboardListener = ClipboardManager.OnPrimaryClipChangedListener { sendClipboard() }

    /** Tells the program what the system clipboard holds (unless it is what the program just put there). */
    private fun sendClipboard() {
        val text = try {
            clipboardManager.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(context)?.toString()
        } catch (_: SecurityException) {
            null // the clipboard is not readable while the app is in the background
        } ?: return
        if (text != clipboardFromProgram) host.clipboard(text)
    }

    /** A plugin's answer or event for the program. */
    fun reply(json: String) = host.reply(json)

    /** Called when the program says the app is finished. */
    var exitHandler: () -> Unit = {}

    init {
        holder.addCallback(this)
        isFocusable = true
        isFocusableInTouchMode = true
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_YES
    }

    fun start() {
        host.start()
        clipboardManager.addPrimaryClipChangedListener(clipboardListener)
    }

    fun stop() {
        clipboardManager.removePrimaryClipChangedListener(clipboardListener)
        host.stop()
    }

    fun setVisible(shown: Boolean) {
        host.visible(shown)
        if (shown) post { sendClipboard() }
    }

    fun setDark(dark: Boolean) = host.scheme(dark)

    fun back() = host.back()

    // ---- the surface -------------------------------------------------------------------------------------

    override fun surfaceCreated(holder: SurfaceHolder) {
        surfaceReady = true
    }

    /** The system bars and the keyboard, in pixels: top, right, bottom, left. The program sees them as `env(safe-area-inset-*)`. */
    private var insets = floatArrayOf(0f, 0f, 0f, 0f)

    init {
        setOnApplyWindowInsetsListener { _, windowInsets ->
            insets = if (Build.VERSION.SDK_INT >= 30) {
                val bars = windowInsets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.ime())
                floatArrayOf(bars.top.toFloat(), bars.right.toFloat(), bars.bottom.toFloat(), bars.left.toFloat())
            } else {
                @Suppress("DEPRECATION")
                floatArrayOf(
                    windowInsets.systemWindowInsetTop.toFloat(), windowInsets.systemWindowInsetRight.toFloat(),
                    windowInsets.systemWindowInsetBottom.toFloat(), windowInsets.systemWindowInsetLeft.toFloat(),
                )
            }
            if (width > 0) host.resize(width, height, resources.displayMetrics.density, insets)
            windowInsets
        }
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        host.resize(width, height, resources.displayMetrics.density, insets)
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        surfaceReady = false
    }

    override fun onFrame(width: Int, height: Int, rgba: ByteArray) {
        if (!surfaceReady) return
        var bmp = bitmap
        if (bmp == null || bmp.width != width || bmp.height != height) {
            bmp = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
            bitmap = bmp
        }
        bmp.copyPixelsFromBuffer(ByteBuffer.wrap(rgba))
        val canvas = holder.lockCanvas() ?: return
        try {
            canvas.drawBitmap(bmp, 0f, 0f, null)
        } finally {
            holder.unlockCanvasAndPost(canvas)
        }
    }

    // ---- touch and keys ----------------------------------------------------------------------------------

    override fun onTouchEvent(event: MotionEvent): Boolean {
        val action = when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> 0
            MotionEvent.ACTION_MOVE -> 1
            MotionEvent.ACTION_UP -> 2
            MotionEvent.ACTION_CANCEL -> 3
            else -> return true
        }
        // One finger: the first one.
        host.touch(action, event.getX(0), event.getY(0))
        return true
    }

    private fun mods(event: KeyEvent): Int =
        (if (event.isShiftPressed) 1 else 0) or (if (event.isCtrlPressed) 2 else 0) or
            (if (event.isAltPressed) 4 else 0) or (if (event.isMetaPressed) 8 else 0)

    /** The DOM `key` value of a key event, or null for keys the program has no use for. */
    private fun keyName(event: KeyEvent): String? = when (event.keyCode) {
        KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER -> "Enter"
        KeyEvent.KEYCODE_DEL -> "Backspace"
        KeyEvent.KEYCODE_FORWARD_DEL -> "Delete"
        KeyEvent.KEYCODE_TAB -> "Tab"
        KeyEvent.KEYCODE_ESCAPE -> "Escape"
        KeyEvent.KEYCODE_DPAD_LEFT -> "ArrowLeft"
        KeyEvent.KEYCODE_DPAD_RIGHT -> "ArrowRight"
        KeyEvent.KEYCODE_DPAD_UP -> "ArrowUp"
        KeyEvent.KEYCODE_DPAD_DOWN -> "ArrowDown"
        KeyEvent.KEYCODE_MOVE_HOME -> "Home"
        KeyEvent.KEYCODE_MOVE_END -> "End"
        KeyEvent.KEYCODE_PAGE_UP -> "PageUp"
        KeyEvent.KEYCODE_PAGE_DOWN -> "PageDown"
        KeyEvent.KEYCODE_SPACE -> " "
        KeyEvent.KEYCODE_BACK -> null
        else -> event.unicodeChar.takeIf { it > 0 && (it and KeyCharacterMapCombining) == 0 }?.let { String(Character.toChars(it)) }
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        val name = keyName(event) ?: return super.onKeyDown(keyCode, event)
        host.key(true, mods(event), name)
        return true
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        val name = keyName(event) ?: return super.onKeyUp(keyCode, event)
        host.key(false, mods(event), name)
        return true
    }

    // ---- the soft keyboard -------------------------------------------------------------------------------

    override fun onKeyboard(show: Boolean) {
        post {
            val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
            if (show) {
                requestFocus()
                imm.restartInput(this)
                imm.showSoftInput(this, 0)
            } else {
                composing = null
                imm.hideSoftInputFromWindow(windowToken, 0)
            }
        }
    }

    override fun onIme(text: String, cursor: Int, anchor: Int, rect: FloatArray) {
        imeText = text
        imeCursor = cursor
        imeAnchor = anchor
        post {
            val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
            imm.updateSelection(this, anchor, cursor, -1, -1)
        }
    }

    override fun onCheckIsTextEditor(): Boolean = true

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection {
        outAttrs.inputType = InputType.TYPE_CLASS_TEXT
        outAttrs.imeOptions = EditorInfo.IME_ACTION_DONE or EditorInfo.IME_FLAG_NO_FULLSCREEN
        outAttrs.initialSelStart = imeAnchor
        outAttrs.initialSelEnd = imeCursor
        return Connection(this)
    }

    /** What the soft keyboard types into: every change goes to the program, which owns the text. */
    private inner class Connection(view: View) : BaseInputConnection(view, false) {
        override fun commitText(text: CharSequence, newCursorPosition: Int): Boolean {
            composing = null
            host.text(text.toString())
            return true
        }

        override fun setComposingText(text: CharSequence, newCursorPosition: Int): Boolean {
            composing = text.toString()
            host.compose(text.toString(), text.length)
            return true
        }

        override fun finishComposingText(): Boolean {
            val pending = composing
            composing = null
            if (pending != null) host.text(pending)
            return true
        }

        override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
            host.delete(beforeLength, afterLength)
            return true
        }

        override fun sendKeyEvent(event: KeyEvent): Boolean {
            val name = keyName(event) ?: return false
            host.key(event.action == KeyEvent.ACTION_DOWN, mods(event), name)
            return true
        }

        override fun performEditorAction(actionCode: Int): Boolean {
            host.key(true, 0, "Enter")
            host.key(false, 0, "Enter")
            return true
        }

        override fun getTextBeforeCursor(n: Int, flags: Int): CharSequence =
            imeText.substring(0, imeCursor.coerceIn(0, imeText.length)).takeLast(n)

        override fun getTextAfterCursor(n: Int, flags: Int): CharSequence =
            imeText.substring(imeCursor.coerceIn(0, imeText.length)).take(n)

        override fun getSelectedText(flags: Int): CharSequence? {
            val a = minOf(imeCursor, imeAnchor).coerceIn(0, imeText.length)
            val b = maxOf(imeCursor, imeAnchor).coerceIn(0, imeText.length)
            return if (a == b) null else imeText.substring(a, b)
        }
    }

    // ---- accessibility -----------------------------------------------------------------------------------

    /** One node of the program's accessibility tree. */
    private class Node(
        val id: Int,
        val parent: Int,
        val role: Int,
        val flags: Int,
        val level: Int,
        val bounds: FloatArray,
        val name: String,
        val value: String,
        val description: String,
        /** The smallest, largest and current value of a slider or progress bar, or null. */
        val range: FloatArray?,
    ) {
        val children = ArrayList<Int>()
        fun flag(bit: Int) = (flags and (1 shl bit)) != 0
    }

    override fun onTree(bytes: ByteArray) {
        val parsed = parseTree(bytes)
        post {
            tree = parsed
            if (isAttachedToWindow) sendAccessibilityEvent(AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED)
        }
    }

    private fun parseTree(bytes: ByteArray): List<Node> {
        val b = ByteBuffer.wrap(bytes)
        val count = b.int
        val nodes = ArrayList<Node>(count)
        fun text(): String {
            val n = b.short.toInt() and 0xffff
            val raw = ByteArray(n)
            b.get(raw)
            return String(raw, Charsets.UTF_8)
        }
        repeat(count) {
            val id = b.int
            val parent = b.int
            val role = b.get().toInt()
            val flags = b.short.toInt() and 0xffff
            val level = b.get().toInt()
            val bounds = FloatArray(4) { b.float }
            val name = text()
            val value = text()
            val description = text()
            val range = if (b.get().toInt() != 0) FloatArray(3) { b.float } else null
            nodes.add(Node(id, parent, role, flags, level, bounds, name, value, description, range))
        }
        val byId = nodes.associateBy { it.id }
        for (n in nodes) byId[n.parent]?.children?.add(n.id)
        return nodes
    }

    override fun getAccessibilityNodeProvider(): AccessibilityNodeProvider = provider

    private fun className(role: Int): String = when (role) {
        4, 12 -> "android.widget.Button"
        6 -> "android.widget.CheckBox"
        7 -> "android.widget.RadioButton"
        8 -> "android.widget.Switch"
        9 -> "android.widget.EditText"
        10 -> "android.widget.ListView"
        19 -> "android.widget.ImageView"
        22 -> "android.widget.SeekBar"
        23 -> "android.widget.ProgressBar"
        else -> "android.widget.TextView"
    }

    private inner class TreeProvider : AccessibilityNodeProvider() {
        private val origin = IntArray(2)

        private fun find(id: Int): Node? = tree.firstOrNull { it.id == id }

        override fun createAccessibilityNodeInfo(virtualViewId: Int): AccessibilityNodeInfo? {
            val info = if (virtualViewId == View.NO_ID) {
                AccessibilityNodeInfo.obtain(this@QuenchSurfaceView).also {
                    this@QuenchSurfaceView.onInitializeAccessibilityNodeInfo(it)
                    tree.firstOrNull { n -> n.parent == -1 }?.let { root -> it.addChild(this@QuenchSurfaceView, root.id) }
                }
            } else {
                val node = find(virtualViewId) ?: return null
                AccessibilityNodeInfo.obtain(this@QuenchSurfaceView, virtualViewId).also { fill(it, node) }
            }
            info.packageName = context.packageName
            return info
        }

        private fun fill(info: AccessibilityNodeInfo, n: Node) {
            val view = this@QuenchSurfaceView
            info.className = className(n.role)
            info.setParent(view, if (n.parent == -1) View.NO_ID else n.parent)
            // The document is the root; its parent is the view itself.
            if (n.parent == -1) info.setParent(view)
            for (c in n.children) info.addChild(view, c)
            val label = if (n.value.isNotEmpty() && n.role == 9) n.value else n.name
            info.text = label
            if (n.role == 9 && n.name.isNotEmpty()) info.hintText = n.name
            if (n.description.isNotEmpty()) info.contentDescription = n.description
            view.getLocationOnScreen(origin)
            val screen = Rect(
                origin[0] + n.bounds[0].toInt(), origin[1] + n.bounds[1].toInt(),
                origin[0] + (n.bounds[0] + n.bounds[2]).toInt(), origin[1] + (n.bounds[1] + n.bounds[3]).toInt(),
            )
            info.setBoundsInScreen(screen)
            info.setBoundsInParent(Rect(0, 0, screen.width(), screen.height()))
            info.isVisibleToUser = true
            info.isEnabled = !n.flag(2)
            info.isFocusable = n.flag(0)
            info.isFocused = n.flag(1)
            info.isClickable = n.flag(11)
            info.isCheckable = n.flag(3)
            info.isChecked = n.flag(4)
            info.isSelected = n.flag(6)
            info.isEditable = n.role == 9 && !n.flag(10)
            if (Build.VERSION.SDK_INT >= 28) info.isHeading = n.role == 1
            n.range?.let {
                // A progress bar that has no value yet is reported as at its start.
                val now = if (it[2].isNaN()) it[0] else it[2]
                info.rangeInfo = AccessibilityNodeInfo.RangeInfo.obtain(AccessibilityNodeInfo.RangeInfo.RANGE_TYPE_FLOAT, it[0], it[1], now)
            }
            if (n.flag(11)) info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_CLICK)
            if (n.flag(12)) info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_FOCUS)
            info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_ACCESSIBILITY_FOCUS)
            info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SHOW_ON_SCREEN)
        }

        override fun performAction(virtualViewId: Int, action: Int, arguments: Bundle?): Boolean {
            if (virtualViewId == View.NO_ID) return this@QuenchSurfaceView.performAccessibilityAction(action, arguments)
            find(virtualViewId) ?: return false
            return when (action) {
                AccessibilityNodeInfo.ACTION_CLICK -> { host.access(virtualViewId, 0); true }
                AccessibilityNodeInfo.ACTION_FOCUS -> { host.access(virtualViewId, 1); true }
                AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS -> {
                    sendEventFor(virtualViewId, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUSED)
                    true
                }
                AccessibilityNodeInfo.AccessibilityAction.ACTION_SHOW_ON_SCREEN.id -> { host.access(virtualViewId, 2); true }
                else -> false
            }
        }

        private fun sendEventFor(id: Int, type: Int) {
            val event = AccessibilityEvent.obtain(type)
            event.setSource(this@QuenchSurfaceView, id)
            event.packageName = context.packageName
            this@QuenchSurfaceView.parent?.requestSendAccessibilityEvent(this@QuenchSurfaceView, event)
        }
    }

    // ---- the rest of the listener ------------------------------------------------------------------------

    override fun onBackHandled() {}

    override fun onClipboard(text: String) {
        post {
            clipboardFromProgram = text
            clipboardManager.setPrimaryClip(ClipData.newPlainText(null, text))
        }
    }

    override fun onCall(json: String) {
        // Plugins run on the main thread, as they do for a page.
        post { bridge?.handle(json) }
    }

    override fun onExit() {
        post { exitHandler() }
    }

    override fun onClosed(reason: String) {
        android.util.Log.w("quench-surface", reason)
    }

    private companion object {
        /** Set in the key character map's result for a dead key (an accent that waits for a letter). */
        const val KeyCharacterMapCombining = 0x80000000.toInt()
    }
}