package dev.utre.utre.updater

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class UpdateLogicTest {
    private fun asset(name: String, size: Long = 100) =
        """{"name":"$name","size":$size,"browser_download_url":"https://example.test/$name"}"""

    private fun release(tag: String, vararg assets: String, draft: Boolean = false, pre: Boolean = true) =
        """{"tag_name":"$tag","draft":$draft,"prerelease":$pre,"assets":[${assets.joinToString(",")}]}"""

    private fun feed(vararg releases: String) = "[${releases.joinToString(",")}]"

    @Test fun versionsCompareNumerically() {
        assertTrue(Version.parse("0.8.10")!! > Version.parse("0.8.9")!!)
        assertTrue(Version.parse("1.0.0")!! > Version.parse("0.99.99")!!)
        assertEquals(Version(0, 8, 10), Version.parse("0.8.10"))
        assertEquals(Version(0, 8, 10), Version.parse("0.8.10-debug"))
        assertNull(Version.parse("0.8"))
        assertNull(Version.parse(null))
    }

    @Test fun onlyPlainVersionTagsCount() {
        assertEquals(Version(0, 9, 0), Version.parseTag("v0.9.0"))
        assertNull(Version.parseTag("arch-repo"))
        assertNull(Version.parseTag("v0.9"))
        assertNull(Version.parseTag("0.9.0"))
        assertNull(Version.parseTag("v0.9.0-rc1"))
    }

    @Test fun abiMapsToAssetSuffix() {
        assertEquals("arm64", UpdateLogic.assetSuffixForAbi("arm64-v8a"))
        assertNull(UpdateLogic.assetSuffixForAbi("x86_64"))
        assertNull(UpdateLogic.assetSuffixForAbi(null))
    }

    @Test fun picksHighestSemverWithMatchingAsset() {
        val json = feed(
            release("arch-repo", asset("utre-9.9.9-arm64.apk")),
            release("v0.8.9", asset("utre-0.8.9-arm64.apk")),
            release("v0.8.10", asset("utre-0.8.10-arm64.apk", 555), asset("utre-0.8.10-macos.dmg")),
            release("v0.8.11", asset("utre-0.8.11-arm64.apk"), draft = true),
        )
        val u = UpdateLogic.pick(json, "arm64")!!
        assertEquals(Version(0, 8, 10), u.version)
        assertEquals(555L, u.size)
        assertEquals("https://example.test/utre-0.8.10-arm64.apk", u.url)
    }

    @Test fun releaseWithoutAbiAssetIsSkipped() {
        val json = feed(
            release("v0.9.0", asset("utre-0.9.0-macos.dmg")),
            release("v0.8.9", asset("utre-0.8.9-arm64.apk")),
        )
        assertEquals(Version(0, 8, 9), UpdateLogic.pick(json, "arm64")!!.version)
        assertNull(UpdateLogic.pick(json, "x64"))
    }

    @Test fun nonStableOrderStillFindsMax() {
        val json = feed(
            release("v0.8.2", asset("utre-0.8.2-arm64.apk")),
            release("v0.10.0", asset("utre-0.10.0-arm64.apk")),
            release("v0.9.5", asset("utre-0.9.5-arm64.apk")),
        )
        assertEquals(Version(0, 10, 0), UpdateLogic.pick(json, "arm64")!!.version)
    }

    @Test fun badJsonIsNothing() {
        assertNull(UpdateLogic.pick("{\"message\":\"rate limited\"}", "arm64"))
        assertNull(UpdateLogic.pick("not json", "arm64"))
        assertNull(UpdateLogic.pick("[]", "arm64"))
    }

    @Test fun assetWithoutSizeIsIgnored() {
        val json = feed(release("v0.9.0", asset("utre-0.9.0-arm64.apk", 0)))
        assertNull(UpdateLogic.pick(json, "arm64"))
    }

    @Test fun newerOnlyWhenHigher() {
        val u = Update(Version(0, 8, 10), "u", 1)
        assertNotNull(UpdateLogic.newer(u, Version(0, 8, 9)))
        assertNull(UpdateLogic.newer(u, Version(0, 8, 10)))
        assertNull(UpdateLogic.newer(u, Version(0, 9, 0)))
        assertNull(UpdateLogic.newer(null, Version(0, 1, 0)))
    }
}
