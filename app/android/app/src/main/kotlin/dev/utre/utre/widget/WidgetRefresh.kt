package dev.utre.utre.widget

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import dev.utre.utre.reminders.Reminders
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import java.util.Calendar
import java.util.TimeZone

/**
 * Keeps date-relative rows right ("today", overdue, Today filters) without the app open: an
 * alarm at the next local midnight, plus the system's date, time and time-zone changes.
 * Each one redraws every widget and re-arms the alarm for the next midnight.
 */
object WidgetRefresh {
    private const val ALARM = "dev.utre.widget.MIDNIGHT"

    /** The first local midnight after `now`, in `zone`. */
    fun nextMidnight(now: Long, zone: TimeZone): Long =
        Calendar.getInstance(zone).apply {
            timeInMillis = now
            set(Calendar.HOUR_OF_DAY, 0)
            set(Calendar.MINUTE, 0)
            set(Calendar.SECOND, 0)
            set(Calendar.MILLISECOND, 0)
            add(Calendar.DAY_OF_YEAR, 1)
        }.timeInMillis

    fun scheduleMidnight(context: Context) {
        val ctx = context.applicationContext
        // A few seconds past midnight, so "today" has certainly rolled over. An inexact alarm
        // is fine: a minute's delay doesn't matter and it needs no exact-alarm permission.
        val at = nextMidnight(System.currentTimeMillis(), TimeZone.getDefault()) + 5_000
        ctx.getSystemService(AlarmManager::class.java).setAndAllowWhileIdle(AlarmManager.RTC, at, pending(ctx))
    }

    fun cancelMidnight(context: Context) {
        val ctx = context.applicationContext
        ctx.getSystemService(AlarmManager::class.java).cancel(pending(ctx))
    }

    private fun pending(ctx: Context): PendingIntent = PendingIntent.getBroadcast(
        ctx, 0, Intent(ctx, WidgetRefreshReceiver::class.java).setAction(ALARM),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
}

/** The midnight alarm and the clock changes land here. */
class WidgetRefreshReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val ctx = context.applicationContext
        val pending = goAsync()
        CoroutineScope(Dispatchers.IO).launch {
            try {
                TaskListWidget.refreshAll(ctx)
                WidgetRefresh.scheduleMidnight(ctx)
                // All-day reminders fire at a local time, so a new zone or date moves them.
                if (intent.action != Intent.ACTION_BOOT_COMPLETED) Reminders.schedule(ctx)
            } finally {
                pending.finish()
            }
        }
    }
}
