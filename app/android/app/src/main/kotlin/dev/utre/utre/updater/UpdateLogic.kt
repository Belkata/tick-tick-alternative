package dev.utre.utre.updater

import org.json.JSONArray
import org.json.JSONException

/** A release version, `major.minor.patch`. */
data class Version(val major: Int, val minor: Int, val patch: Int) : Comparable<Version> {
    override fun compareTo(other: Version): Int =
        compareValuesBy(this, other, Version::major, Version::minor, Version::patch)

    override fun toString() = "$major.$minor.$patch"

    companion object {
        private val NAME = Regex("""^(\d+)\.(\d+)\.(\d+)(?:[-+].*)?$""")
        private val TAG = Regex("""^v(\d+)\.(\d+)\.(\d+)$""")

        /** An app version name such as `0.8.10` (a `-suffix` is ignored). */
        fun parse(name: String?): Version? = NAME.matchEntire(name?.trim() ?: return null)?.toVersion()

        /** A release tag: exactly `vX.Y.Z`, so `arch-repo` and the like never match. */
        fun parseTag(tag: String?): Version? = TAG.matchEntire(tag ?: return null)?.toVersion()

        private fun MatchResult.toVersion(): Version? = try {
            Version(groupValues[1].toInt(), groupValues[2].toInt(), groupValues[3].toInt())
        } catch (e: NumberFormatException) {
            null
        }
    }
}

/** The release asset to install. */
data class Update(val version: Version, val url: String, val size: Long)

object UpdateLogic {
    /** The asset suffix for a device ABI: `utre-X.Y.Z-arm64.apk`. Only arm64 is published. */
    fun assetSuffixForAbi(abi: String?): String? = if (abi == "arm64-v8a") "arm64" else null

    /**
     * The newest release that has an APK for [suffix], from the GitHub releases list.
     * Drafts and tags that are not `vX.Y.Z` are skipped; pre-releases count. Null when
     * nothing matches or the JSON is not a release list.
     */
    fun pick(json: String, suffix: String): Update? {
        val releases = try {
            JSONArray(json)
        } catch (e: JSONException) {
            return null
        }
        var best: Update? = null
        for (i in 0 until releases.length()) {
            val r = releases.optJSONObject(i) ?: continue
            if (r.optBoolean("draft", false)) continue
            val version = Version.parseTag(r.optString("tag_name", "")) ?: continue
            if (best != null && version <= best.version) continue
            val assets = r.optJSONArray("assets") ?: continue
            val wanted = "utre-$version-$suffix.apk"
            for (j in 0 until assets.length()) {
                val a = assets.optJSONObject(j) ?: continue
                if (a.optString("name") != wanted) continue
                val url = a.optString("browser_download_url", "")
                val size = a.optLong("size", -1)
                if (url.isEmpty() || size <= 0) continue
                best = Update(version, url, size)
                break
            }
        }
        return best
    }

    /** PackageInstaller.STATUS_FAILURE_INCOMPATIBLE, spelled out so plain JVM tests can use it. */
    const val STATUS_INCOMPATIBLE = 7

    /**
     * True when an install failed because the new APK is signed with a different key than
     * the installed app (the switch from the debug key to the release key). Android reports
     * STATUS_FAILURE_INCOMPATIBLE, and some builds only put INSTALL_FAILED_UPDATE_INCOMPATIBLE
     * (or a "signatures do not match" text) in the status message.
     */
    fun isIncompatibleSignature(status: Int, message: String?): Boolean {
        if (status == STATUS_INCOMPATIBLE) return true
        val m = message ?: return false
        return m.contains("INSTALL_FAILED_UPDATE_INCOMPATIBLE") ||
            m.contains("INSTALL_PARSE_FAILED_INCONSISTENT_CERTIFICATES") ||
            m.contains("signatures do not match", ignoreCase = true)
    }

    /** Newer than what is installed, or null. */
    fun newer(update: Update?, installed: Version?): Update? =
        if (update != null && installed != null && update.version > installed) update else null
}
