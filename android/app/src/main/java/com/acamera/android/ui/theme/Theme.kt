package com.acamera.android.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val AccentGreen = Color(0xFF1F7A5C)

private val ACameraColorScheme = lightColorScheme(
    primary = AccentGreen,
    onPrimary = Color.White,
    primaryContainer = Color(0xFFD4EDE3),
    onPrimaryContainer = Color(0xFF0A3D2E),
)

@Composable
fun ACameraTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = ACameraColorScheme,
        content = content,
    )
}
