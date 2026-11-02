package dev.lorehaven.songbook

import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

/** What a call answered. */
class Answer(val code: Int, val body: String)

/** The service could not be reached, or answered something that is not an answer. */
class Unreachable(message: String) : IOException(message)

/** The session is over: the refresh token is refused, or nobody is signed in. */
class SignedOut(message: String = "signed out") : IOException(message)

object Http {
    /** One call. `json` is sent as the body of a POST. A transport failure is [Unreachable]. */
    fun call(method: String, url: String, json: String? = null, token: String? = null): Answer {
        val connection = try {
            URL(url).openConnection() as HttpURLConnection
        } catch (e: Exception) {
            throw Unreachable("bad address: $url")
        }
        return try {
            connection.requestMethod = method
            connection.connectTimeout = 15_000
            connection.readTimeout = 30_000
            connection.setRequestProperty("Accept", "application/json")
            if (token != null) connection.setRequestProperty("Authorization", "Bearer $token")
            if (json != null) {
                connection.doOutput = true
                connection.setRequestProperty("Content-Type", "application/json")
                connection.outputStream.use { it.write(json.toByteArray()) }
            }
            val code = connection.responseCode
            val stream = if (code >= 400) connection.errorStream else connection.inputStream
            Answer(code, stream?.bufferedReader()?.use { it.readText() } ?: "")
        } catch (e: IOException) {
            throw Unreachable(e.message ?: "cannot reach the server")
        } finally {
            connection.disconnect()
        }
    }
}
