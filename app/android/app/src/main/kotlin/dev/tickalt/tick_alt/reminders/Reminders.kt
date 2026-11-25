package dev.tickalt.tick_alt.reminders

import android.Manifest
import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import dev.tickalt.tick_alt.MainActivity
import dev.tickalt.tick_alt.R
import dev.tickalt.tick_alt.sync.SyncWorker
import dev.tickalt.tick_alt.widget.TaskListWidget
import dev.tickalt.tick_alt.widget.TtCore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import java.util.TimeZone

/**
 * Reminders as Android alarms (M4, plan/07): the core says which reminders fire in the next
 * 48 hours; each becomes an exact alarm, and the alarm a notification with Done and Snooze.
 * Rescheduled after the app changes tasks, after background syncs, and at boot.
 */
object Reminders {
    private const val CHANNEL = "reminders"
    private const val PREFS = "reminders"
    private const val WINDOW_MS = 48L * 3600 * 1000
    const val SNOOZE_MS = 10L * 60 * 1000
    const val SNOOZE_LONG_MS = 60L * 60 * 1000

    /** Replace every scheduled alarm with the core's current list. */
    fun schedule(context: Context) {
        val ctx = context.applicationContext
        val alarms = ctx.getSystemService(AlarmManager::class.java)
        val prefs = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        // Cancel what was scheduled last time (the list may have changed in any way).
        for (code in prefs.getStringSet("codes", emptySet())!!) {
            alarms.cancel(alarmIntent(ctx, code.toInt(), null, null))
        }
        val now = System.currentTimeMillis()
        val due = try {
            TtCore.db(ctx).upcomingReminders(now, now + WINDOW_MS, TimeZone.getDefault().id)
        } catch (e: Exception) {
            android.util.Log.w("TickAlt", "reminders: ${e.message}")
            return
        }
        val codes = mutableSetOf<String>()
        for (r in due) {
            val code = (r.reminderId + r.fireAt).hashCode()
            set(ctx, r.fireAt, alarmIntent(ctx, code, r.taskId, r.title))
            codes += code.toString()
        }
        prefs.edit().putStringSet("codes", codes).apply()
    }

    /** Show the reminder again in `delayMs` (10 minutes or an hour). */
    fun snooze(context: Context, taskId: String, title: String, delayMs: Long) {
        val code = (taskId + "snooze").hashCode()
        set(context, System.currentTimeMillis() + delayMs, alarmIntent(context, code, taskId, title))
    }

    private fun set(context: Context, at: Long, intent: PendingIntent) {
        val alarms = context.getSystemService(AlarmManager::class.java)
        if (Build.VERSION.SDK_INT < 31 || alarms.canScheduleExactAlarms()) {
            alarms.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, intent)
        } else {
            alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, intent)
        }
    }

    private fun alarmIntent(context: Context, code: Int, taskId: String?, title: String?): PendingIntent {
        val i = Intent(context, ReminderReceiver::class.java).setAction(ReminderReceiver.FIRE)
        if (taskId != null) i.putExtra("task", taskId).putExtra("title", title).putExtra("code", code)
        return PendingIntent.getBroadcast(
            context, code, i, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    fun ensureChannel(context: Context) {
        val nm = context.getSystemService(NotificationManager::class.java)
        if (nm.getNotificationChannel(CHANNEL) == null) {
            nm.createNotificationChannel(
                NotificationChannel(CHANNEL, "Reminders", NotificationManager.IMPORTANCE_HIGH),
            )
        }
    }

    fun notify(context: Context, code: Int, taskId: String, title: String) {
        ensureChannel(context)
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            return
        }
        fun action(kind: String) = PendingIntent.getBroadcast(
            context, (code.toString() + kind).hashCode(),
            Intent(context, ReminderReceiver::class.java).setAction(kind)
                .putExtra("task", taskId).putExtra("title", title).putExtra("code", code),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val open = PendingIntent.getActivity(
            context, 0, Intent(context, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val n = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle(title)
            .setContentText("Reminder")
            .setContentIntent(open)
            .setAutoCancel(true)
            .setCategory(NotificationCompat.CATEGORY_REMINDER)
            .addAction(0, "Done", action(ReminderReceiver.DONE))
            .addAction(0, "Snooze 10 min", action(ReminderReceiver.SNOOZE))
            .addAction(0, "1 h", action(ReminderReceiver.SNOOZE_LONG))
            .build()
        NotificationManagerCompat.from(context).notify(code, n)
    }
}

/** Alarms, notification actions and boot all land here. */
class ReminderReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val ctx = context.applicationContext
        val task = intent.getStringExtra("task")
        val title = intent.getStringExtra("title") ?: ""
        val code = intent.getIntExtra("code", 0)
        when (intent.action) {
            FIRE -> if (task != null) Reminders.notify(ctx, code, task, title)
            SNOOZE, SNOOZE_LONG -> if (task != null) {
                NotificationManagerCompat.from(ctx).cancel(code)
                val delay = if (intent.action == SNOOZE_LONG) Reminders.SNOOZE_LONG_MS else Reminders.SNOOZE_MS
                Reminders.snooze(ctx, task, title, delay)
            }
            DONE -> if (task != null) {
                NotificationManagerCompat.from(ctx).cancel(code)
                val pending = goAsync()
                CoroutineScope(Dispatchers.IO).launch {
                    try {
                        TtCore.db(ctx).complete(task)
                        TaskListWidget.refreshAll(ctx)
                        Reminders.schedule(ctx)
                        SyncWorker.syncSoon(ctx)
                    } finally {
                        pending.finish()
                    }
                }
            }
            Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED -> {
                Reminders.schedule(ctx)
                SyncWorker.enqueue(ctx)
            }
        }
    }

    companion object {
        const val FIRE = "dev.tickalt.reminder.FIRE"
        const val DONE = "dev.tickalt.reminder.DONE"
        const val SNOOZE = "dev.tickalt.reminder.SNOOZE"
        const val SNOOZE_LONG = "dev.tickalt.reminder.SNOOZE_LONG"
    }
}
