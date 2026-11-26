package dev.utre.utre

import android.content.Context
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.utre.utre.reminders.Reminders
import org.json.JSONArray
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

/** Snoozes are kept in the prefs so a reboot or the next reschedule re-arms them. */
@RunWith(AndroidJUnit4::class)
class RemindersTest {
    private val ctx: Context = InstrumentationRegistry.getInstrumentation().targetContext
    private fun stored(): JSONArray =
        JSONArray(ctx.getSharedPreferences("reminders", Context.MODE_PRIVATE).getString("snoozes", "[]"))

    @Test
    fun snoozeIsStoredAndCancelledOnce() {
        Reminders.snooze(ctx, "task-a", "Water the plants", Reminders.SNOOZE_MS)
        Reminders.snooze(ctx, "task-a", "Water the plants", Reminders.SNOOZE_LONG_MS)
        Reminders.snooze(ctx, "task-b", "Call mum", Reminders.SNOOZE_MS)
        val ids = (0 until stored().length()).map { stored().getJSONObject(it).getString("task") }.sorted()
        // Snoozing the same task again replaces the first snooze.
        assertEquals(listOf("task-a", "task-b"), ids)

        Reminders.cancelSnooze(ctx, "task-a")
        Reminders.cancelSnooze(ctx, "task-b")
        assertEquals(0, stored().length())
    }

    @Test
    fun staleNotificationsAreTheOnesWithoutALiveTask() {
        val shown = listOf(1 to "done-elsewhere", 2 to "still-open", 3 to "done-elsewhere")
        assertEquals(listOf(1, 3), Reminders.staleIds(shown, setOf("still-open")))
        assertEquals(emptyList<Int>(), Reminders.staleIds(shown, setOf("done-elsewhere", "still-open")))
    }
}
