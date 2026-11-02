package dev.lorehaven.songbook

import android.app.Activity

/**
 * The plugins this app offers the program that draws its screen. `quench plugin add <name>` adds lines at the marker;
 * remove one here (and its Kotlin file) to take a plugin out.
 */
fun plugins(activity: Activity): List<Plugin> = listOf(
    PlayerPlugin(activity),
    // quench:plugins
)
