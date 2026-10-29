package dev.lorehaven.hoist

import android.app.Activity

/**
 * The plugins this app offers its web side. `quench plugin add <name>` adds lines at the marker;
 * remove one here (and its Kotlin file) to take a plugin out.
 */
fun plugins(activity: Activity): List<Plugin> = listOf(
    EchoPlugin(),
    PackagesPlugin(activity),
    // quench:plugins
)