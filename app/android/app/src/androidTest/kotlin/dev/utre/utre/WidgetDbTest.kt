package dev.tickalt.tick_alt

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.tickalt.core.WidgetDb
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/** Spike 3: Kotlin -> UniFFI -> Rust core -> SQLite, on a real device or emulator. */
@RunWith(AndroidJUnit4::class)
class WidgetDbTest {
    private fun freshDb(): WidgetDb {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val file = File(ctx.cacheDir, "widget-test.db")
        listOf("", "-wal", "-shm").forEach { File(file.path + it).delete() }
        return WidgetDb.open(file.path)
    }

    @Test
    fun addListAndComplete() {
        freshDb().use { db ->
            assertTrue(db.inbox().isEmpty())

            val task = db.add("Water the plants")
            assertEquals(listOf("Water the plants"), db.inbox().map { it.title })

            assertTrue(db.complete(task.id))
            assertFalse("second complete is a no-op", db.complete(task.id))
            assertTrue(db.inbox().isEmpty())
        }
    }

    @Test
    fun twoConnectionsSeeEachOthersWrites() {
        // The widget and the Flutter UI each hold their own connection to the same file.
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val path = File(ctx.cacheDir, "shared-test.db").path
        listOf("", "-wal", "-shm").forEach { File(path + it).delete() }
        WidgetDb.open(path).use { app ->
            WidgetDb.open(path).use { widget ->
                val t = app.add("From the app")
                assertEquals(listOf(t.id), widget.inbox().map { it.id })
                widget.complete(t.id)
                assertTrue(app.inbox().isEmpty())
            }
        }
    }
}
