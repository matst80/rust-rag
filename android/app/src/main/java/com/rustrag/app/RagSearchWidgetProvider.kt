package com.rustrag.app

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.widget.RemoteViews
import androidx.core.net.toUri

class RagSearchWidgetProvider : AppWidgetProvider() {

    override fun onUpdate(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetIds: IntArray
    ) {
        val app = context.applicationContext as? RagApplication
        val tokenManager = app?.tokenManager
        val latestText = tokenManager?.widgetLatestText ?: "Search Second Brain..."
        val latestId = tokenManager?.widgetLatestId

        for (appWidgetId in appWidgetIds) {
            val views = RemoteViews(context.packageName, R.layout.rag_search_widget)

            // Set Text to latest entry text or search placeholder
            views.setTextViewText(R.id.widget_search_text, latestText)

            // Setup Search Bar click target (deep links to MainActivity Detail if available, otherwise Search)
            val clickIntent = if (!latestId.isNullOrBlank()) {
                Intent(
                    Intent.ACTION_VIEW,
                    "rustrag://detail?id=$latestId".toUri(),
                    context,
                    MainActivity::class.java
                )
            } else {
                Intent(
                    Intent.ACTION_VIEW,
                    "rustrag://search?focus=true".toUri(),
                    context,
                    MainActivity::class.java
                )
            }.apply {
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            }
            val clickPendingIntent = PendingIntent.getActivity(
                context,
                0,
                clickIntent,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )
            views.setOnClickPendingIntent(R.id.widget_container, clickPendingIntent)

            // Setup Quick Capture (+) click target (launches ShareActivity dialog)
            val addIntent = Intent(context, ShareActivity::class.java).apply {
                action = Intent.ACTION_SEND
                type = "text/plain"
                putExtra(Intent.EXTRA_TEXT, "")
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            }
            val addPendingIntent = PendingIntent.getActivity(
                context,
                1,
                addIntent,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )
            views.setOnClickPendingIntent(R.id.btn_widget_add, addPendingIntent)

            // Commit the update to the widget manager
            appWidgetManager.updateAppWidget(appWidgetId, views)
        }
    }

    companion object {
        fun triggerUpdate(context: Context) {
            val intent = Intent(context, RagSearchWidgetProvider::class.java).apply {
                action = AppWidgetManager.ACTION_APPWIDGET_UPDATE
            }
            val appWidgetManager = AppWidgetManager.getInstance(context)
            val ids = appWidgetManager.getAppWidgetIds(
                ComponentName(context, RagSearchWidgetProvider::class.java)
            )
            intent.putExtra(AppWidgetManager.EXTRA_APPWIDGET_IDS, ids)
            context.sendBroadcast(intent)
        }
    }
}
