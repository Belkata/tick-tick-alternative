package dev.utre.utre.widget

import android.app.Activity
import android.appwidget.AppWidgetManager
import android.content.Intent
import android.graphics.Typeface
import android.os.Bundle
import android.view.View
import android.view.ViewGroup
import android.widget.ArrayAdapter
import android.widget.ListView
import android.widget.TextView
import androidx.glance.appwidget.GlanceAppWidgetManager
import dev.utre.core.WidgetSource
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.util.TimeZone

/**
 * Shown when a widget is placed (and from its "reconfigure" menu on Android 12+): pick what
 * this widget shows, the Inbox or one list, tag or saved filter. Stored in the widget's own
 * Glance state, so several widgets can show different things.
 */
class WidgetConfigActivity : Activity() {
    private val scope = MainScope()
    private var appWidgetId = AppWidgetManager.INVALID_APPWIDGET_ID

    /** A list row: a section heading (`source == null`) or a choice. */
    private class Row(val label: String, val source: WidgetSource?)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        appWidgetId = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        // Backing out cancels the placement.
        setResult(RESULT_CANCELED, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, appWidgetId))
        if (appWidgetId == AppWidgetManager.INVALID_APPWIDGET_ID) return finish()
        title = "Show in widget"

        val list = ListView(this)
        setContentView(list)
        scope.launch {
            val rows = withContext(Dispatchers.IO) {
                runCatching { buildRows(UtreCore.db(applicationContext).sources(TimeZone.getDefault().id)) }
            }.getOrNull()
            if (rows == null) return@launch finish()
            list.adapter = object : ArrayAdapter<Row>(this@WidgetConfigActivity, 0, rows) {
                override fun getView(position: Int, convertView: View?, parent: ViewGroup): View {
                    val row = getItem(position)!!
                    val dp = resources.displayMetrics.density
                    return TextView(context).apply {
                        text = row.label
                        if (row.source == null) {
                            textSize = 12f
                            setTypeface(typeface, Typeface.BOLD)
                            alpha = 0.6f
                            setPadding((20 * dp).toInt(), (16 * dp).toInt(), (20 * dp).toInt(), (4 * dp).toInt())
                        } else {
                            textSize = 16f
                            val indent = row.source.depth.toInt() * 16
                            setPadding(((20 + indent) * dp).toInt(), (12 * dp).toInt(), (20 * dp).toInt(), (12 * dp).toInt())
                        }
                    }
                }

                override fun isEnabled(position: Int) = getItem(position)?.source != null
                override fun areAllItemsEnabled() = false
            }
            list.setOnItemClickListener { _, _, position, _ ->
                rows[position].source?.let { choose(Source(it.kind, it.id)) }
            }
        }
    }

    private fun buildRows(sources: List<WidgetSource>): List<Row> {
        val rows = mutableListOf<Row>()
        fun section(title: String, kind: String) {
            val items = sources.filter { it.kind == kind }
            if (items.isEmpty()) return
            rows += Row(title, null)
            items.forEach { rows += Row("${it.name}  ·  ${it.count}", it) }
        }
        sources.firstOrNull { it.kind == "inbox" }?.let { rows += Row("${it.name}  ·  ${it.count}", it) }
        section("LISTS", "list")
        section("TAGS", "tag")
        section("FILTERS", "filter")
        return rows
    }

    private fun choose(source: Source) {
        scope.launch {
            val glanceId = GlanceAppWidgetManager(applicationContext).getGlanceIdBy(appWidgetId)
            TaskListWidget.setSource(applicationContext, glanceId, source)
            setResult(RESULT_OK, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, appWidgetId))
            finish()
        }
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }
}
