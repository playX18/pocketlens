package com.pocketlens.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.pocketlens.android.R
import com.pocketlens.android.app.PocketLensUiState
import com.pocketlens.android.app.PocketLensViewModel
import com.pocketlens.android.discovery.ReceiverAdvertisement
import com.pocketlens.android.protocol.QualityPreset
import com.pocketlens.android.state.SessionStatus
import com.pocketlens.android.state.UiStep
import com.pocketlens.android.ui.theme.PocketLensTheme

@Composable
fun PocketLensApp(viewModel: PocketLensViewModel = viewModel()) {
    val state by viewModel.uiState.collectAsStateWithLifecycle()
    KeepScreenAwake(
        state.session.status in setOf(
            SessionStatus.STARTING,
            SessionStatus.ACTIVE,
            SessionStatus.RECONNECTING,
        ),
    )
    PocketLensTheme {
        Surface(modifier = Modifier.fillMaxSize()) {
            PocketLensScreen(
                state = state,
                onHostChanged = viewModel::setManualHost,
                onPortChanged = viewModel::setManualPort,
                onReceiverConnect = viewModel::connectReceiver,
                onRefresh = viewModel::refreshDiscovery,
                onManualConnect = viewModel::showManualConnect,
                onPair = viewModel::pair,
                onCancelPairing = viewModel::cancelPairing,
                onForgetReceiver = viewModel::forgetReceiver,
                onStart = viewModel::startSession,
                onStop = viewModel::stopSession,
                onMute = viewModel::toggleMute,
                onPauseVideo = viewModel::toggleVideoPaused,
                onFlipCamera = viewModel::flipCamera,
                onPresetSelected = viewModel::selectPreset,
            )
        }
    }
}

@Composable
private fun KeepScreenAwake(enabled: Boolean) {
    val view = LocalView.current
    DisposableEffect(view, enabled) {
        val previous = view.keepScreenOn
        view.keepScreenOn = enabled
        onDispose {
            view.keepScreenOn = previous
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PocketLensScreen(
    state: PocketLensUiState,
    onHostChanged: (String) -> Unit,
    onPortChanged: (String) -> Unit,
    onReceiverConnect: (Int) -> Unit,
    onRefresh: () -> Unit,
    onManualConnect: () -> Unit,
    onPair: () -> Unit,
    onCancelPairing: () -> Unit,
    onForgetReceiver: () -> Unit,
    onStart: () -> Unit,
    onStop: () -> Unit,
    onMute: () -> Unit,
    onPauseVideo: () -> Unit,
    onFlipCamera: () -> Unit,
    onPresetSelected: (QualityPreset) -> Unit,
) {
    val step = state.currentStep()
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        text = when (step) {
                            UiStep.FindPc -> stringResource(R.string.step_find_pc)
                            UiStep.Pairing -> stringResource(R.string.step_pairing)
                            UiStep.Stream -> stringResource(R.string.step_stream)
                        },
                    )
                },
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 22.dp, vertical = 16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            StatusArea(state)
            when (step) {
                UiStep.FindPc -> FindPcStep(
                    state = state,
                    onReceiverConnect = onReceiverConnect,
                    onRefresh = onRefresh,
                    onManualConnect = onManualConnect,
                    onHostChanged = onHostChanged,
                    onPortChanged = onPortChanged,
                    onPair = onPair,
                )
                UiStep.Pairing -> PairingStep(
                    state = state,
                    onCancelPairing = onCancelPairing,
                )
                UiStep.Stream -> StreamStep(
                    state = state,
                    onStart = onStart,
                    onStop = onStop,
                    onMute = onMute,
                    onPauseVideo = onPauseVideo,
                    onFlipCamera = onFlipCamera,
                    onPresetSelected = onPresetSelected,
                    onForgetReceiver = onForgetReceiver,
                )
            }
        }
    }
}

@Composable
private fun StatusArea(state: PocketLensUiState) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(text = state.statusMessage, style = MaterialTheme.typography.bodyLarge)
        state.errorMessage?.let {
            Text(
                text = it,
                color = MaterialTheme.colorScheme.error,
                style = MaterialTheme.typography.bodyMedium,
            )
        }
    }
}

@Composable
private fun FindPcStep(
    state: PocketLensUiState,
    onReceiverConnect: (Int) -> Unit,
    onRefresh: () -> Unit,
    onManualConnect: () -> Unit,
    onHostChanged: (String) -> Unit,
    onPortChanged: (String) -> Unit,
    onPair: () -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(text = stringResource(R.string.pcs_nearby), style = MaterialTheme.typography.titleMedium)
            OutlinedButton(onClick = onRefresh) {
                Text(stringResource(R.string.refresh))
            }
        }

        if (state.discoveredReceivers.isEmpty() && state.discoveryRefreshing) {
            CircularProgressIndicator(strokeWidth = 2.dp)
        } else {
            state.discoveredReceivers.forEachIndexed { index, receiver ->
                ReceiverRow(receiver = receiver, onConnect = { onReceiverConnect(index) })
            }
        }

        if (state.manualConnectSuggested && !state.manualConnectVisible) {
            TextButton(onClick = onManualConnect, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.enter_address_manually))
            }
        } else if (!state.manualConnectVisible) {
            OutlinedButton(modifier = Modifier.fillMaxWidth(), onClick = onManualConnect) {
                Text(stringResource(R.string.enter_address_manually))
            }
        }

        if (state.manualConnectVisible) {
            ManualConnectForm(
                state = state,
                onHostChanged = onHostChanged,
                onPortChanged = onPortChanged,
                onPair = onPair,
            )
        }
    }
}

