package dev.tickalt.tick_alt.widget

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.ui.unit.dp
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
import androidx.glance.appwidget.provideContent
import androidx.glance.appwidget.updateAll
import androidx.glance.background
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
import dev.tickalt.core.WidgetTask
import dev.tickalt.tick_alt.MainActivity
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * M0 Spike 3: a home-screen widget showing Inbox, read straight from the Rust core.
 * Ticking a checkbox completes the task in the core and redraws the widget.
 * Choosing a list or filter per widget comes in M4.
 */
class TaskListWidget : GlanceAppWidget() {
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val tasks = loadInbox(context)
        provideContent {
            GlanceTheme { Content(tasks) }
        }
    }

    private suspend fun loadInbox(context: Context): Result<List<WidgetTask>> =
        withContext(Dispatchers.IO) { runCatching { TtCore.db(context).inbox() } }

    companion object {
        /** Redraw every placed Tick Alt widget, e.g. after the app changed tasks. */
        suspend fun refreshAll(context: Context) = TaskListWidget().updateAll(context)
    }
}

@Composable
private fun Content(tasks: Result<List<WidgetTask>>) {
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
            // Kotlin 2.4.20 crashes inlining Glance 1.2's `items(list)` helper, so use the
            // non-inline `items(count)` member instead.
            else -> LazyColumn {
                items(list.size, itemId = { i -> list[i].id.hashCode().toLong() }) { i -> TaskRow(list[i]) }
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
        withContext(Dispatchers.IO) { TtCore.db(context).complete(id) }
        TaskListWidget().update(context, glanceId)
    }

    companion object {
        val taskId = ActionParameters.Key<String>("taskId")
    }
}

class TaskListWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = TaskListWidget()
}
