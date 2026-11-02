package dev.lorehaven.songbook

import android.content.Context

/**
 * Who is signed in to which server. The Rust program ([Brain]) owns the session: it signs in to Gatehouse, keeps the refresh
 * token sealed with the device's key, and renews the access token as it runs out. This only remembers what it last said, so
 * that screens can ask quickly.
 */
class Session private constructor(private val brain: Brain) {
    @Volatile private var loaded = false
    @Volatile private var signed = false
    @Volatile private var host = ""
    @Volatile private var name = ""
    @Volatile private var base = ""

    /** When it was last asked: the screens' program signs in and out too, so what is known is asked again after a moment. */
    @Volatile private var askedAt = 0L

    private fun load() {
        if (loaded && System.currentTimeMillis() - askedAt < FRESH_MS) return
        synchronized(this) {
            if (loaded && System.currentTimeMillis() - askedAt < FRESH_MS) return
            refresh()
        }
    }

    /** Asks the program who is signed in. */
    @Synchronized
    fun refresh() {
        try {
            val s = brain.call("status")
            signed = s.optBoolean("signed_in")
            host = s.optString("server")
            name = s.optString("user")
            base = s.optString("api")
            loaded = true
            askedAt = System.currentTimeMillis()
        } catch (e: java.io.IOException) {
            // Not known yet: asked again next time.
        }
    }

    /** `https://host`, no trailing slash; empty when nobody is signed in. */
    val server: String get() { load(); return host }
    val user: String get() { load(); return name }
    val signedIn: Boolean get() { load(); return signed }

    /** Where the library's API is. */
    val api: String get() { load(); return base }

    /** Where the player (the web page) is. */
    val player: String get() = "$server/songbook/ui"

    /** Signs in. [IllegalArgumentException] says why in words (wrong password, account off); [Unreachable] when the server is not there. */
    fun login(address: String, username: String, password: String) {
        brain.call("login", "server" to address, "user" to username, "password" to password)
        refresh()
    }

    /** The token to send, renewed when it is about to run out; null for an open session. [SignedOut] when it is over. */
    fun token(): String? = try {
        brain.call("token").takeIf { !it.isNull("token") }?.getString("token")
    } catch (e: SignedOut) {
        signed = false
        throw e
    }

    fun signOut() {
        brain.call("sign_out")
        refresh()
    }

    companion object {
        private const val FRESH_MS = 2_000L

        @Volatile private var instance: Session? = null

        fun of(context: Context): Session = instance ?: synchronized(this) {
            instance ?: Session(Brain.of(context)).also { instance = it }
        }
    }
}
