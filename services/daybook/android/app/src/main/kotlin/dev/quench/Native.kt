package dev.quench

import android.content.Context
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.File
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread

/**
 * The app's native code (`native/`, built by `quench android build-native`): a small Rust program
 * that runs operations registered in it by name, with bytes in and bytes out.
 *
 * It is a separate process, started from the app's native library directory (the one place Android
 * lets an app run its own code from): one request on its standard input, one answer on its standard
 * output, as length-prefixed frames. There is no JNI and nothing is loaded into the app, so a
 * crash or a runaway in native code ends that process and not the app, and the same call works from
 * any thread and from a background job with no page. Starting it costs a few milliseconds.
 */
object Native {
    private const val PROGRAM = "libquench_native.so"
    private const val MAX_ERROR = 4096

    private fun program(context: Context) = File(context.applicationInfo.nativeLibraryDir, PROGRAM)

    /** Whether the native program is part of this build. */
    fun available(context: Context): Boolean = program(context).exists()

    /** Runs the operation [op]; throws [NativeException] with its message when it fails. */
    fun invoke(context: Context, op: String, input: ByteArray = ByteArray(0), timeoutSeconds: Long = 60): ByteArray {
        val file = program(context)
        if (!file.exists()) throw NativeException("the native program is not in this build")
        val process = try {
            ProcessBuilder(file.absolutePath).start()
        } catch (e: IOException) {
            throw NativeException("cannot start the native program: ${e.message}")
        }
        // What it says on standard error is the reason when it dies without an answer.
        val errors = ByteArrayOutputStream()
        val reader = thread {
            // Destroying the process closes this stream under a read: that is not an error here, and an
            // exception that escapes a thread ends the whole app.
            try {
                process.errorStream.use { s ->
                    val buffer = ByteArray(1024)
                    while (true) {
                        val n = s.read(buffer)
                        if (n < 0) break
                        if (errors.size() < MAX_ERROR) errors.write(buffer, 0, minOf(n, MAX_ERROR - errors.size()))
                    }
                }
            } catch (_: IOException) {
            }
        }
        // A program that hangs is killed, which ends the read below.
        val watchdog = thread {
            try {
                if (!process.waitFor(timeoutSeconds, TimeUnit.SECONDS)) process.destroyForcibly()
            } catch (_: InterruptedException) {
                // The call finished first. (Uncaught, this would end the whole app.)
            }
        }
        try {
            DataOutputStream(process.outputStream.buffered()).use { out ->
                for (part in listOf(op.toByteArray(), input)) {
                    out.writeInt(part.size)
                    out.write(part)
                }
            }
            val answer = try {
                val stdout = DataInputStream(process.inputStream.buffered())
                ByteArray(stdout.readInt()).also { stdout.readFully(it) }
            } catch (e: IOException) {
                process.waitFor(2, TimeUnit.SECONDS)
                reader.join(500)
                throw NativeException("the native program ended without an answer: ${String(errors.toByteArray()).trim().ifEmpty { "exit ${runCatching { process.exitValue() }.getOrDefault(-1)}" }}")
            }
            if (answer.isEmpty() || answer[0].toInt() != 0) {
                throw NativeException(if (answer.size > 1) String(answer, 1, answer.size - 1) else "native call failed")
            }
            return answer.copyOfRange(1, answer.size)
        } finally {
            process.destroy()
            watchdog.interrupt()
        }
    }
}

class NativeException(message: String) : Exception(message)