package dev.utre.utre.widget

import android.content.Context
import android.content.Intent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.LocalContext
import androidx.glance.action.ActionParameters
import androidx.glance.action.actionParametersOf
import androidx.glance.action.actionStartActivity
import androidx.glance.appwidget.action.actionStartActivity as actionStartIntent
import androidx.glance.action.clickable
import androidx.glance.appwidget.CheckBox
import androidx.glance.appwidget.CheckboxDefaults
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.action.ActionCallback
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.lazy.LazyColumn
import androidx.glance.appwidget.lazy.items
import androidx.glance.appwidget.provideContent
import androidx.glance.appwidget.state.updateAppWidgetState
import androidx.glance.background
import androidx.glance.currentState
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.state.PreferencesGlanceStateDefinition
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import androidx.glance.color.ColorProvider
import dev.utre.core.WidgetList
import dev.utre.core.WidgetRow
import dev.utre.utre.MainActivity
import dev.utre.utre.sync.SyncWorker
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The home-screen widget: one list, tag, saved filter or the Inbox (chosen per instance in
 * [WidgetConfigActivity]), read straight from the Rust core. A checkbox completes the task,
 * a tap on its title opens the app, **+** opens the quick-add box.
 */
class TaskListWidget : GlanceAppWidget() {
    // Glance calls provideGlance once per session and later updates only recompose, so
    // data loaded up front would go stale. Instead a "version" lives in the widget's
    // Glance state; refreshAll() bumps it and the composition reloads from the core.
    override val stateDefinition = PreferencesGlanceStateDefinition

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        provideContent {
            val prefs = currentState<androidx.datastore.preferences.core.Preferences>()
            val version = prefs[Source.versionKey] ?: 0L
            val source = Source.from(prefs)
            val list by produceState<Result<WidgetList>?>(initialValue = null, version, source) {
                value = withContext(Dispatchers.IO) { loadSource(context, source) }
            }
            Content(list, source)
        }
    }

    companion object {
        /** Reload and redraw every placed Utre widget, e.g. after tasks changed. */
        suspend fun refreshAll(context: Context) {
            val ids = GlanceAppWidgetManager(context).getGlanceIds(TaskListWidget::class.java)
            for (id in ids) {
                updateAppWidgetState(context, id) { it[Source.versionKey] = System.nanoTime() }
                TaskListWidget().update(context, id)
            }
        }

        /** Point one widget at a source and redraw it. */
        suspend fun setSource(context: Context, id: GlanceId, source: Source) {
            updateAppWidgetState(context, id) {
                it[Source.kindKey] = source.kind
                it[Source.idKey] = source.id
                it[Source.versionKey] = System.nanoTime()
            }
            TaskListWidget().update(context, id)
        }
    }
}

/** The Soft style's colours (`app/lib/desk/theme.dart`), light and dark. */
private object Soft {
    private fun c(day: Long, night: Long) = ColorProvider(Color(day), Color(night))
    val panel = c(0xFFF7F8FB, 0xFF151821)
    val card = c(0xFFFFFFFF, 0xFF1B1F2A)
    val fg = c(0xFF1F2433, 0xFFE6E9F2)
    val mute = c(0xFF7A8197, 0xFF8F97AD)
    val accent = c(0xFF12A594, 0xFF3CC9B5)
    val onAccent = c(0xFFFFFFFF, 0xFF0B2A26)
    val today = c(0xFF0F8B7D, 0xFF5FD4C4)
    val late = c(0xFFE2525C, 0xFFFF7880)

    /** The checkbox border for a priority (0 none ... 3 high). */
    fun priority(p: Int) = when (p) {
        1 -> c(0xFF4C8DF6, 0xFF4C8DF6)
        2 -> c(0xFFEB9B2C, 0xFFEB9B2C)
        3 -> late
        else -> mute
    }
}

