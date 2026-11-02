package com.example.musicshowcase

import android.content.ComponentName
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.media3.common.MediaItem
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                var serverUrl by remember { mutableStateOf(PlaybackConfig.DEFAULT_BASE_URL) }
                var token by remember { mutableStateOf("") }
                var tracks by remember { mutableStateOf<List<Track>>(emptyList()) }
                var selectedTrack by remember { mutableStateOf<Track?>(null) }
                var status by remember { mutableStateOf("") }
                var isLoading by remember { mutableStateOf(false) }
                // The only player is the one in PlaybackService; the UI talks to
                // it through this controller (no second, local ExoPlayer).
                var controller by remember { mutableStateOf<MediaController?>(null) }
                val context = LocalContext.current
                DisposableEffect(Unit) {
                    val sessionToken = SessionToken(context, ComponentName(context, PlaybackService::class.java))
                    val future = MediaController.Builder(context, sessionToken).buildAsync()
                    future.addListener(
                        { runCatching { controller = future.get() } },
                        ContextCompat.getMainExecutor(context)
                    )
                    onDispose {
                        controller = null
                        MediaController.releaseFuture(future)
                    }
                }
                Column(
                    modifier = Modifier.fillMaxSize().padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp)
                ) {
                    OutlinedTextField(
                        value = serverUrl,
                        onValueChange = { serverUrl = it },
                        label = { Text("Server URL") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth()
                    )
                    OutlinedTextField(
                        value = token,
                        onValueChange = { token = it },
                        label = { Text("Token") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth()
                    )
                    Button(
                        onClick = {
                            PlaybackConfig.baseUrl = serverUrl
                            PlaybackConfig.token = token
                            isLoading = true
                            status = "Loading..."
                            Thread {
                                runCatching { ApiClient(serverUrl, token).fetchTracks() }
                                    .onSuccess { loaded ->
                                        runOnUiThread {
                                            tracks = loaded
                                            isLoading = false
                                            status = ""
                                        }
                                    }
                                    .onFailure { e ->
                                        runOnUiThread {
                                            isLoading = false
                                            status = e.message ?: "Failed to load tracks"
                                        }
                                    }
                            }.start()
                        },
                        modifier = Modifier.fillMaxWidth()
                    ) { Text("Load") }
                    if (status.isNotEmpty()) Text(status)
                    if (tracks.isNotEmpty()) {
                        TrackListScreen(
                            tracks = tracks,
                            isLoading = isLoading,
                            currentTrackId = selectedTrack?.id,
                            onTrackClick = { track ->
                                selectedTrack = track
                                controller?.let { player ->
                                    val startIndex = tracks.indexOf(track).coerceAtLeast(0)
                                    val items = tracks.map { MediaItem.fromUri(it.streamAbsolute(serverUrl)) }
                                    player.setMediaItems(items, startIndex, 0L)
                                    player.prepare()
                                    player.play()
                                }
                            }
                        )
                    }
                    selectedTrack?.let { track ->
                        controller?.let { player -> NowPlayingScreen(player = player, track = track) }
                    }
                }
            }
        }
    }
}
