package dev.utre.utre.reminders

import android.Manifest
import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.SharedPreferences
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import dev.utre.utre.MainActivity
import dev.utre.utre.R
import dev.utre.utre.sync.SyncWorker
import dev.utre.utre.widget.TaskListWidget
import dev.utre.utre.widget.UtreCore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import org.json.JSONArray
import org.json.JSONObject
import java.util.TimeZone

/**
 * Reminders as Android alarms (M4, plan/07): the core says which reminders fire in the next
 * 48 hours; each becomes an exact alarm, and the alarm a notification with Done and Snooze.
 * Rescheduled after the app changes tasks, after background syncs, and at boot.
 */
object Reminders {
    private const val CHANNEL = "reminders"
    private const val PREFS = "reminders"
    private const val EXTRA_TASK = "utre_task"
    private const val WINDOW_MS = 48L * 3600 * 1000
    const val SNOOZE_MS = 10L * 60 * 1000
    const val SNOOZE_LONG_MS = 60L * 60 * 1000

    /** Replace every scheduled alarm with the core's current list. */
    @Synchronized
    fun schedule(context: Context) {
        val ctx = context.applicationContext
        val alarms = ctx.getSystemService(AlarmManager::class.java)
        val prefs = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        // Cancel what was scheduled last time (the list may have changed in any way).
        for (code in prefs.getStringSet("codes", emptySet())!!) {
            alarms.cancel(alarmIntent(ctx, code.toInt(), null, null))
        }
        val now = System.currentTimeMillis()
        // From 0, so that reminders which already fired still tell which tasks keep one.
        val all = try {
            UtreCore.db(ctx).upcomingReminders(0, now + WINDOW_MS, TimeZone.getDefault().id)
        } catch (e: Exception) {
            android.util.Log.w("Utre", "reminders: ${e.message}")
            return
        }
        val live = all.map { it.taskId }.toSet()
        cancelStale(ctx, live)
        val due = all.filter { it.fireAt >= now }
        val codes = mutableSetOf<String>()
        for (r in due) {
            val code = (r.reminderId + r.fireAt).hashCode()
            set(ctx, r.fireAt, alarmIntent(ctx, code, r.taskId, r.title))
            codes += code.toString()
        }
        prefs.edit().putStringSet("codes", codes).apply()
        // Snoozes live outside the core's list, and alarms don't survive a reboot: re-arm them.
        for (s in snoozes(prefs)) {
            if (s.at > now && s.task in live) set(ctx, s.at, alarmIntent(ctx, s.code, s.task, s.title))
        }
    }

    /**
     * Take down shown reminder notifications whose task is no longer open or has no reminder
     * left (completed or edited on another device). `live` = tasks that still have one.
     */
    private fun cancelStale(ctx: Context, live: Set<String>) {
        try {
            val nm = ctx.getSystemService(NotificationManager::class.java)
            val shown = nm.activeNotifications
                .filter { it.notification.channelId == CHANNEL }
                .mapNotNull { sbn -> sbn.notification.extras.getString(EXTRA_TASK)?.let { sbn.id to it } }
            for (id in staleIds(shown, live)) {
                nm.cancel(id)
                shown.firstOrNull { it.first == id }?.let { cancelSnooze(ctx, it.second) }
            }
        } catch (e: Exception) {
            android.util.Log.w("Utre", "reminders: stale check: ${e.message}")
        }
    }

    /** Which of the shown (notification id, task id) pairs belong to a task not in `live`. */
    internal fun staleIds(shown: List<Pair<Int, String>>, live: Set<String>): List<Int> =
        shown.filter { it.second !in live }.map { it.first }

    /** Forget a task's snooze (it was completed), and cancel its alarm. */
    @Synchronized
    fun cancelSnooze(context: Context, taskId: String) {
        val ctx = context.applicationContext
        val code = (taskId + "snooze").hashCode()
        val prefs = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        saveSnoozes(prefs, snoozes(prefs).filter { it.code != code })
        ctx.getSystemService(AlarmManager::class.java).cancel(alarmIntent(ctx, code, null, null))
    }

