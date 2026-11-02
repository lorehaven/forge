package dev.lorehaven.songbook

import android.content.Context
import android.content.SharedPreferences
import org.json.JSONObject

/**
 * Who is signed in to which server, and the access token the calls carry.
 *
 * The password goes once, to Gatehouse, which answers a refresh token for this app (one session per user and app, kept for
 * months); that token is kept encrypted ([Vault]) and swapped for short access tokens as they run out. A server that
 * cannot be reached never ends the session: only Gatehouse refusing the refresh token does. A server that answers the
 * library with no sign-in at all (a development one) is an *open* session: no token is sent.
 */
class Session private constructor(private val prefs: SharedPreferences) {
    private var access: String? = null
    private var expiresAt = 0L

    /** `https://host`, no trailing slash; empty when nobody is signed in. */
    val server: String get() = prefs.getString("server", "") ?: ""
    val user: String get() = prefs.getString("user", "") ?: ""
    private val open: Boolean get() = prefs.getBoolean("open", false)

    val signedIn: Boolean get() = server.isNotEmpty() && (open || refreshToken() != null)

    /** Where the library's API is. */
    val api: String get() = "$server/songbook/api/v1"

    /** Where the player (the web page) is. */
    val player: String get() = "$server/songbook/ui"

    private fun refreshToken(): String? = prefs.getString("refresh", null)?.let { Vault.open(it) }

    /**
     * Signs in. [IllegalArgumentException] says why in words (wrong password, account off); [Unreachable] when the server
     * is not there.
     */
    @Synchronized
    fun login(address: String, username: String, password: String) {
        val base = normalise(address)
        // A server that needs no sign-in: say so, and ask for nothing.
        if (username.isBlank() && Http.call("GET", "$base/songbook/api/v1/library/overview").code == 200) {
            prefs.edit().clear().putString("server", base).putBoolean("open", true).apply()
            access = null
            return
        }
        val body = JSONObject().put("username", username).put("password", password).put("client", "songbook-android")
        val answer = Http.call("POST", "$base/gatehouse/api/v1/auth/login", body.toString())
        if (answer.code != 200) throw IllegalArgumentException(whyRefused(answer))
        keep(base, username, answer.body)
    }

    private fun whyRefused(answer: Answer): String {
        val text = answer.body
        return when {
            "account_locked" in text -> "This account is locked for a while after too many wrong passwords."
            "account_disabled" in text -> "This account is turned off."
            "email_unverified" in text -> "The email address of this account is not confirmed."
            "mfa_required" in text -> "This account has a second factor, which this app cannot ask for."
            answer.code == 401 || answer.code == 400 || answer.code == 403 -> "The user name or password is wrong."
            else -> "The server answered ${answer.code}."
        }
    }

    private fun keep(base: String, username: String, tokens: String) {
        val t = JSONObject(tokens)
        prefs.edit()
            .putString("server", base).putString("user", username).putBoolean("open", false)
            .putString("refresh", Vault.seal(t.getString("refresh_token")))
            .apply()
        access = t.getString("access_token")
        expiresAt = System.currentTimeMillis() + t.optLong("expires_in", 900) * 1000L * 4 / 5
    }

    /** The token to send, renewed when it is about to run out; null for an open session. [SignedOut] when it is over. */
    @Synchronized
    fun token(): String? {
        if (open) return null
        if (access != null && System.currentTimeMillis() < expiresAt) return access
        val refresh = refreshToken() ?: throw SignedOut()
        val answer = Http.call("POST", "$server/gatehouse/api/v1/auth/refresh", JSONObject().put("refresh_token", refresh).toString())
        when {
            answer.code == 200 -> keep(server, user, answer.body)
            // Gatehouse says the token is no good: that, and only that, ends the session.
            answer.code == 400 || answer.code == 401 || answer.code == 403 -> {
                signOut()
                throw SignedOut()
            }
            else -> throw Unreachable("the server answered ${answer.code}")
        }
        return access
    }

    /** Forgets the access token (the server refused it): the next call renews it. */
    @Synchronized
    fun expire() {
        expiresAt = 0
    }

    @Synchronized
    fun signOut() {
        prefs.edit().clear().apply()
        access = null
    }

    companion object {
        @Volatile private var instance: Session? = null

        fun of(context: Context): Session = instance ?: synchronized(this) {
            instance ?: Session(context.applicationContext.getSharedPreferences("session", Context.MODE_PRIVATE)).also { instance = it }
        }

        /** `host`, `host:8080` or `https://host/` as `https://host`. */
        fun normalise(address: String): String {
            val text = address.trim().trimEnd('/')
            return if (text.startsWith("http://") || text.startsWith("https://")) text else "https://$text"
        }
    }
}
