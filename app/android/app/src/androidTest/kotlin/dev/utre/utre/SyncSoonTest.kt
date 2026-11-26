package dev.utre.utre

import android.content.Context
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.work.WorkManager
import dev.utre.utre.sync.SyncWorker
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

/** Edits made outside the app queue one sync each behind the running one, never beside it. */
@RunWith(AndroidJUnit4::class)
class SyncSoonTest {
    private val ctx: Context = InstrumentationRegistry.getInstrumentation().targetContext

    @Test
    fun syncSoonChainsInsteadOfReplacing() {
        val wm = WorkManager.getInstance(ctx)
        wm.cancelUniqueWork("utre-sync-soon").result.get()
        SyncWorker.syncSoon(ctx)
        SyncWorker.syncSoon(ctx)
        SyncWorker.syncSoon(ctx)
        val infos = wm.getWorkInfosForUniqueWork("utre-sync-soon").get()
        // REPLACE would leave one live request; APPEND_OR_REPLACE keeps the chain.
        assertEquals(3, infos.count { !it.state.isFinished })
        wm.cancelUniqueWork("utre-sync-soon").result.get()
    }
}
