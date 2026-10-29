package dev.lorehaven.hoist

import android.content.Context
import android.util.Log
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.File
import java.io.IOException
import kotlin.concurrent.thread

/**
 * The app's Rust program that draws the screen (`surface/`, built by `quench android build-native`), run as a
 * process of its own: no JNI, nothing of it is loaded into the app. Its standard input carries what the user does
 * (touches, keys, text); its standard output carries what to show (frames, the soft keyboard, the text field the
 * input method types into, the accessibility tree). Messages are a length, a kind and a payload, big-endian.
 */
class SurfaceHost(private val context: Context, private val listener: Listener) {
    interface Listener {
        /** A frame of [width] x [height] pixels: RGBA, premultiplied. */
        fun onFrame(width: Int, height: Int, rgba: ByteArray)
        fun onKeyboard(show: Boolean)
        /** The focused text field: its text, the caret and selection anchor in UTF-16 units, the caret rectangle. */
        fun onIme(text: String, cursor: Int, anchor: Int, rect: FloatArray)
        fun onTree(bytes: ByteArray)
        fun onBackHandled()
        /** Text for the system clipboard. */
        fun onClipboard(text: String)
        /** A call to a plugin: the JSON envelope `{"id","plugin","method","args"}`. */
        fun onCall(json: String)
        fun onExit()
        fun onClosed(reason: String)
    }

    private var process: Process? = null
    private var output: DataOutputStream? = null

    /** Whether the program is part of this build. */
    fun available(): Boolean = File(context.applicationInfo.nativeLibraryDir, PROGRAM).exists()

    /** Starts the program; returns false (and says why to the listener) when it cannot. */
    fun start(): Boolean {
        val file = File(context.applicationInfo.nativeLibraryDir, PROGRAM)
        if (!file.exists()) {
            listener.onClosed("the surface program is not in this build")
            return false
        }
        val started = try {
            // The program lives in the app's own files: its working directory and `HOME` are there, so what it writes
            // (a session, a cache, a database) goes where the system looks after it.
            ProcessBuilder(file.absolutePath)
                .directory(context.filesDir)
                .apply {
                    environment()["HOME"] = context.filesDir.absolutePath
                    // Where frames spend their time, in the log once a second (debug builds only).
                    if (BuildConfig.DEBUG) {
                        environment()["QUENCH_FRAME_STATS"] = "1"
                        // What the app asked of the network and what came back (paths and statuses only).
                        environment()["HOIST_TRACE"] = "1"
                    }
                }
                .start()
        } catch (e: IOException) {
            listener.onClosed("cannot start the surface program: ${e.message}")
            return false
        }
        process = started
        output = DataOutputStream(started.outputStream.buffered(1 shl 16))
        thread(name = "surface-stderr") {
            try {
                started.errorStream.bufferedReader().forEachLine { Log.w(TAG, it) }
            } catch (_: IOException) {
            }
        }
        thread(name = "surface-reader") {
            val reason = try {
                readLoop(DataInputStream(started.inputStream.buffered(1 shl 16)))
                "the surface program ended"
            } catch (e: IOException) {
                "the surface program ended: ${e.message}"
            }
            listener.onClosed(reason)
        }
        return true
    }

    private var frame = ByteArray(0)

