package dev.lorehaven.daybook

import android.appwidget.AppWidgetManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.widget.RemoteViews
import android.widget.RemoteViewsService

/** Supplies the task rows of one widget instance. */
class DaybookWidgetService : RemoteViewsService() {
    override fun onGetViewFactory(intent: Intent): RemoteViewsFactory =
        Factory(
            applicationContext,
            intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID),
        )

    private class Factory(private val context: Context, private val widgetId: Int) : RemoteViewsFactory {
        private var items: List<WidgetTask> = emptyList()
        private var listId: String = WidgetData.INBOX

        /** Re-reads the snapshot; called by the system whenever the widget's data is notified as changed. */
        override fun onDataSetChanged() {
            val snapshot = WidgetData.snapshot(context)
            listId = WidgetData.effectiveSelection(snapshot, WidgetData.selection(context, widgetId))
            items = if (snapshot == null) emptyList() else WidgetData.tasksFor(snapshot, listId)
        }

        override fun getViewAt(position: Int): RemoteViews? {
            val task = items.getOrNull(position) ?: return null
            val views = RemoteViews(context.packageName, R.layout.daybook_widget_item)
            views.setTextViewText(R.id.item_title, task.content)

            val due = task.due
            if (due == null) {
                views.setViewVisibility(R.id.item_due, android.view.View.GONE)
            } else {
                val rendered = DueColors.render(due, task.allDay, System.currentTimeMillis())
                views.setTextViewText(R.id.item_due, if (task.repeats) "↻ ${rendered.text}" else rendered.text)
                views.setTextColor(R.id.item_due, rendered.color)
                views.setViewVisibility(R.id.item_due, android.view.View.VISIBLE)
            }

            // Opens the app on this list; merged into the provider's click template.
            val fill = Intent().setData(
                Uri.parse("daybook://open?list=${Uri.encode(listId)}&task=${Uri.encode(task.id)}"),
            )
            views.setOnClickFillInIntent(R.id.item_root, fill)
            return views
        }

        override fun getCount(): Int = items.size
        override fun getItemId(position: Int): Long = items.getOrNull(position)?.id?.hashCode()?.toLong() ?: position.toLong()
        override fun hasStableIds(): Boolean = true
        override fun getViewTypeCount(): Int = 1
        override fun getLoadingView(): RemoteViews? = null
        override fun onCreate() {}
        override fun onDestroy() {}
    }
}
