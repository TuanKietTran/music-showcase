package com.example.musicshowcase

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

// UNCOMPILED: written without an Android toolchain. Renders catalog text only.
@Composable
fun ErrorBanner(code: ErrorCode, onAction: (String) -> Unit, modifier: Modifier = Modifier) {
    val entry = code.entry
    if (entry.silent) return
    val label = when (entry.severity) { "info" -> "Notice"; "warning" -> "Warning"; "blocking" -> "Action needed"; else -> "Error" }
    Surface(
        color = MaterialTheme.colorScheme.surfaceVariant,
        shape = MaterialTheme.shapes.medium,
        modifier = modifier.fillMaxWidth().semantics {
            liveRegion = if (entry.assertive) LiveRegionMode.Assertive else LiveRegionMode.Polite
        },
    ) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("$label: ${entry.title}", style = MaterialTheme.typography.titleSmall) // text label, never colour-only
            Text(entry.message, style = MaterialTheme.typography.bodyMedium)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                entry.actions.filter { it != "enter-token" && it != "change-server" }.forEach { action ->
                    TextButton(onClick = { onAction(action) }) {
                        Text(when (action) { "retry" -> "Try again"; "skip-track" -> "Skip track"; else -> "Dismiss" })
                    }
                }
            }
        }
    }
}