    private fun readLoop(input: DataInputStream) {
        while (true) {
            val length = input.readInt()
            if (length < 1 || length > MAX_MESSAGE) throw IOException("a message of $length bytes")
            val kind = input.readUnsignedByte()
            when (kind) {
                1 -> {
                    val width = input.readInt()
                    val height = input.readInt()
                    // One buffer for every frame of this size: a new array of megabytes per frame is what makes the
                    // garbage collector stutter on a phone.
                    if (frame.size != length - 9) frame = ByteArray(length - 9)
                    input.readFully(frame)
                    listener.onFrame(width, height, frame)
                }
                2 -> listener.onKeyboard(input.readUnsignedByte() != 0)
                3 -> {
                    val textBytes = ByteArray(input.readInt())
                    input.readFully(textBytes)
                    val cursor = input.readInt()
                    val anchor = input.readInt()
                    val rect = FloatArray(4) { input.readFloat() }
                    listener.onIme(String(textBytes, Charsets.UTF_8), cursor, anchor, rect)
                }
                4 -> {
                    val tree = ByteArray(length - 1)
                    input.readFully(tree)
                    listener.onTree(tree)
                }
                5 -> listener.onBackHandled()
                6 -> {
                    listener.onExit()
                    return
                }
                7, 8 -> {
                    val bytes = ByteArray(input.readInt())
                    input.readFully(bytes)
                    val text = String(bytes, Charsets.UTF_8)
                    if (kind == 7) listener.onClipboard(text) else listener.onCall(text)
                }
                else -> input.skipBytes(length - 1)
            }
        }
    }

    private fun send(kind: Int, payload: DataOutputStream.() -> Unit) {
        val out = output ?: return
        synchronized(out) {
            try {
                // The length is known only after the payload is written: build it first.
                val body = java.io.ByteArrayOutputStream()
                val data = DataOutputStream(body)
                data.writeByte(kind)
                data.payload()
                data.flush()
                out.writeInt(body.size())
                body.writeTo(out)
                out.flush()
            } catch (_: IOException) {
                // The program is gone; the reader reports it.
            }
        }
    }

    private fun DataOutputStream.writeText(text: String) {
        val bytes = text.toByteArray(Charsets.UTF_8).let { if (it.size > 65535) it.copyOf(65535) else it }
        writeShort(bytes.size)
        write(bytes)
    }

    fun resize(width: Int, height: Int, density: Float, insets: FloatArray) = send(1) {
        writeInt(width)
        writeInt(height)
        writeFloat(density)
        for (i in insets) writeFloat(i)
    }

    /** [action]: 0 down, 1 move, 2 up, 3 cancel. */
    fun touch(action: Int, x: Float, y: Float) = send(2) {
        writeByte(action)
        writeFloat(x)
        writeFloat(y)
    }

    /** [mods]: 1 shift, 2 control, 4 alt, 8 meta. */
    fun key(down: Boolean, mods: Int, name: String) = send(3) {
        writeByte(if (down) 1 else 0)
        writeByte(mods)
        writeText(name)
    }

    fun text(text: String) = send(4) { writeText(text) }

    fun compose(text: String, cursor: Int) = send(5) {
        writeText(text)
        writeInt(cursor)
    }

    fun delete(before: Int, after: Int) = send(6) {
        writeInt(before)
        writeInt(after)
    }

    /** [action]: 0 click, 1 focus, 2 scroll into view. */
    fun access(id: Int, action: Int) = send(7) {
        writeInt(id)
        writeByte(action)
    }

    fun scheme(dark: Boolean) = send(8) { writeByte(if (dark) 1 else 0) }

    fun back() = send(9) {}

    fun visible(shown: Boolean) = send(10) { writeByte(if (shown) 1 else 0) }

    /** The system clipboard's text, when it changes and when the app comes back to the front. */
    fun clipboard(text: String) = send(11) { writeLong(text) }

    /** A plugin's answer or event (a JSON envelope). */
    fun reply(json: String) = send(12) { writeLong(json) }

    private fun DataOutputStream.writeLong(text: String) {
        val bytes = text.toByteArray(Charsets.UTF_8)
        writeInt(bytes.size)
        write(bytes)
    }

    fun stop() {
        try {
            output?.close()
        } catch (_: IOException) {
        }
        process?.destroy()
        process = null
        output = null
    }

    companion object {
        private const val PROGRAM = "libquench_surface.so"
        private const val TAG = "quench-surface"
        private const val MAX_MESSAGE = 1 shl 26
    }
}