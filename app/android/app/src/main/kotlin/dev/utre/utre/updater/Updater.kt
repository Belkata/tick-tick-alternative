package dev.utre.utre.updater

import android.app.Activity
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.app.NotificationCompat
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import dev.utre.utre.BuildConfig
import dev.utre.utre.MainActivity
import dev.utre.utre.R
import dev.utre.utre.sync.SyncWorker
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.withContext
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.TimeUnit

/**
 * The phone updating itself from GitHub releases (plan/17). A daily job and a check at app
 * start (at most every 6 hours) look for a newer `utre-X.Y.Z-arm64.apk`, download it on an
 * unmetered network and hand it to PackageInstaller. Android refuses an APK signed with a
 * different key, which is the authenticity check. Once Utre is the installer of record, an
 * update on Android 12 and newer needs no confirmation (USER_ACTION_NOT_REQUIRED).
 */
object Updater {
    private const val TAG = "UtreUpdater"
    private const val PREFS = "updater"
    private const val CHANNEL = "updates"
    private const val NOTE_READY = 0x7a21
    private const val NOTE_DONE = 0x7a22
    private const val NOTE_FAIL = 0x7a23
    private const val NOTE_KEY = 0x7a24
    private const val CHECK_EVERY_MS = 6 * 60 * 60 * 1000L
    private const val FEED = "https://api.github.com/repos/Belkata/utre/releases?per_page=30"

    /** Set by MainActivity: true while the app is on screen. */
    @Volatile var foreground = false

    private val lock = Mutex()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    private fun prefs(ctx: Context) = ctx.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    fun autoEnabled(ctx: Context) = prefs(ctx).getBoolean("auto", true)

    private fun feedUrl() = BuildConfig.UPDATE_FEED.ifEmpty { FEED }

    private fun assetSuffix(): String? =
        BuildConfig.UPDATE_ASSET_SUFFIX.ifEmpty { null }
            ?: UpdateLogic.assetSuffixForAbi(Build.SUPPORTED_ABIS.firstOrNull())

    fun installed(ctx: Context): Version? = Version.parse(
        ctx.packageManager.getPackageInfo(ctx.packageName, 0).versionName,
    )

    private fun set(ctx: Context, status: String, latest: String? = null, message: String? = null) {
        prefs(ctx).edit().putString("status", status).putString("latest", latest).putString("message", message).apply()
    }

    /** What the Updates page shows. */
    fun state(ctx: Context): Map<String, Any?> {
        val p = prefs(ctx)
        return mapOf(
            "status" to (p.getString("status", null) ?: "idle"),
            "latest" to p.getString("latest", null),
            "message" to p.getString("message", null),
            "current" to installed(ctx)?.toString(),
            "auto" to autoEnabled(ctx),
            "canInstall" to ctx.packageManager.canRequestPackageInstalls(),
            "lastCheckMs" to p.getLong("lastCheck", 0),
        )
    }

    /** The Flutter channel's updater calls; false when [call] is not one of them. */
    fun handle(activity: Activity, call: MethodCall, result: MethodChannel.Result): Boolean {
        val ctx = activity.applicationContext
        when (call.method) {
            "updateState" -> result.success(state(ctx))
            "updateCheck" -> {
                checkNow(ctx)
                result.success(null)
            }
            "updateSetAuto" -> {
                prefs(ctx).edit().putBoolean("auto", call.arguments as? Boolean ?: true).apply()
                result.success(null)
            }
            "updateInstallSettings" -> {
                activity.startActivity(installSettingsIntent(ctx))
                result.success(null)
            }
            else -> return false
        }
        return true
    }

    fun installSettingsIntent(ctx: Context): Intent =
        Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${ctx.packageName}"))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

    // ---- scheduling ----

    private fun unmetered() = Constraints.Builder().setRequiredNetworkType(NetworkType.UNMETERED).build()

    /** The daily check; also called again after the app was replaced. */
    fun enqueueDaily(ctx: Context) {
        val request = PeriodicWorkRequestBuilder<UpdateWorker>(1, TimeUnit.DAYS)
            .setConstraints(unmetered())
            .build()
        WorkManager.getInstance(ctx)
            .enqueueUniquePeriodicWork("utre-update", ExistingPeriodicWorkPolicy.KEEP, request)
    }

    /** At app start: a check when the last one is older than 6 hours. */
    fun checkOnStart(ctx: Context) {
        if (!autoEnabled(ctx)) return
        if (System.currentTimeMillis() - prefs(ctx).getLong("lastCheck", 0) < CHECK_EVERY_MS) return
        val request = OneTimeWorkRequestBuilder<UpdateWorker>().setConstraints(unmetered()).build()
        WorkManager.getInstance(ctx).enqueueUniqueWork("utre-update-start", ExistingWorkPolicy.KEEP, request)
    }

    // ---- the check ----

