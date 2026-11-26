package dev.utre.utre

import android.Manifest
import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import dev.utre.utre.reminders.Reminders
import dev.utre.utre.sync.SyncWorker
import java.util.TimeZone
import dev.utre.utre.widget.TaskListWidget
import dev.utre.utre.widget.TaskListWidgetReceiver
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

class MainActivity : FlutterActivity() {
    private val scope = MainScope()
    private var channel: MethodChannel? = null
    /** Text shared to the app before Flutter asked for it (plan/07: share target → Inbox). */
    private var pendingShare: String? = null
    /** A task opened from the widget, until Flutter asks for it. */
    private var pendingTask: String? = null

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        val ch = MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "utre/widget")
        channel = ch
        ch.setMethodCallHandler { call, result ->
            when (call.method) {
                // Text shared from another app while Flutter was starting.
                "takeShared" -> {
                    result.success(pendingShare)
                    pendingShare = null
                }
                // The task the widget row was tapped on, if the app was opened that way.
                "takeOpenTask" -> {
                    result.success(pendingTask)
                    pendingTask = null
                }
                // The app changed tasks: redraw every placed widget.
                "refresh" -> scope.launch {
                    TaskListWidget.refreshAll(applicationContext)
                    result.success(null)
                }
                // The app changed tasks: reschedule reminder alarms.
                "reminders" -> {
                    Reminders.schedule(applicationContext)
                    result.success(null)
                }
                // The core needs the IANA zone (Android has no /etc/localtime).
                "timeZone" -> result.success(TimeZone.getDefault().id)
                // What paired devices call this phone; where its sync key lives.
                "deviceName" -> result.success(SyncWorker.deviceName())
                "keyPath" -> result.success(SyncWorker.keyPath(applicationContext))
                // Ask once for notifications (Android 13+), for reminders.
                "askNotifications" -> {
                    if (Build.VERSION.SDK_INT >= 33 &&
                        checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
                    ) {
                        requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
                    }
                    result.success(null)
                }
                // Ask the launcher to place the widget; it shows its own confirmation.
                "pin" -> {
                    val manager = AppWidgetManager.getInstance(this)
                    val supported = manager.isRequestPinAppWidgetSupported
                    if (supported) {
                        manager.requestPinAppWidget(ComponentName(this, TaskListWidgetReceiver::class.java), null, null)
                    }
                    result.success(supported)
                }
                else -> result.notImplemented()
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        pendingShare = sharedText(intent)
        pendingTask = openedTask(intent)
        Reminders.ensureChannel(this)
        SyncWorker.enqueue(applicationContext)
    }

    // Shared again while the app is open: hand it straight to Flutter.
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        openedTask(intent)?.let { task ->
            channel?.invokeMethod("openTask", task) ?: run { pendingTask = task }
        }
        val text = sharedText(intent) ?: return
        channel?.invokeMethod("shared", text) ?: run { pendingShare = text }
    }

    /** The text of an ACTION_SEND share (subject and text, e.g. a page title and its link). */
    private fun sharedText(intent: Intent?): String? {
        if (intent?.action != Intent.ACTION_SEND || intent.type?.startsWith("text/") != true) return null
        return listOfNotNull(
            intent.getStringExtra(Intent.EXTRA_SUBJECT),
            intent.getStringExtra(Intent.EXTRA_TEXT),
        ).distinct().joinToString(" ").trim().ifEmpty { null }
    }

    private fun openedTask(intent: Intent?): String? =
        if (intent?.action == OPEN_TASK) intent.getStringExtra("task") else null

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    companion object {
        private const val OPEN_TASK = "dev.utre.OPEN_TASK"

        /** Open the app on a task: the widget's row tap. */
        fun openTaskIntent(context: android.content.Context, taskId: String): Intent =
            Intent(context, MainActivity::class.java)
                .setAction(OPEN_TASK)
                .putExtra("task", taskId)
                .setData(android.net.Uri.parse("utre://task/$taskId"))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
    }
}
