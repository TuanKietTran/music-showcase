package com.example.musicshowcase

// Mirror of docs/error-catalog.json. UNCOMPILED: written without an Android toolchain.
enum class ErrorCode(val id: String) {
    ServerUnreachable("server-unreachable"), RequestTimeout("request-timeout"),
    Unauthorized("unauthorized"), TokenRejected("token-rejected"), UnauthorizedMidSession("unauthorized-mid-session"),
    TrackNotFound("track-not-found"), BadRange("bad-range"), ServerError("server-error"),
    MalformedResponse("malformed-response"), AudioDecodeFailed("audio-decode-failed"),
    StreamStalled("stream-stalled"), ConnectionLostDuringPlayback("connection-lost-during-playback"),
    CoverLoadFailed("cover-load-failed"), EmptyLibrary("empty-library"), Unknown("unknown"),
}

data class CatalogEntry(
    val severity: String, val title: String, val message: String, val actions: List<String>,
    val autoRetry: Boolean, val maxAttempts: Int, val backoffMs: List<Long>, val assertive: Boolean,
    val silent: Boolean = false,
)

private val NET = listOf(1000L, 2000L, 4000L)

val ErrorCode.entry: CatalogEntry
    get() = when (this) {
        ErrorCode.ServerUnreachable -> CatalogEntry("error", "Can't reach the server", "We couldn't connect to the music server. Check that it is running and that you are online, then try again.", listOf("retry", "change-server", "dismiss"), true, 3, NET, true)
        ErrorCode.RequestTimeout -> CatalogEntry("warning", "The server is taking too long", "The server didn't answer in time. Your library is still here. Try again in a moment.", listOf("retry", "dismiss"), true, 3, NET, false)
        ErrorCode.Unauthorized -> CatalogEntry("blocking", "Access token needed", "This server needs an access token before it will share music. Enter the token to continue.", listOf("enter-token", "change-server"), false, 0, emptyList(), true)
        ErrorCode.TokenRejected -> CatalogEntry("warning", "That token wasn't accepted", "Check it and try again; it is the MUSIC_TOKEN the server was started with.", listOf("retry"), false, 0, emptyList(), true)
        ErrorCode.UnauthorizedMidSession -> CatalogEntry("blocking", "Please sign in again", "The server no longer accepts your token. Enter it again to keep listening. Your queue and position are kept.", listOf("enter-token", "dismiss"), false, 0, emptyList(), true)
        ErrorCode.TrackNotFound -> CatalogEntry("warning", "Track unavailable", "This track is no longer on the server, so we can't play it. You can skip to the next one.", listOf("skip-track", "dismiss"), false, 0, emptyList(), false)
        ErrorCode.BadRange -> CatalogEntry("warning", "Couldn't jump to that spot", "We couldn't seek to that point in the track. Playback restarts from where you were.", listOf("retry", "dismiss"), true, 1, listOf(500L), false)
        ErrorCode.ServerError -> CatalogEntry("error", "The server had a problem", "Something went wrong on the server's side. Nothing you did caused it. Try again shortly.", listOf("retry", "dismiss"), true, 3, listOf(1000L, 3000L, 8000L), false)
        ErrorCode.MalformedResponse -> CatalogEntry("error", "Unexpected reply from the server", "The server sent something we couldn't read. Trying again may help; if not, the server may need an update.", listOf("retry", "change-server", "dismiss"), false, 0, emptyList(), false)
        ErrorCode.AudioDecodeFailed -> CatalogEntry("error", "This track won't play", "We couldn't play this track's audio. Your queue is unchanged, so you can skip to the next track.", listOf("skip-track", "dismiss"), false, 0, emptyList(), false)
        ErrorCode.StreamStalled -> CatalogEntry("info", "Still loading the music", "The music is taking a while to load. We'll keep trying and resume from the same spot.", listOf("retry", "skip-track"), true, 3, listOf(2000L, 4000L, 8000L), false)
        ErrorCode.ConnectionLostDuringPlayback -> CatalogEntry("warning", "Connection lost", "Playback paused because the connection dropped. We'll resume from the same spot when you're back online.", listOf("retry", "skip-track", "dismiss"), true, 3, NET, false)
        ErrorCode.CoverLoadFailed -> CatalogEntry("info", "Cover art unavailable", "Cover art couldn't be loaded. A placeholder is shown instead.", emptyList(), false, 0, emptyList(), false, silent = true)
        ErrorCode.EmptyLibrary -> CatalogEntry("info", "No music yet", "The library is empty. Add some tracks on the server and refresh.", listOf("retry"), false, 0, emptyList(), false)
        ErrorCode.Unknown -> CatalogEntry("error", "Something went wrong", "Something unexpected happened. Try again, and your library and queue will stay as they are.", listOf("retry", "dismiss"), false, 0, emptyList(), false)
    }

/** Map an HTTP status to a code; [loaded] distinguishes a mid-session 401. */
fun classifyHttp(status: Int, loaded: Boolean = false): ErrorCode = when {
    status == 401 -> if (loaded) ErrorCode.UnauthorizedMidSession else ErrorCode.Unauthorized
    status == 404 -> ErrorCode.TrackNotFound
    status == 416 -> ErrorCode.BadRange
    status >= 500 -> ErrorCode.ServerError
    else -> ErrorCode.Unknown
}

/** Map a thrown exception (java.io / org.json) to a code. Never reads the token. */
fun classifyThrowable(e: Throwable, playing: Boolean = false): ErrorCode = when (e) {
    is java.net.SocketTimeoutException -> ErrorCode.RequestTimeout
    is java.net.UnknownHostException, is java.net.ConnectException, is java.io.IOException ->
        if (playing) ErrorCode.ConnectionLostDuringPlayback else ErrorCode.ServerUnreachable
    is org.json.JSONException -> ErrorCode.MalformedResponse
    else -> ErrorCode.Unknown
}

/** Map a Media3 PlaybackException error code to a catalog code. */
fun classifyPlayer(errorCode: Int): ErrorCode = when (errorCode) {
    androidx.media3.common.PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_FAILED,
    androidx.media3.common.PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_TIMEOUT -> ErrorCode.ConnectionLostDuringPlayback
    androidx.media3.common.PlaybackException.ERROR_CODE_IO_FILE_NOT_FOUND -> ErrorCode.TrackNotFound
    androidx.media3.common.PlaybackException.ERROR_CODE_DECODING_FAILED,
    androidx.media3.common.PlaybackException.ERROR_CODE_PARSING_CONTAINER_MALFORMED -> ErrorCode.AudioDecodeFailed
    else -> ErrorCode.Unknown
}