    /** "Check for updates": any network, and failures are shown. */
    fun checkNow(ctx: Context) {
        val app = ctx.applicationContext
        scope.launch { run(app, manual = true) }
    }

    /** One check, and a download and install when there is something newer. */
    suspend fun run(ctx: Context, manual: Boolean) {
        if (!lock.tryLock()) return
        try {
            val p = prefs(ctx)
            p.edit().putBoolean("manual", manual).apply()
            set(ctx, "checking")
            val installed = installed(ctx)
            val suffix = assetSuffix()
            if (suffix == null) {
                p.edit().putLong("lastCheck", System.currentTimeMillis()).apply()
                set(ctx, "uptodate")
                return
            }
            val update = try {
                withContext(Dispatchers.IO) { UpdateLogic.newer(UpdateLogic.pick(fetch(feedUrl()), suffix), installed) }
            } catch (e: Exception) {
                android.util.Log.w(TAG, "check failed: $e")
                set(ctx, "error", message = "Could not check for updates (${e.message ?: e.javaClass.simpleName})")
                return
            }
            p.edit().putLong("lastCheck", System.currentTimeMillis()).apply()
            if (update == null) {
                set(ctx, "uptodate")
                return
            }
            val v = update.version.toString()
            if (p.getString("wrongKey", null) == v) {
                // This version was already refused for its signing key: don't download it again.
                set(ctx, "wrong_key", v, wrongKeyText(v))
                return
            }
            if (!ctx.packageManager.canRequestPackageInstalls()) {
                set(ctx, "needs_permission", v)
                notifyReady(ctx, v, installSettingsIntent(ctx), "Allow installs, then check again")
                return
            }
            set(ctx, "downloading", v)
            val file = try {
                withContext(Dispatchers.IO) { download(ctx, update) }
            } catch (e: Exception) {
                android.util.Log.w(TAG, "download failed: $e")
                fail(ctx, "Download failed (${e.message ?: e.javaClass.simpleName})", manual)
                return
            }
            try {
                withContext(Dispatchers.IO) { install(ctx, file, v) }
                android.util.Log.i(TAG, "install session committed for $v")
            } catch (e: Exception) {
                android.util.Log.w(TAG, "install failed: $e")
                fail(ctx, "Install failed (${e.message ?: e.javaClass.simpleName})", manual)
            } finally {
                file.delete()
            }
        } finally {
            lock.unlock()
        }
    }

    private fun fetch(url: String): String {
        val c = open(url)
        try {
            c.setRequestProperty("Accept", "application/vnd.github+json")
            if (c.responseCode != 200) throw java.io.IOException("HTTP ${c.responseCode}")
            return c.inputStream.bufferedReader().use { it.readText() }
        } finally {
            c.disconnect()
        }
    }

    private fun open(url: String): HttpURLConnection =
        (URL(url).openConnection() as HttpURLConnection).apply {
            connectTimeout = 15_000
            readTimeout = 30_000
            setRequestProperty("User-Agent", "utre-android")
        }

    /** To the app's no-backup storage; the size must match what the release says. */
    private fun download(ctx: Context, update: Update): File {
        val dir = File(ctx.noBackupFilesDir, "updates").apply { mkdirs() }
        dir.listFiles()?.forEach { it.delete() }
        val file = File(dir, "utre-${update.version}.apk")
        val c = open(update.url)
        try {
            if (c.responseCode != 200) throw java.io.IOException("HTTP ${c.responseCode}")
            c.inputStream.use { input -> file.outputStream().use { input.copyTo(it) } }
        } finally {
            c.disconnect()
        }
        if (file.length() != update.size) {
            file.delete()
            throw java.io.IOException("size ${file.length()} does not match ${update.size}")
        }
        return file
    }