@Composable
private fun ReceiverRow(receiver: ReceiverAdvertisement, onConnect: () -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                text = receiver.receiverName,
                style = MaterialTheme.typography.titleSmall,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = "${receiver.host ?: "resolving"}:${receiver.controlPort}",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Button(onClick = onConnect, enabled = receiver.host != null) {
            Text(stringResource(R.string.connect))
        }
    }
}

@Composable
private fun ManualConnectForm(
    state: PocketLensUiState,
    onHostChanged: (String) -> Unit,
    onPortChanged: (String) -> Unit,
    onPair: () -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(text = stringResource(R.string.manual_connect), style = MaterialTheme.typography.titleMedium)
        OutlinedTextField(
            modifier = Modifier.fillMaxWidth(),
            value = state.manualHost,
            onValueChange = onHostChanged,
            label = { Text(stringResource(R.string.pc_address)) },
            placeholder = { Text(stringResource(R.string.pc_address_hint)) },
            singleLine = true,
        )
        OutlinedTextField(
            modifier = Modifier.fillMaxWidth(),
            value = state.manualPort,
            onValueChange = onPortChanged,
            label = { Text(stringResource(R.string.port)) },
            singleLine = true,
        )
        Button(modifier = Modifier.fillMaxWidth(), enabled = state.canPair, onClick = onPair) {
            Text(
                if (state.pairing.inFlight) {
                    stringResource(R.string.waiting_for_pc)
                } else {
                    stringResource(R.string.connect)
                },
            )
        }
    }
}

@Composable
private fun PairingStep(
    state: PocketLensUiState,
    onCancelPairing: () -> Unit,
) {
    val receiverName = state.pairing.selectedReceiver?.receiverName.orEmpty()
    Column(
        modifier = Modifier.fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Spacer(modifier = Modifier.height(24.dp))
        if (receiverName.isNotBlank()) {
            Text(
                text = receiverName,
                style = MaterialTheme.typography.titleLarge,
                textAlign = TextAlign.Center,
            )
        }
        Text(
            text = stringResource(R.string.enter_on_pc),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        if (state.pin.isNotBlank()) {
            Text(
                text = state.pin,
                style = MaterialTheme.typography.displayMedium,
                fontWeight = FontWeight.Bold,
                textAlign = TextAlign.Center,
            )
        } else {
            CircularProgressIndicator(strokeWidth = 2.dp)
        }
        OutlinedButton(onClick = onCancelPairing) {
            Text(stringResource(R.string.cancel))
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun StreamStep(
    state: PocketLensUiState,
    onStart: () -> Unit,
    onStop: () -> Unit,
    onMute: () -> Unit,
    onPauseVideo: () -> Unit,
    onFlipCamera: () -> Unit,
    onPresetSelected: (QualityPreset) -> Unit,
    onForgetReceiver: () -> Unit,
) {
    val receiverName = state.pairing.selectedReceiver?.receiverName
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        receiverName?.let {
            Text(text = it, style = MaterialTheme.typography.titleMedium)
        }
        state.session.warning?.let {
            Text(text = it, color = MaterialTheme.colorScheme.tertiary)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(enabled = state.canStart, onClick = onStart) {
                Text(stringResource(R.string.start))
            }
            OutlinedButton(enabled = state.canStop, onClick = onStop) {
                Text(stringResource(R.string.stop))
            }
        }
        if (state.session.status == SessionStatus.ACTIVE) {
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                OutlinedButton(onClick = onMute) {
                    Text(
                        if (state.controls.microphoneMuted) {
                            stringResource(R.string.unmute)
                        } else {
                            stringResource(R.string.mute)
                        },
                    )
                }
                OutlinedButton(onClick = onPauseVideo) {
                    Text(
                        if (state.controls.videoPaused) {
                            stringResource(R.string.resume_video)
                        } else {
                            stringResource(R.string.pause_video)
                        },
                    )
                }
                OutlinedButton(onClick = onFlipCamera) {
                    Text(stringResource(R.string.flip))
                }
            }
        }
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            QualityPreset.entries.forEach { preset ->
                FilterChip(
                    selected = state.controls.preset == preset,
                    onClick = { onPresetSelected(preset) },
                    label = { Text(qualityPresetLabel(preset)) },
                )
            }
        }
        TextButton(onClick = onForgetReceiver) {
            Text(stringResource(R.string.change_pc))
        }
        Spacer(modifier = Modifier.height(8.dp))
    }
}

@Composable
private fun qualityPresetLabel(preset: QualityPreset): String = when (preset) {
    QualityPreset.LOW -> stringResource(R.string.quality_save_data)
    QualityPreset.BALANCED -> stringResource(R.string.quality_balanced)
    QualityPreset.HIGH -> stringResource(R.string.quality_best)
}
