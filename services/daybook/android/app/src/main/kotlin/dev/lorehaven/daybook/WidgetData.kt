package dev.lorehaven.daybook

import android.content.Context
import android.content.SharedPreferences
import java.io.File
import org.json.JSONObject

data class WidgetList(val id: String, val name: String, val kind: String)

data class WidgetTask(
    val id: String,
    val content: String,
    val project: String?,
    val labels: List<String>,
    val due: Long?,
    val allDay: Boolean,
    val repeats: Boolean,
)

data class Snapshot(val lists: List<WidgetList>, val tasks: List<WidgetTask>)

/**
 * What the app's program hands the widget, and which list each widget instance shows.
 *
 * The program writes one snapshot (every list, every open task, already in display order) to `widget.json` in its
 * data directory (`daybook_core::widget`); the widget only filters it. It never talks to the server, so it needs no
 * sign-in of its own. A background job (see [WidgetRefresh]) runs the program with `--widget` to keep it fresh.
 */
object WidgetData {
    const val INBOX = "inbox"

    /** The program's data directory: `HOME/.local/share/daybook` (the host sets `HOME` to the files directory). */
    fun dataDir(context: Context): File = File(context.filesDir, ".local/share/daybook")

    private fun prefs(context: Context): SharedPreferences =
        context.getSharedPreferences("daybook-widget", Context.MODE_PRIVATE)

    /** The snapshot, or null when there is none (nobody is signed in) or it cannot be read. */
    fun snapshot(context: Context): Snapshot? {
        val file = File(dataDir(context), "widget.json")
        if (!file.exists()) return null
        return runCatching { parse(file.readText()) }.getOrNull()
    }

    fun parse(raw: String): Snapshot? = runCatching {
        val root = JSONObject(raw)
        val lists = root.getJSONArray("lists")
        val tasks = root.getJSONArray("tasks")
        Snapshot(
            lists = (0 until lists.length()).map {
                val item = lists.getJSONObject(it)
                WidgetList(item.getString("id"), item.getString("name"), item.getString("kind"))
            },
            tasks = (0 until tasks.length()).map {
                val item = tasks.getJSONObject(it)
                val labels = item.getJSONArray("labels")
                WidgetTask(
                    id = item.getString("id"),
                    content = item.getString("content"),
                    project = if (item.isNull("project")) null else item.getString("project"),
                    labels = (0 until labels.length()).map { index -> labels.getString(index) },
                    due = if (item.isNull("due")) null else item.getLong("due"),
                    allDay = item.optBoolean("allDay", false),
                    repeats = item.optBoolean("repeats", false),
                )
            },
        )
    }.getOrNull()

    private fun selectionKey(widgetId: Int) = "widget_list_$widgetId"

    /** The list this widget instance was set to show, even if it no longer exists. */
    fun selection(context: Context, widgetId: Int): String =
        prefs(context).getString(selectionKey(widgetId), INBOX) ?: INBOX

    fun select(context: Context, widgetId: Int, listId: String) {
        prefs(context).edit().putString(selectionKey(widgetId), listId).apply()
    }

    fun forget(context: Context, widgetIds: IntArray) {
        val editor = prefs(context).edit()
        widgetIds.forEach { editor.remove(selectionKey(it)) }
        editor.apply()
    }

    /** What to actually show: the chosen list, or the inbox if it was deleted since. */
    fun effectiveSelection(snapshot: Snapshot?, chosen: String): String =
        if (snapshot != null && snapshot.lists.any { it.id == chosen }) chosen else INBOX

    /** The open tasks of one list, in the order the snapshot has them (soonest due first). */
    fun tasksFor(snapshot: Snapshot, listId: String): List<WidgetTask> = when {
        listId == INBOX -> snapshot.tasks.filter { it.project == null }
        listId.startsWith("p:") -> snapshot.tasks.filter { it.project == listId.removePrefix("p:") }
        listId.startsWith("l:") -> snapshot.tasks.filter { it.labels.contains(listId.removePrefix("l:")) }
        else -> emptyList()
    }

    fun titleFor(snapshot: Snapshot?, listId: String): String =
        snapshot?.lists?.firstOrNull { it.id == listId }?.name ?: "Daybook"
}
