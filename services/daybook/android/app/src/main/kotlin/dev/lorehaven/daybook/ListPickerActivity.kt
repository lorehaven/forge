package dev.lorehaven.daybook

import android.app.Activity
import android.app.AlertDialog
import android.appwidget.AppWidgetManager
import android.os.Bundle

/**
 * Lets the user choose what a widget instance shows: the inbox, a project or a label. A plain dialog over a
 * transparent activity, opened by tapping the widget's title. The choices come from the snapshot, so it works
 * with the app closed.
 */
class ListPickerActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val widgetId = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        val snapshot = WidgetData.snapshot(this)
        if (widgetId == AppWidgetManager.INVALID_APPWIDGET_ID || snapshot == null || snapshot.lists.isEmpty()) {
            finish()
            return
        }

        val lists = snapshot.lists
        // Labels get a "#" so a label and a project of the same name can be told apart.
        val names = lists.map { if (it.kind == "label") "# ${it.name}" else it.name }.toTypedArray()
        val current = lists.indexOfFirst { it.id == WidgetData.effectiveSelection(snapshot, WidgetData.selection(this, widgetId)) }

        AlertDialog.Builder(this)
            .setTitle(R.string.widget_pick_list)
            .setSingleChoiceItems(names, current) { dialog, which ->
                WidgetData.select(this, widgetId, lists[which].id)
                DaybookWidgetProvider.refresh(this, widgetId)
                dialog.dismiss()
            }
            .setOnDismissListener { finish() }
            .show()
    }
}
