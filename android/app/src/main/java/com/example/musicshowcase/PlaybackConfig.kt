package com.example.musicshowcase

/**
 * Runtime-only server settings, shared between the UI and [PlaybackService].
 * The token is kept in memory only and never written to disk
 * (docs/api.md: "Clients keep the token in memory only").
 */
object PlaybackConfig {
    const val DEFAULT_BASE_URL = "http://10.0.2.2:8080"

    @Volatile
    var baseUrl: String = DEFAULT_BASE_URL

    @Volatile
    var token: String = ""

    fun authorizationHeader(): String? =
        token.takeIf { it.isNotBlank() }?.let { "Bearer $it" }
}
