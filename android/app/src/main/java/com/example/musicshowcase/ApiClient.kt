package com.example.musicshowcase

import org.json.JSONArray
import org.json.JSONObject
import java.io.BufferedReader
import java.io.InputStreamReader
import java.net.HttpURLConnection
import java.net.URL

class ApiClient(
    private val baseUrl: String,
    private val token: String
) {
    fun fetchTracks(): List<Track> {
        val url = URL(baseUrl.trimEnd('/') + "/api/tracks")
        val conn = url.openConnection() as HttpURLConnection
        try {
            conn.requestMethod = "GET"
            conn.connectTimeout = 10_000
            conn.readTimeout = 15_000
            conn.setRequestProperty("Accept", "application/json")
            if (token.isNotBlank()) {
                conn.setRequestProperty("Authorization", "Bearer $token")
            }
            val code = conn.responseCode
            check(code in 200..299) { "GET /api/tracks failed: HTTP $code" }
            val body = BufferedReader(InputStreamReader(conn.inputStream, Charsets.UTF_8)).use { it.readText() }
            return parseTracks(body)
        } finally {
            conn.disconnect()
        }
    }

    /**
     * Parses a bare JSON array (per docs/api.md) or a {"tracks": [...]} envelope.
     * Parsing itself lives in [Track.fromJson]; this only makes each track's
     * cover/stream URLs absolute against [baseUrl] so UI and playback can
     * use them directly.
     */
    private fun parseTracks(body: String): List<Track> {
        val trimmed = body.trim()
        val array: JSONArray = if (trimmed.startsWith("[")) {
            JSONArray(trimmed)
        } else {
            JSONObject(trimmed).optJSONArray("tracks") ?: JSONArray()
        }
        val result = ArrayList<Track>(array.length())
        for (i in 0 until array.length()) {
            val track = Track.fromJson(array.getJSONObject(i))
            result.add(
                track.copy(
                    coverUrl = track.coverAbsolute(baseUrl),
                    streamUrl = track.streamAbsolute(baseUrl)
                )
            )
        }
        return result
    }
}
