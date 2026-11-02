package com.example.musicshowcase

import org.json.JSONObject

/**
 * One track as returned by `GET /api/tracks` (see docs/api.md).
 *
 * [coverUrl]/[streamUrl] are stored exactly as returned by the server
 * (server-relative paths). Use [coverAbsolute]/[streamAbsolute] with the
 * configured base URL before handing them to a player or image loader.
 * [ApiClient] already returns tracks with both URLs made absolute.
 */
data class Track(
    val id: String,
    val title: String,
    val artist: String,
    val durationSec: Double,
    val sampleRate: Int,
    val bitDepth: Int,
    val bytes: Long,
    val coverUrl: String,
    val streamUrl: String,
) {
    fun coverAbsolute(baseUrl: String): String = resolve(baseUrl, coverUrl)
    fun streamAbsolute(baseUrl: String): String = resolve(baseUrl, streamUrl)

    companion object {
        /**
         * The single place track JSON is parsed (docs/api.md field names only).
         * Missing/blank cover or stream URLs fall back to the canonical
         * relative routes for the track id.
         */
        fun fromJson(obj: JSONObject): Track {
            val id = obj.optString("id")
            val stream = obj.optString("streamUrl")
            val cover = obj.optString("coverUrl")
            return Track(
                id = id,
                title = obj.optString("title").ifBlank { "Untitled" },
                artist = obj.optString("artist").ifBlank { "Unknown Artist" },
                durationSec = obj.optDouble("durationSec", 0.0),
                sampleRate = obj.optInt("sampleRate", 0),
                bitDepth = obj.optInt("bitDepth", 0),
                bytes = obj.optLong("bytes", 0L),
                coverUrl = cover.ifBlank { "/api/tracks/$id/cover" },
                streamUrl = stream.ifBlank { "/api/tracks/$id/stream" },
            )
        }

        fun resolve(baseUrl: String, path: String): String {
            if (path.startsWith("http://") || path.startsWith("https://")) return path
            val base = baseUrl.trimEnd('/')
            return if (path.startsWith("/")) base + path else "$base/$path"
        }
    }
}

/** "m:ss" for a duration in seconds. */
fun formatDuration(durationSec: Double): String {
    val total = durationSec.toLong()
    return "%d:%02d".format(total / 60, total % 60)
}
