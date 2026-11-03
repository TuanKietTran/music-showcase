package com.example.musicshowcase

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.SkipNext
import androidx.compose.material.icons.filled.SkipPrevious
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import coil.compose.AsyncImage
import coil.request.ImageRequest
import androidx.media3.common.Player
import kotlinx.coroutines.delay

@Composable
fun NowPlayingScreen(player: androidx.media3.common.Player, track: Track) {
    var positionMs by remember(player) { mutableLongStateOf(player.currentPosition.coerceAtLeast(0L)) }
    var durationMs by remember(player) { mutableLongStateOf(player.duration.coerceAtLeast(0L)) }
    var isPlaying by remember(player) { mutableStateOf(player.isPlaying) }
    var isBuffering by remember(player) { mutableStateOf(player.playbackState == Player.STATE_BUFFERING) }
    LaunchedEffect(player) {
        while (true) {
            positionMs = player.currentPosition.coerceAtLeast(0L)
            durationMs = player.duration.coerceAtLeast(0L)
            isPlaying = player.isPlaying
            isBuffering = player.playbackState == Player.STATE_BUFFERING
            delay(500)
        }
    }
    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center
    ) {
        AsyncImage(
            model = ImageRequest.Builder(LocalContext.current)
                .data(track.coverAbsolute(PlaybackConfig.baseUrl))
                .apply { PlaybackConfig.authorizationHeader()?.let { addHeader("Authorization", it) } }
                .build(),
            contentDescription = "Cover art for ${track.title}",
            contentScale = ContentScale.Crop,
            modifier = Modifier.size(200.dp)
        )
        Spacer(Modifier.height(16.dp))
        Text(track.title, style = MaterialTheme.typography.headlineMedium)
        Spacer(Modifier.height(8.dp))
        Text(track.artist, style = MaterialTheme.typography.bodyLarge)
        if (track.sampleRate > 0) {
            Spacer(Modifier.height(8.dp))
            Text("${track.sampleRate / 1000.0} kHz / ${track.bitDepth}-bit", style = MaterialTheme.typography.bodyMedium)
        }
        Spacer(Modifier.height(24.dp))
        if (isBuffering) {
            CircularProgressIndicator()
            Spacer(Modifier.height(16.dp))
        }
        Slider(
            value = if (durationMs > 0L) positionMs.toFloat() / durationMs.toFloat() else 0f,
            onValueChange = { fraction ->
                if (durationMs > 0L) {
                    val target = (fraction * durationMs).toLong()
                    positionMs = target
                    player.seekTo(target)
                }
            },
            modifier = Modifier.fillMaxWidth()
        )
        Row(horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = { player.seekToPrevious() }) {
                Icon(Icons.Filled.SkipPrevious, contentDescription = "Previous")
            }
            IconButton(onClick = {
                if (player.isPlaying) player.pause() else player.play()
                isPlaying = player.isPlaying
            }) {
                Icon(
                    if (isPlaying) Icons.Filled.Pause else Icons.Filled.PlayArrow,
                    contentDescription = if (isPlaying) "Pause" else "Play"
                )
            }
            IconButton(onClick = { player.seekToNext() }) {
                Icon(Icons.Filled.SkipNext, contentDescription = "Next")
            }
        }
    }
}
