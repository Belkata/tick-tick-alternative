package dev.tickalt.tick_alt.widget

import android.content.Context
import dev.tickalt.core.WidgetDb

/**
 * One process-wide connection to the Rust core for widget code.
 *
 * The path must match the one Flutter uses: path_provider's
 * getApplicationSupportDirectory() is Context.filesDir on Android.
 */
object TtCore {
    @Volatile private var db: WidgetDb? = null

    fun db(context: Context): WidgetDb =
        db ?: synchronized(this) {
            db ?: WidgetDb.open(dbPath(context)).also { db = it }
        }

    fun dbPath(context: Context): String = "${context.applicationContext.filesDir.absolutePath}/tt.db"
}
