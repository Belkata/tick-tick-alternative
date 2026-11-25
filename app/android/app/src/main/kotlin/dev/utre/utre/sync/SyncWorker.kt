package dev.tickalt.tick_alt.sync

import android.content.Context
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.ForegroundInfo
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import dev.tickalt.core.backgroundSync
import dev.tickalt.tick_alt.R
import dev.tickalt.tick_alt.reminders.Reminders
import dev.tickalt.tick_alt.widget.TaskListWidget
import dev.tickalt.tick_alt.widget.TtCore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.util.concurrent.TimeUnit

/**
 * Background sync (M4, plan/05 "When does syncing happen?"): every 15 minutes when online,
 * one round with every paired device through the core's sync node (the app's own, if it's
 * running in this process). Then reminders and the widget catch up with what came in.
 */
class SyncWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    // Expedited work on Android 11 and older runs as a foreground service and needs this.
    override suspend fun getForegroundInfo(): ForegroundInfo {
        Reminders.ensureChannel(applicationContext)
        val n = NotificationCompat.Builder(applicationContext, "reminders")
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle("Syncing tasks")
            .setSilent(true)
            .build()
        return ForegroundInfo(0x7a11, n)
    }

    override suspend fun doWork(): Result = withContext(Dispatchers.IO) {
        val ctx = applicationContext
        try {
            val summary = backgroundSync(TtCore.dbPath(ctx), keyPath(ctx), deviceName())
            android.util.Log.i("TickAlt", "background sync: $summary")
        } catch (e: Exception) {
            android.util.Log.w("TickAlt", "background sync: ${e.message}")
        }
        TaskListWidget.refreshAll(ctx)
        Reminders.schedule(ctx)
        Result.success()
    }

    companion object {
        private const val NAME = "tick-alt-sync"

        fun enqueue(context: Context) {
            val request = PeriodicWorkRequestBuilder<SyncWorker>(15, TimeUnit.MINUTES)
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .build()
            WorkManager.getInstance(context)
                .enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.KEEP, request)
        }

        /**
         * One sync as soon as the network allows (plan/07: after an edit made outside the
         * app, e.g. a widget tick or a reminder's Done). Edits close together share one job.
         */
        fun syncSoon(context: Context) {
            val request = OneTimeWorkRequestBuilder<SyncWorker>()
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
                .build()
            WorkManager.getInstance(context)
                .enqueueUniqueWork("$NAME-soon", ExistingWorkPolicy.REPLACE, request)
        }

        /** The node key lives next to the database, as on desktop (tt.db -> tt.key). */
        fun keyPath(context: Context): String = TtCore.dbPath(context).removeSuffix(".db") + ".key"

        /** What paired devices call this phone. */
        fun deviceName(): String =
            listOf(Build.MANUFACTURER, Build.MODEL).distinct().joinToString(" ").trim().ifEmpty { "phone" }
    }
}