@Composable
private fun Content(result: Result<WidgetList>?, source: Source) {
    val context = LocalContext.current
    val list = result?.getOrNull()
    val shell = GlanceModifier
        .fillMaxSize()
        .background(Soft.panel)
        .cornerRadius(20.dp)
        .padding(12.dp)
    if (result == null) {
        Box(shell, contentAlignment = Alignment.Center) { Muted("Loading…") }
        return
    }
    Column(modifier = shell) {
        Row(
            modifier = GlanceModifier.fillMaxWidth().padding(bottom = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = if (list == null) "Utre" else "${list.name} · ${list.rows.size}",
                style = TextStyle(color = Soft.fg, fontSize = 16.sp, fontWeight = FontWeight.Bold),
                maxLines = 1,
                modifier = GlanceModifier.defaultWeight().clickable(actionStartActivity<MainActivity>()),
            )
            Box(
                modifier = GlanceModifier
                    .size(32.dp)
                    .cornerRadius(16.dp)
                    .background(Soft.accent)
                    .clickable(actionStartIntent(QuickAddActivity.intent(context, source, list?.name))),
                contentAlignment = Alignment.Center,
            ) {
                Text("+", style = TextStyle(color = Soft.onAccent, fontSize = 20.sp, fontWeight = FontWeight.Bold))
            }
        }
        when {
            list == null -> Box(GlanceModifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Muted("Couldn't read tasks")
            }
            list.rows.isEmpty() -> Box(GlanceModifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Text("All clear", style = TextStyle(color = Soft.fg, fontSize = 15.sp, fontWeight = FontWeight.Medium))
                    Muted("Nothing to do in ${list.name}")
                }
            }
            else -> LazyColumn {
                items(list.rows, itemId = { it.id.hashCode().toLong() }) { row -> TaskRow(row) }
            }
        }
    }
}

@Composable
private fun TaskRow(task: WidgetRow) {
    val context = LocalContext.current
    Row(
        modifier = GlanceModifier
            .fillMaxWidth()
            .padding(vertical = 2.dp),
    ) {
        Row(
            modifier = GlanceModifier
                .fillMaxWidth()
                .cornerRadius(12.dp)
                .background(Soft.card)
                .padding(horizontal = 6.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            CheckBox(
                checked = false,
                onCheckedChange = actionRunCallback<CompleteTaskAction>(
                    actionParametersOf(CompleteTaskAction.taskId to task.id),
                ),
                colors = CheckboxDefaults.colors(
                    checkedColor = Soft.accent,
                    uncheckedColor = Soft.priority(task.priority.toInt()),
                ),
            )
            Column(
                modifier = GlanceModifier
                    .defaultWeight()
                    .padding(vertical = 2.dp)
                    .clickable(actionStartIntent(MainActivity.openTaskIntent(context, task.id))),
            ) {
                Text(
                    task.title,
                    style = TextStyle(color = Soft.fg, fontSize = 14.sp),
                    maxLines = 2,
                )
                val due = task.dueLabel
                if (due != null) {
                    Text(
                        due,
                        style = TextStyle(
                            color = when {
                                task.overdue -> Soft.late
                                task.dueToday -> Soft.today
                                else -> Soft.mute
                            },
                            fontSize = 11.sp,
                        ),
                        maxLines = 1,
                    )
                }
            }
            for (hex in task.tagColors) {
                val color = parseColor(hex) ?: continue
                Spacer(GlanceModifier.width(4.dp))
                Box(
                    GlanceModifier.size(8.dp).cornerRadius(4.dp).background(ColorProvider(color, color)),
                ) {}
            }
            Spacer(GlanceModifier.width(6.dp))
        }
    }
}

private fun parseColor(hex: String): Color? =
    hex.removePrefix("#").toLongOrNull(16)?.let { Color(0xFF000000 or it) }

@Composable
private fun Muted(text: String) {
    Text(text, style = TextStyle(color = Soft.mute, fontSize = 13.sp))
}

/** Runs when a checkbox is ticked: complete the task in the Rust core, then redraw. */
class CompleteTaskAction : ActionCallback {
    override suspend fun onAction(context: Context, glanceId: GlanceId, parameters: ActionParameters) {
        val id = parameters[taskId] ?: return
        withContext(Dispatchers.IO) { UtreCore.db(context).complete(id) }
        // Redraw every placed widget, not just this one, so copies never disagree.
        TaskListWidget.refreshAll(context)
        // Send the change out soon rather than at the next 15-minute sync.
        SyncWorker.syncSoon(context)
    }

    companion object {
        val taskId = ActionParameters.Key<String>("taskId")
    }
}

class TaskListWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = TaskListWidget()

    // Keep the midnight refresh alarm armed while any widget is placed.
    override fun onUpdate(context: Context, appWidgetManager: android.appwidget.AppWidgetManager, appWidgetIds: IntArray) {
        super.onUpdate(context, appWidgetManager, appWidgetIds)
        WidgetRefresh.scheduleMidnight(context)
    }

    override fun onEnabled(context: Context) {
        super.onEnabled(context)
        WidgetRefresh.scheduleMidnight(context)
    }

    override fun onDisabled(context: Context) {
        super.onDisabled(context)
        WidgetRefresh.cancelMidnight(context)
    }
}
