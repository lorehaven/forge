package dev.lorehaven.daybook

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.widget.RemoteViews

/**
 * The home-screen widget: a header (logo, the list it shows, an add button) over that list's open tasks.
 *
 * Tapping the title opens [ListPickerActivity] to choose the inbox, a project or a label; the add button and the tasks
 * open the app with a `daybook://` link that [SurfaceActivity] hands to the program. The task rows come from
 * [DaybookWidgetService]. The widget is redrawn by the program after every change, by [WidgetRefresh] every 30 minutes,
 * and by the system's own half-hourly update (which keeps "Today"/"Tomorrow" right).
 */
class DaybookWidgetProvider : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, appWidgetIds: IntArray) {
        appWidgetIds.forEach { update(context, manager, it) }
    }

    /** Resizing: redraw so the list gets the new height. */
    override fun onAppWidgetOptionsChanged(context: Context, manager: AppWidgetManager, appWidgetId: Int, newOptions: Bundle) {
        update(context, manager, appWidgetId)
    }

    override fun onDeleted(context: Context, appWidgetIds: IntArray) {
        WidgetData.forget(context, appWidgetIds)
    }

    /** The first widget on the home screen starts the refresh job; the last one removed stops it. */
    override fun onEnabled(context: Context) = WidgetRefresh.schedule(context)

    override fun onDisabled(context: Context) = WidgetRefresh.cancel(context)

    /** A receiver that throws takes the whole process down; the widget must never do that, whatever the system sends. */
    override fun onReceive(context: Context, intent: Intent) {
        runCatching { super.onReceive(context, intent) }.onFailure { Log.w(TAG, "widget broadcast ${intent.action} failed: $it") }
    }

    companion object {
        private const val TAG = "daybook-widget"

        /**
         * Draws one widget. If building the full widget fails for any reason (an unreadable snapshot, a resource the
         * launcher cannot apply), a plain "open Daybook" tile is shown instead: the launcher's own answer to a widget
         * that cannot be applied is "Problem loading widget", which says nothing and cannot be tapped.
         */
        fun update(context: Context, manager: AppWidgetManager, widgetId: Int) {
            val views = runCatching { build(context, widgetId) }
                .onFailure { Log.w(TAG, "widget $widgetId could not be built: $it") }
                .getOrNull()
            if (views != null && runCatching { manager.updateAppWidget(widgetId, views) }.isSuccess) {
                runCatching { manager.notifyAppWidgetViewDataChanged(widgetId, R.id.widget_list) }
                return
            }
            runCatching { manager.updateAppWidget(widgetId, fallback(context, widgetId)) }
                .onFailure { Log.w(TAG, "widget $widgetId fallback failed: $it") }
        }

        private fun fallback(context: Context, widgetId: Int): RemoteViews {
            val views = RemoteViews(context.packageName, R.layout.daybook_widget_fallback)
            views.setOnClickPendingIntent(R.id.widget_fallback, launch(context, widgetId * 2 + 1, "daybook://open?list=inbox"))
            return views
        }

        private fun build(context: Context, widgetId: Int): RemoteViews {
            val snapshot = WidgetData.snapshot(context)
            val selected = WidgetData.effectiveSelection(snapshot, WidgetData.selection(context, widgetId))

            val views = RemoteViews(context.packageName, R.layout.daybook_widget)
            views.setTextViewText(R.id.widget_title, WidgetData.titleFor(snapshot, selected))
            views.setTextViewText(
                R.id.widget_empty,
                context.getString(if (snapshot == null) R.string.widget_empty_signed_out else R.string.widget_empty_tasks),
            )

            // The task rows. The data URI is unique per widget so each instance gets its own factory.
            val adapter = Intent(context, DaybookWidgetService::class.java).putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId)
            adapter.data = Uri.parse("daybook-widget://list/$widgetId")
            views.setRemoteAdapter(R.id.widget_list, adapter)
            views.setEmptyView(R.id.widget_list, R.id.widget_empty)

            // Title: choose which list to show.
            val picker = Intent(context, ListPickerActivity::class.java).putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId)
            picker.data = Uri.parse("daybook-widget://picker/$widgetId")
            views.setOnClickPendingIntent(
                R.id.widget_picker,
                PendingIntent.getActivity(context, widgetId, picker, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE),
            )

            // Plus: the add-task form, on this widget's list. Logo: just open the app there.
            val list = Uri.encode(selected)
            views.setOnClickPendingIntent(R.id.widget_add, launch(context, widgetId * 2, "daybook://add?list=$list"))
            views.setOnClickPendingIntent(R.id.widget_logo, launch(context, widgetId * 2 + 1, "daybook://open?list=$list"))

            // A task: open the app on it. The row supplies the URI; this is the shared template.
            views.setPendingIntentTemplate(R.id.widget_list, template(context))
            return views
        }

        private fun launch(context: Context, code: Int, link: String): PendingIntent =
            PendingIntent.getActivity(
                context,
                code,
                Intent(context, SurfaceActivity::class.java).setData(Uri.parse(link)).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )

        /** The click template of the task rows: it must be mutable because each row fills in its own data URI. */
        private fun template(context: Context): PendingIntent {
            val intent = Intent(context, SurfaceActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP)
            val flags = PendingIntent.FLAG_UPDATE_CURRENT or if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0
            return PendingIntent.getActivity(context, 0, intent, flags)
        }

        /** Redraw one widget, e.g. after its list was changed in the picker. */
        fun refresh(context: Context, widgetId: Int) = update(context, AppWidgetManager.getInstance(context), widgetId)

        /** Redraw every widget (the program wrote a new snapshot, or the refresh job did). */
        fun refreshAll(context: Context) {
            val manager = AppWidgetManager.getInstance(context)
            manager.getAppWidgetIds(ComponentName(context, DaybookWidgetProvider::class.java)).forEach { update(context, manager, it) }
        }
    }
}
