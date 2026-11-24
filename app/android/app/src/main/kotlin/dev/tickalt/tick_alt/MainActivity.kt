package dev.tickalt.tick_alt

import android.appwidget.AppWidgetManager
import android.content.ComponentName
import dev.tickalt.tick_alt.widget.TaskListWidget
import dev.tickalt.tick_alt.widget.TaskListWidgetReceiver
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

class MainActivity : FlutterActivity() {
    private val scope = MainScope()

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "tick_alt/widget").setMethodCallHandler { call, result ->
            when (call.method) {
                // The app changed tasks: redraw every placed widget.
                "refresh" -> scope.launch {
                    TaskListWidget.refreshAll(applicationContext)
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

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }
}
