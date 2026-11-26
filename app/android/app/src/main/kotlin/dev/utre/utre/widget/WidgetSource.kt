package dev.utre.utre.widget

import android.content.Context
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.longPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.utre.core.WidgetList
import java.util.TimeZone

/**
 * What one widget instance shows: the Inbox, or a list, tag or saved filter. Stored in the
 * widget's Glance state (`kind` + `id`), so every placed widget has its own.
 */
data class Source(val kind: String, val id: String) {
    companion object {
        val INBOX = Source("inbox", "")
        val kindKey = stringPreferencesKey("source_kind")
        val idKey = stringPreferencesKey("source_id")
        val versionKey = longPreferencesKey("version")

        fun from(prefs: Preferences): Source {
            val kind = prefs[kindKey] ?: return INBOX
            return Source(kind, prefs[idKey] ?: "")
        }
    }
}

/** A source's tasks, or the Inbox's when the list, tag or filter was deleted since. */
fun loadSource(context: Context, source: Source): Result<WidgetList> = runCatching {
    val db = UtreCore.db(context)
    val zone = TimeZone.getDefault().id
    try {
        db.tasks(source.kind, source.id, zone)
    } catch (e: Exception) {
        if (source == Source.INBOX) throw e
        db.tasks(Source.INBOX.kind, Source.INBOX.id, zone)
    }
}
