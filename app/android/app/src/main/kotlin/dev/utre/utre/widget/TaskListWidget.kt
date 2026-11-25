package dev.utre.utre.widget

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.unit.dp
import androidx.datastore.preferences.core.longPreferencesKey
import androidx.compose.ui.unit.sp
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.GlanceTheme
import androidx.glance.action.ActionParameters
import androidx.glance.action.actionParametersOf
import androidx.glance.action.actionStartActivity
import androidx.glance.action.clickable
import androidx.glance.appwidget.CheckBox
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.action.ActionCallback
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.lazy.LazyColumn
import androidx.glance.appwidget.lazy.items
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.glance.appwidget.provideContent
import androidx.glance.appwidget.state.updateAppWidgetState
import androidx.glance.background
import androidx.glance.currentState
import androidx.glance.state.PreferencesGlanceStateDefinition
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import dev.utre.utre.sync.SyncWorker
import dev.utre.core.WidgetTask
import dev.utre.utre.MainActivity
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * M0 Spike 3: a home-screen widget showing Inbox, read straight from the Rust core.
 * Ticking a checkbox completes the task in the core and redraws the widget.
 * Choosing a list or filter per widget comes in M4.
 */
class TaskListWidget : GlanceAppWidget() {
    // Glance calls provideGlance once per session and later updates only recompose, so
    // data loaded up front would go stale. Instead a "version" lives in the widget's
    // Glance state; refreshAll() bumps it and the composition reloads from the core.
    override val stateDefinition = PreferencesGlanceStateDefinition

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        provideContent {
            val version = currentState(versionKey) ?: 0L
            val tasks by produceState<Result<List<WidgetTask>>?>(initialValue = null, version) {
                value = loadInbox(context)
            }
            GlanceTheme { Content(tasks) }
        }
    }

    private suspend fun loadInbox(context: Context): Result<List<WidgetTask>> =
        withContext(Dispatchers.IO) { runCatching { UtreCore.db(context).inbox() } }

    companion object {
        private val versionKey = longPreferencesKey("version")

        /** Reload and redraw every placed Utre widget, e.g. after tasks changed. */
        suspend fun refreshAll(context: Context) {
            val ids = GlanceAppWidgetManager(context).getGlanceIds(TaskListWidget::class.java)
            for (id in ids) {
                updateAppWidgetState(context, id) { it[versionKey] = System.nanoTime() }
                TaskListWidget().update(context, id)
            }
        }
    }
}

@Composable
private fun Content(tasks: Result<List<WidgetTask>>?) {
    if (tasks == null) return Message("Loading…")
    val list = tasks.getOrDefault(emptyList())
    Column(
        modifier = GlanceModifier
            .fillMaxSize()
            .background(GlanceTheme.colors.widgetBackground)
            .cornerRadius(20.dp)
            .padding(12.dp),
    ) {
        Row(
            modifier = GlanceModifier.fillMaxWidth().padding(bottom = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = "Inbox · ${list.size}",
                style = TextStyle(
                    color = GlanceTheme.colors.onSurface,
                    fontSize = 16.sp,
                    fontWeight = FontWeight.Bold,
                ),
                modifier = GlanceModifier.defaultWeight().clickable(actionStartActivity<MainActivity>()),
            )
            // Quick add opens the app for now; an in-widget entry sheet comes in M4.
            Box(
                modifier = GlanceModifier
                    .size(32.dp)
                    .cornerRadius(16.dp)
                    .background(GlanceTheme.colors.primary)
                    .clickable(actionStartActivity<MainActivity>()),
                contentAlignment = Alignment.Center,
            ) {
                Text("+", style = TextStyle(color = GlanceTheme.colors.onPrimary, fontSize = 18.sp))
            }
        }

        when {
            tasks.isFailure -> Message("Couldn't read tasks")
            list.isEmpty() -> Message("Inbox is empty 🎉")
            else -> LazyColumn {
                items(list, itemId = { it.id.hashCode().toLong() }) { task -> TaskRow(task) }
            }
        }
    }
}

@Composable
private fun TaskRow(task: WidgetTask) {
    Row(
        modifier = GlanceModifier
            .fillMaxWidth()
            .padding(vertical = 2.dp)
            .cornerRadius(12.dp)
            .background(GlanceTheme.colors.surface)
            .padding(horizontal = 4.dp, vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CheckBox(
            checked = false,
            onCheckedChange = actionRunCallback<CompleteTaskAction>(
                actionParametersOf(CompleteTaskAction.taskId to task.id),
            ),
            text = task.title,
            style = TextStyle(color = GlanceTheme.colors.onSurface, fontSize = 14.sp),
            maxLines = 1,
        )
        Spacer(GlanceModifier.defaultWeight())
    }
}

@Composable
private fun Message(text: String) {
    Box(modifier = GlanceModifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Text(text, style = TextStyle(color = GlanceTheme.colors.onSurfaceVariant, fontSize = 14.sp))
    }
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
}
