package dev.utre.utre.widget

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.view.Gravity
import android.view.WindowManager
import android.view.inputmethod.EditorInfo
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView
import dev.utre.utre.sync.SyncWorker
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import java.util.TimeZone

/**
 * The widget's **+**: a one-line dialog over the home screen, no app start. The task is
 * written through the core (so it syncs) into what the widget shows: the list or tag, what a
 * filter asks for, else the Inbox. No parsing, as in the app (plan/01).
 */
class QuickAddActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val source = Source(intent.getStringExtra(KIND) ?: "inbox", intent.getStringExtra(ID) ?: "")
        val dp = resources.displayMetrics.density
        fun px(v: Int) = (v * dp).toInt()

        val input = EditText(this).apply {
            hint = "Task title"
            setSingleLine()
            imeOptions = EditorInfo.IME_ACTION_DONE
            // A dismissed keyboard would leave an empty dialog; keep it up.
            setSelectAllOnFocus(true)
        }
        val add = Button(this).apply { text = "Add" }
        var submitted = false
        val submit = {
            val title = input.text.toString().trim()
            if (title.isEmpty()) {
                finish()
            } else if (!submitted) {
                // Enter on the keyboard and the button must not both add the task.
                submitted = true
                add.isEnabled = false
                val app = applicationContext
                // Not tied to this Activity: a rotation or a back press mustn't drop the redraw and sync.
                CoroutineScope(Dispatchers.IO).launch {
                    addTask(app, source, title)
                    TaskListWidget.refreshAll(app)
                    SyncWorker.syncSoon(app)
                }
                finish()
            }
        }
        add.setOnClickListener { submit() }
        input.setOnEditorActionListener { _, _, _ -> submit(); true }

        val heading = TextView(this).apply {
            text = "Add to ${intent.getStringExtra(NAME) ?: "Inbox"}"
            textSize = 16f
            setTypeface(typeface, android.graphics.Typeface.BOLD)
        }
        val row = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(input, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
            addView(add)
        }
        setContentView(
            LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(px(20), px(16), px(20), px(8))
                addView(heading)
                addView(row)
            },
        )
        window.setLayout(
            (resources.displayMetrics.widthPixels * 0.92).toInt().coerceAtMost(px(480)),
            WindowManager.LayoutParams.WRAP_CONTENT,
        )
        window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_STATE_VISIBLE)
        input.requestFocus()
    }

    companion object {
        private const val KIND = "kind"
        private const val ID = "id"
        private const val NAME = "name"

        fun intent(context: Context, source: Source, name: String?): Intent =
            Intent(context, QuickAddActivity::class.java)
                .putExtra(KIND, source.kind)
                .putExtra(ID, source.id)
                .putExtra(NAME, name)
                // One distinct intent per widget source: Glance turns it into a PendingIntent.
                .setData(android.net.Uri.parse("utre://quickadd/${source.kind}/${source.id}"))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

        /** Add through the core; a deleted list, tag or filter falls back to the Inbox. */
        fun addTask(context: Context, source: Source, title: String) {
            val db = UtreCore.db(context)
            val zone = TimeZone.getDefault().id
            try {
                db.addIn(source.kind, source.id, title, zone)
            } catch (e: Exception) {
                db.addIn(Source.INBOX.kind, Source.INBOX.id, title, zone)
            }
        }
    }
}
