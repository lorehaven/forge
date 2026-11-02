package dev.lorehaven.songbook

import android.content.Context
import android.content.SharedPreferences

/** What the person chose on the phone and the player follows. */
class Settings private constructor(private val prefs: SharedPreferences) {
    /** How long one song fades into the next, in seconds (0: no crossfade). */
    var crossfadeSecs: Int
        get() = prefs.getInt("crossfade", 0).coerceIn(0, MAX_FADE)
        set(value) = prefs.edit().putInt("crossfade", value.coerceIn(0, MAX_FADE)).apply()

    /** How much of what was played is kept for next time, in GiB (the oldest songs go when it is full). */
    var cacheGiB: Int
        get() = prefs.getInt("cache_gib", DEFAULT_CACHE_GIB).coerceIn(1, MAX_CACHE_GIB)
        set(value) = prefs.edit().putInt("cache_gib", value.coerceIn(1, MAX_CACHE_GIB)).apply()

    private val listeners = java.util.concurrent.CopyOnWriteArrayList<() -> Unit>()
    private val watcher = SharedPreferences.OnSharedPreferenceChangeListener { _, _ -> listeners.forEach { it() } }

    init {
        prefs.registerOnSharedPreferenceChangeListener(watcher)
    }

    /** Calls `listener` when a setting changes (from the phone or the car); returns how to stop. */
    fun onChange(listener: () -> Unit): () -> Unit {
        listeners.add(listener)
        return { listeners.remove(listener) }
    }

    companion object {
        const val MAX_FADE = 12
        const val DEFAULT_CACHE_GIB = 5
        const val MAX_CACHE_GIB = 50

        /** The lengths the car's button goes through. */
        val PRESETS = listOf(0, 3, 6, 9, 12)

        @Volatile private var instance: Settings? = null

        fun of(context: Context): Settings = instance ?: synchronized(this) {
            instance ?: Settings(context.applicationContext.getSharedPreferences("settings", Context.MODE_PRIVATE)).also { instance = it }
        }
    }
}
