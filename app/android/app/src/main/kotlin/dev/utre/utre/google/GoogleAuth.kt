package dev.utre.utre.google

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import com.google.android.gms.auth.api.identity.AuthorizationClient
import com.google.android.gms.auth.api.identity.AuthorizationRequest
import com.google.android.gms.auth.api.identity.ClearTokenRequest
import com.google.android.gms.auth.api.identity.Identity
import com.google.android.gms.common.api.ApiException
import com.google.android.gms.common.api.CommonStatusCodes
import com.google.android.gms.common.api.Scope
import com.google.android.gms.tasks.Tasks
import dev.utre.core.AccessTokenSource
import dev.utre.core.TokenAnswer
import dev.utre.core.googleInit
import dev.utre.core.googleSignedIn
import dev.utre.utre.widget.UtreCore
import io.flutter.plugin.common.MethodChannel
import java.util.concurrent.ExecutionException
import java.util.concurrent.TimeUnit

/**
 * Google Calendar sign-in on the phone (M5b slice 5, plan/19).
 *
 * Google Identity's `AuthorizationClient` keeps the user's grant inside Google Play services
 * and hands out one-hour access tokens, so the app never holds a refresh token or a client
 * secret. The Rust core asks for a token through [accessToken] (silently, from any thread),
 * and the consent screen runs from the Activity through [signIn] and [onActivityResult].
 *
 * The OAuth client itself is not referenced in code: Google matches an "Android" client in
 * the user's Cloud project by this package name and the APK's signing-key SHA-1. The id in
 * the manifest only tells the app that the client was set up (see build.gradle.kts).
 */
object GoogleAuth : AccessTokenSource {
    private const val REQUEST_CODE = 0x6006
    private val SCOPES = listOf(
        Scope("https://www.googleapis.com/auth/calendar.readonly"),
        Scope("https://www.googleapis.com/auth/calendar.app.created"),
    )

    @Volatile private var appContext: Context? = null

    /** The consent screen's Flutter reply, while the user is on it. */
    private var pending: MethodChannel.Result? = null

    /** Installs the core's Google service (idempotent). Call before Flutter or a job needs it. */
    fun init(context: Context) {
        val ctx = context.applicationContext
        appContext = ctx
        googleInit(UtreCore.dbPath(ctx), this, isConfigured(ctx))
    }

    /** True when the build was given an Android OAuth client id. */
    fun isConfigured(context: Context): Boolean = try {
        val info = context.packageManager.getApplicationInfo(context.packageName, PackageManager.GET_META_DATA)
        !info.metaData?.getString("dev.utre.google_android_client_id").isNullOrBlank()
    } catch (_: Exception) {
        false
    }

    private fun request(): AuthorizationRequest =
        AuthorizationRequest.builder().setRequestedScopes(SCOPES).build()

    private fun client(context: Context): AuthorizationClient = Identity.getAuthorizationClient(context)

    /**
     * An access token without any UI, for the core. Blocks on Play services, so it runs on
     * the core's own threads, never the main one.
     */
    override fun accessToken(stale: String?): TokenAnswer {
        val ctx = appContext ?: return TokenAnswer.Unavailable("the app isn't ready")
        return try {
            val client = client(ctx)
            if (stale != null) {
                // Google refused this token: have Play services drop it so it isn't handed out again.
                Tasks.await(client.clearToken(ClearTokenRequest.builder().setToken(stale).build()), 20, TimeUnit.SECONDS)
            }
            val result = Tasks.await(client.authorize(request()), 30, TimeUnit.SECONDS)
            val token = result.accessToken
            if (result.hasResolution() || token == null) TokenAnswer.ConsentNeeded else TokenAnswer.Token(token)
        } catch (e: ExecutionException) {
            val api = e.cause as? ApiException
            when (api?.statusCode) {
                CommonStatusCodes.SIGN_IN_REQUIRED -> TokenAnswer.ConsentNeeded
                null -> TokenAnswer.Unavailable(e.cause?.message ?: "Google didn't answer")
                else -> TokenAnswer.Unavailable(describe(api))
            }
        } catch (e: Exception) {
            TokenAnswer.Unavailable(e.message ?: "Google didn't answer")
        }
    }

    private fun describe(e: ApiException): String = when (e.statusCode) {
        CommonStatusCodes.DEVELOPER_ERROR ->
            "Google doesn't know this build: the Android OAuth client needs this package name and signing key"
        CommonStatusCodes.NETWORK_ERROR -> "no network"
        else -> "Google Play services: ${CommonStatusCodes.getStatusCodeString(e.statusCode)}"
    }

    /**
     * The consent screen, for the Google page's button. Replies to Flutter with `ok`,
     * `cancelled`, `not_configured`, or `error: ...`; the caller fetches events after `ok`.
     */
    fun signIn(activity: Activity, result: MethodChannel.Result) {
        if (!isConfigured(activity)) {
            result.success("not_configured")
            return
        }
        init(activity)
        client(activity).authorize(request())
            .addOnSuccessListener { r ->
                val intent = r.pendingIntent
                if (r.hasResolution() && intent != null) {
                    pending?.success("cancelled")
                    pending = result
                    try {
                        activity.startIntentSenderForResult(intent.intentSender, REQUEST_CODE, null, 0, 0, 0)
                    } catch (e: Exception) {
                        pending = null
                        result.success("error: ${e.message}")
                    }
                } else {
                    finish(r.accessToken, result)
                }
            }
            .addOnFailureListener { e ->
                result.success("error: " + ((e as? ApiException)?.let { describe(it) } ?: e.message))
            }
    }

    /** Wire this to the Activity's `onActivityResult`. Returns true when the result was ours. */
    fun onActivityResult(activity: Activity, requestCode: Int, data: Intent?): Boolean {
        if (requestCode != REQUEST_CODE) return false
        val reply = pending ?: return true
        pending = null
        try {
            finish(client(activity).getAuthorizationResultFromIntent(data).accessToken, reply)
        } catch (_: ApiException) {
            reply.success("cancelled")
        }
        return true
    }

    private fun finish(token: String?, reply: MethodChannel.Result) {
        if (token == null) {
            reply.success("cancelled")
            return
        }
        try {
            googleSignedIn(token)
            reply.success("ok")
        } catch (e: Exception) {
            reply.success("error: ${e.message}")
        }
    }
}
