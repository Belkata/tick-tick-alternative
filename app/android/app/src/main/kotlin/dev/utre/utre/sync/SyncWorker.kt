package dev.utre.utre.sync

import android.app.NotificationChannel
import android.app.NotificationManager
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
import dev.utre.core.backgroundSync
import dev.utre.utre.R
import dev.utre.utre.google.GoogleAuth
import dev.utre.utre.reminders.Reminders
import dev.utre.utre.widget.TaskListWidget
import dev.utre.utre.widget.UtreCore
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
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
        // Its own quiet channel: this is not a reminder and must not pop up like one.
        val nm = applicationContext.getSystemService(NotificationManager::class.java)
        if (nm.getNotificationChannel(SYNC_CHANNEL) == null) {
            nm.createNotificationChannel(
                NotificationChannel(SYNC_CHANNEL, "Syncing", NotificationManager.IMPORTANCE_MIN),
            )
        }
        val n = NotificationCompat.Builder(applicationContext, SYNC_CHANNEL)
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle("Syncing tasks")
            .setSilent(true)
            .build()
        return ForegroundInfo(0x7a11, n)
    }

    override suspend fun doWork(): Result = withContext(Dispatchers.IO) {
        val ctx = applicationContext
        try {
            // The job may run with no Activity: install the Google service so the fetch below works.
            GoogleAuth.init(ctx)
            // The Rust round can't be cancelled and blocks its thread until it ends. One at a
            // time, so the periodic job, expedited jobs and a replaced worker never overlap.
            val summary = syncLock.withLock { backgroundSync(UtreCore.dbPath(ctx), keyPath(ctx), deviceName()) }
            android.util.Log.i("Utre", "background sync: $summary")
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            android.util.Log.w("Utre", "background sync: ${e.message}")
        }
        TaskListWidget.refreshAll(ctx)
        Reminders.schedule(ctx)
        Result.success()
    }

    companion object {
        private const val NAME = "utre-sync"
        private const val SYNC_CHANNEL = "sync"

        /** Serialises the native sync rounds of every worker in this process. */
        private val syncLock = Mutex()

        fun enqueue(context: Context) {
            val request = PeriodicWorkRequestBuilder<SyncWorker>(15, TimeUnit.MINUTES)
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .build()
            WorkManager.getInstance(context)
                .enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.KEEP, request)
        }

        /**
         * One sync as soon as the network allows (plan/07: after an edit made outside the
         * app, e.g. a widget tick or a reminder's Done).
         *
         * APPEND_OR_REPLACE, not REPLACE: REPLACE cancels the running worker, but its native
         * round keeps going, so the new worker would start a second round on top of it. Appending
         * queues the new run behind the running one (an edit made mid-round may have missed it,
         * so it can't be dropped, which rules out KEEP) and, unlike plain APPEND, starts afresh
         * when the previous run failed or was cancelled. The lock in doWork also covers the
         * 15-minute periodic job, which is a different unique work name.
         */
        fun syncSoon(context: Context) {
            val request = OneTimeWorkRequestBuilder<SyncWorker>()
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
                .build()
            WorkManager.getInstance(context)
                .enqueueUniqueWork("$NAME-soon", ExistingWorkPolicy.APPEND_OR_REPLACE, request)
        }

        /** The node key lives next to the database, as on desktop (utre.db -> utre.key). */
        fun keyPath(context: Context): String = UtreCore.dbPath(context).removeSuffix(".db") + ".key"

        /** What paired devices call this phone. */
        fun deviceName(): String =
            listOf(Build.MANUFACTURER, Build.MODEL).distinct().joinToString(" ").trim().ifEmpty { "phone" }
    }
}