    private fun install(ctx: Context, file: File, version: String) {
        val installer = ctx.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
        params.setAppPackageName(ctx.packageName)
        params.setSize(file.length())
        if (Build.VERSION.SDK_INT >= 31) {
            params.setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED)
        }
        val id = installer.createSession(params)
        installer.openSession(id).use { session ->
            session.openWrite("utre.apk", 0, file.length()).use { out ->
                file.inputStream().use { it.copyTo(out) }
                session.fsync(out)
            }
            prefs(ctx).edit().putString("installing", version).apply()
            set(ctx, "installing", version)
            val intent = Intent(ctx, InstallStatusReceiver::class.java)
            // The system fills in the status extras, so the intent has to be mutable.
            val pending = PendingIntent.getBroadcast(
                ctx, id, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE,
            )
            session.commit(pending.intentSender)
        }
    }

    // ---- results ----

    private fun fail(ctx: Context, text: String, manual: Boolean) {
        prefs(ctx).edit().remove("installing").apply()
        set(ctx, "error", message = text)
        if (manual) notify(ctx, NOTE_FAIL, "Utre update failed", text, null)
    }

    /** The system wants a tap: show it now if the app is open, else leave a notification. */
    fun onPendingUserAction(ctx: Context, confirm: Intent) {
        val v = prefs(ctx).getString("installing", null) ?: "update"
        set(ctx, "ready", v)
        confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (foreground) {
            ctx.startActivity(confirm)
        } else {
            notifyReady(ctx, v, confirm, null)
        }
    }

    fun onInstallStatus(ctx: Context, status: Int, message: String?) {
        val manual = prefs(ctx).getBoolean("manual", false)
        when (status) {
            PackageInstaller.STATUS_SUCCESS -> set(ctx, "uptodate")
            PackageInstaller.STATUS_FAILURE_BLOCKED -> {
                val v = prefs(ctx).getString("installing", null)
                set(ctx, "needs_permission", v)
                notifyReady(ctx, v ?: "update", installSettingsIntent(ctx), "Allow installs, then check again")
            }
            else -> if (UpdateLogic.isIncompatibleSignature(status, message)) {
                onWrongKey(ctx)
            } else {
                fail(ctx, "Install failed${message?.let { ": $it" } ?: ""}", manual)
            }
        }
    }

    private fun wrongKeyText(v: String) =
        "This update is signed with Utre's new release key. Sync your tasks, then uninstall Utre " +
            "and install $v from GitHub once; updates work again after that."

    /** The new APK has another signing key: remember the version so it isn't retried. */
    private fun onWrongKey(ctx: Context) {
        val p = prefs(ctx)
        val v = p.getString("installing", null) ?: p.getString("latest", null) ?: "update"
        val already = p.getString("wrongKey", null) == v
        p.edit().remove("installing").putString("wrongKey", v).apply()
        set(ctx, "wrong_key", v, wrongKeyText(v))
        // One notification per version, for manual and automatic checks alike.
        if (!already) notify(ctx, NOTE_KEY, "Utre $v needs a one-time reinstall", wrongKeyText(v), null)
    }

    /** The new build is running: tell the user, once, if this app started the update. */
    fun onReplaced(ctx: Context) {
        val p = prefs(ctx)
        val wanted = p.getString("installing", null)
        p.edit().remove("installing").apply()
        val now = installed(ctx)?.toString()
        set(ctx, "uptodate")
        if (wanted != null && now != null) {
            val open = PendingIntent.getActivity(
                ctx, 0, Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
            notify(ctx, NOTE_DONE, "Utre updated to $now", "Tap to open", open)
        }
    }

    private fun ensureChannel(ctx: Context) {
        val nm = ctx.getSystemService(NotificationManager::class.java)
        if (nm.getNotificationChannel(CHANNEL) == null) {
            nm.createNotificationChannel(NotificationChannel(CHANNEL, "Updates", NotificationManager.IMPORTANCE_DEFAULT))
        }
    }

    private fun notifyReady(ctx: Context, version: String, target: Intent, text: String?) {
        val open = if (target.component != null || target.action != null) {
            PendingIntent.getActivity(ctx, 1, target, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        } else null
        notify(ctx, NOTE_READY, "Utre $version is ready. Tap to install", text, open)
    }

    private fun notify(ctx: Context, id: Int, title: String, text: String?, tap: PendingIntent?) {
        try {
            ensureChannel(ctx)
            val n = NotificationCompat.Builder(ctx, CHANNEL)
                .setSmallIcon(R.drawable.ic_stat_utre)
                .setContentTitle(title)
                .setContentText(text)
                .setAutoCancel(true)
                .apply { if (tap != null) setContentIntent(tap) }
                .build()
            ctx.getSystemService(NotificationManager::class.java).notify(id, n)
        } catch (e: SecurityException) {
            android.util.Log.w(TAG, "no notification permission: $e")
        }
    }
}

/** The daily and at-start check, on an unmetered network. */
class UpdateWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        if (Updater.autoEnabled(applicationContext)) Updater.run(applicationContext, manual = false)
        return Result.success()
    }
}

/** PackageInstaller reports here: a confirmation is needed, or it finished. */
class InstallStatusReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
        val message = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE)
        android.util.Log.i("UtreUpdater", "install status $status $message")
        if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
            @Suppress("DEPRECATION")
            val confirm = if (Build.VERSION.SDK_INT >= 33) {
                intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
            } else {
                intent.getParcelableExtra(Intent.EXTRA_INTENT)
            }
            if (confirm != null) Updater.onPendingUserAction(context.applicationContext, confirm)
        } else {
            Updater.onInstallStatus(context.applicationContext, status, message)
        }
    }
}

/** This app was just replaced (an update, from here or elsewhere). */
class PackageReplacedReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val ctx = context.applicationContext
        // Reminder alarms and widgets are redrawn by ReminderReceiver and WidgetRefreshReceiver,
        // which listen for the same broadcast. WorkManager keeps its jobs, but be sure.
        SyncWorker.enqueue(ctx)
        Updater.enqueueDaily(ctx)
        Updater.onReplaced(ctx)
    }
}
