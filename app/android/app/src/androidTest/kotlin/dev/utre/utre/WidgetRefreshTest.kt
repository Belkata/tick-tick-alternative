package dev.utre.utre

import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.utre.utre.widget.WidgetRefresh
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import java.util.Calendar
import java.util.TimeZone

/** The widget's midnight refresh fires at the next local midnight, whatever the zone. */
@RunWith(AndroidJUnit4::class)
class WidgetRefreshTest {
    private fun at(zone: TimeZone, y: Int, m: Int, d: Int, h: Int, min: Int): Long =
        Calendar.getInstance(zone).apply { clear(); set(y, m, d, h, min) }.timeInMillis

    @Test
    fun nextMidnightIsTheStartOfTheNextLocalDay() {
        val sofia = TimeZone.getTimeZone("Europe/Sofia")
        val nov27 = at(sofia, 2026, Calendar.NOVEMBER, 27, 0, 0)
        assertEquals(nov27, WidgetRefresh.nextMidnight(at(sofia, 2026, Calendar.NOVEMBER, 26, 14, 30), sofia))
        // Exactly at midnight, the next one is a day later.
        assertEquals(nov27, WidgetRefresh.nextMidnight(at(sofia, 2026, Calendar.NOVEMBER, 26, 0, 0), sofia))
        // The same instant is a different local day elsewhere.
        val la = TimeZone.getTimeZone("America/Los_Angeles")
        val now = at(sofia, 2026, Calendar.NOVEMBER, 26, 1, 0)
        assertEquals(at(la, 2026, Calendar.NOVEMBER, 26, 0, 0), WidgetRefresh.nextMidnight(now, la))
    }

    @Test
    fun nextMidnightSurvivesTheClockChange() {
        // Europe/Sofia springs forward on Sunday 28 March 2027: that day has 23 hours.
        val sofia = TimeZone.getTimeZone("Europe/Sofia")
        assertEquals(
            at(sofia, 2027, Calendar.MARCH, 29, 0, 0),
            WidgetRefresh.nextMidnight(at(sofia, 2027, Calendar.MARCH, 28, 1, 0), sofia),
        )
    }
}