    private class Snooze(val code: Int, val at: Long, val task: String, val title: String)

    private fun snoozes(prefs: SharedPreferences): List<Snooze> = try {
        val arr = JSONArray(prefs.getString("snoozes", "[]"))
        val now = System.currentTimeMillis()
        (0 until arr.length()).map { arr.getJSONObject(it) }
            .map { Snooze(it.getInt("code"), it.getLong("at"), it.getString("task"), it.getString("title")) }
            .filter { it.at > now }
    } catch (e: Exception) {
        emptyList()
    }

    private fun saveSnoozes(prefs: SharedPreferences, list: List<Snooze>) {
        val arr = JSONArray()
        for (s in list) {
            arr.put(JSONObject().put("code", s.code).put("at", s.at).put("task", s.task).put("title", s.title))
        }
        prefs.edit().putString("snoozes", arr.toString()).apply()
    }

    /** Show the reminder again in `delayMs` (10 minutes or an hour). */
    @Synchronized
    fun snooze(context: Context, taskId: String, title: String, delayMs: Long) {
        val ctx = context.applicationContext
        val code = (taskId + "snooze").hashCode()
        val at = System.currentTimeMillis() + delayMs
        val prefs = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        // Kept in the prefs so a reboot (or the next schedule()) doesn't lose it.
        saveSnoozes(prefs, snoozes(prefs).filter { it.code != code } + Snooze(code, at, taskId, title))
        set(ctx, at, alarmIntent(ctx, code, taskId, title))
    }

    private fun set(context: Context, at: Long, intent: PendingIntent) {
        val alarms = context.getSystemService(AlarmManager::class.java)
        try {
            if (Build.VERSION.SDK_INT < 31 || alarms.canScheduleExactAlarms()) {
                alarms.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, intent)
                return
            }
        } catch (e: SecurityException) {
            // The permission was revoked between the check and the call: fall through.
        }
        alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, intent)
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
        // A tap opens the task itself, as a widget row does.
        val open = PendingIntent.getActivity(
            context, (code.toString() + "open").hashCode(), MainActivity.openTaskIntent(context, taskId),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val n = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat_utre)
            .setContentTitle(title)
            .setContentText("Reminder")
            .setContentIntent(open)
            .setAutoCancel(true)
            .addExtras(android.os.Bundle().apply { putString(EXTRA_TASK, taskId) })
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
                Reminders.cancelSnooze(ctx, task)
                val pending = goAsync()
                CoroutineScope(Dispatchers.IO).launch {
                    try {
                        UtreCore.db(ctx).complete(task)
                        TaskListWidget.refreshAll(ctx)
                        Reminders.schedule(ctx)
                        SyncWorker.syncSoon(ctx)
                    } finally {
                        pending.finish()
                    }
                }
            }
            Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED -> {
                rescheduleAsync(ctx)
                SyncWorker.enqueue(ctx)
            }
            // Android 12-13: the user granted or revoked "Alarms & reminders".
            AlarmManager.ACTION_SCHEDULE_EXACT_ALARM_PERMISSION_STATE_CHANGED -> rescheduleAsync(ctx)
        }
    }

    /** schedule() reads the database: keep it off the main thread, and the receiver alive. */
    private fun BroadcastReceiver.rescheduleAsync(ctx: Context) {
        val pending = goAsync()
        CoroutineScope(Dispatchers.IO).launch {
            try {
                Reminders.schedule(ctx)
            } finally {
                pending.finish()
            }
        }
    }

    companion object {
        const val FIRE = "dev.utre.reminder.FIRE"
        const val DONE = "dev.utre.reminder.DONE"
        const val SNOOZE = "dev.utre.reminder.SNOOZE"
        const val SNOOZE_LONG = "dev.utre.reminder.SNOOZE_LONG"
    }
}
