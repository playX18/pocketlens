package com.acamera.android.media

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.hardware.camera2.CameraCaptureSession
import android.hardware.camera2.CameraCharacteristics
import android.hardware.camera2.CameraDevice
import android.hardware.camera2.CameraManager
import android.hardware.camera2.CaptureRequest
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaCodec
import android.media.MediaCodec.BufferInfo
import android.media.MediaFormat
import android.media.MediaRecorder
import android.os.Handler
import android.os.HandlerThread
import android.view.Surface
import androidx.core.content.ContextCompat
import com.acamera.android.state.CameraFacing
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

interface EncodedVideoSink {
    suspend fun onH264NalUnit(nalUnit: ByteArray, presentationTimeUs: Long)
}

interface EncodedAudioSink {
    suspend fun onOpusFrame(opusFrame: ByteArray, presentationTimeUs: Long)
}

class AndroidCamera2VideoCaptureController(
    private val context: Context,
    private val encodedSink: EncodedVideoSink,
) : VideoCaptureController {
    private val cameraManager: CameraManager by lazy {
        context.getSystemService(Context.CAMERA_SERVICE) as CameraManager
    }
    private var codec: MediaCodec? = null
    private var inputSurface: Surface? = null
    private var cameraThread: HandlerThread? = null
    private var cameraHandler: Handler? = null
    private var cameraDevice: CameraDevice? = null
    private var captureSession: CameraCaptureSession? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var drainJob: Job? = null
    private var currentFacing: CameraFacing? = null
    private var currentSettings: VideoEncoderSettings? = null
    @Volatile
    private var paused: Boolean = false
    @Volatile
    private var running: Boolean = false

    @SuppressLint("MissingPermission")
    override suspend fun start(facing: CameraFacing, settings: VideoEncoderSettings) {
        requireCameraPermission()
        withContext(Dispatchers.IO) {
            stopInternal()
            val cameraId = findCameraId(facing)
            require(cameraId != null) { "No ${facing.name.lowercase()} camera is available" }
            val encoder = MediaCodec.createEncoderByType(settings.mimeType).apply {
                configure(videoFormat(settings), null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            }
            val surface = encoder.createInputSurface()
            encoder.start()

            val thread = HandlerThread("ACameraVideoCapture").also { it.start() }
            val handler = Handler(thread.looper)
            val device = openCamera(cameraId, handler)
            val session = createSession(device, surface, handler)
            val request = device.createCaptureRequest(CameraDevice.TEMPLATE_RECORD).apply {
                addTarget(surface)
                set(CaptureRequest.CONTROL_MODE, CaptureRequest.CONTROL_MODE_AUTO)
            }.build()
            session.setRepeatingRequest(request, null, handler)

            codec = encoder
            inputSurface = surface
            cameraThread = thread
            cameraHandler = handler
            cameraDevice = device
            captureSession = session
            running = true
            drainJob = scope.launch {
                drainVideoEncoder(encoder)
            }
            currentFacing = facing
            currentSettings = settings
            paused = false
        }
    }

    override suspend fun flip(to: CameraFacing) {
        val settings = currentSettings ?: return
        start(to, settings)
    }

    override suspend fun pause(paused: Boolean) {
        this.paused = paused
    }

    override suspend fun stop() {
        withContext(Dispatchers.IO) {
            stopInternal()
        }
    }

    fun encodedSink(): EncodedVideoSink = encodedSink

    fun isPaused(): Boolean = paused

    private suspend fun stopInternal() {
        running = false
        drainJob?.cancelAndJoin()
        drainJob = null
        captureSession?.run {
            runCatching { stopRepeating() }
            runCatching { abortCaptures() }
            close()
        }
        captureSession = null
        cameraDevice?.close()
        cameraDevice = null
        inputSurface?.release()
        inputSurface = null
        codec?.stopAndRelease()
        codec = null
        cameraThread?.quitSafely()
        cameraThread = null
        cameraHandler = null
        currentFacing = null
        currentSettings = null
        paused = false
    }

    @SuppressLint("MissingPermission")
    private suspend fun openCamera(cameraId: String, handler: Handler): CameraDevice =
        suspendCancellableCoroutine { continuation ->
            cameraManager.openCamera(
                cameraId,
                object : CameraDevice.StateCallback() {
                    override fun onOpened(camera: CameraDevice) {
                        continuation.resume(camera)
                    }

                    override fun onDisconnected(camera: CameraDevice) {
                        camera.close()
                        if (continuation.isActive) {
                            continuation.resumeWithException(IllegalStateException("Camera disconnected"))
                        }
                    }

                    override fun onError(camera: CameraDevice, error: Int) {
                        camera.close()
                        if (continuation.isActive) {
                            continuation.resumeWithException(IllegalStateException("Camera open failed: $error"))
                        }
                    }
                },
                handler,
            )
            continuation.invokeOnCancellation {
                runCatching { cameraDevice?.close() }
            }
        }

    @Suppress("DEPRECATION")
    private suspend fun createSession(
        camera: CameraDevice,
        surface: Surface,
        handler: Handler,
    ): CameraCaptureSession =
        suspendCancellableCoroutine { continuation ->
            camera.createCaptureSession(
                listOf(surface),
                object : CameraCaptureSession.StateCallback() {
                    override fun onConfigured(session: CameraCaptureSession) {
                        continuation.resume(session)
                    }

                    override fun onConfigureFailed(session: CameraCaptureSession) {
                        session.close()
                        continuation.resumeWithException(IllegalStateException("Camera capture session configuration failed"))
                    }
                },
                handler,
            )
            continuation.invokeOnCancellation {
                runCatching { captureSession?.close() }
            }
        }

    private suspend fun drainVideoEncoder(encoder: MediaCodec) {
        val info = BufferInfo()
        while (running && scope.isActive) {
            val index = try {
                encoder.dequeueOutputBuffer(info, 10_000)
            } catch (_: IllegalStateException) {
                break
            }
            when {
                index >= 0 -> {
                    try {
                        if (
                            info.size > 0 &&
                            !paused &&
                            (info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG) == 0
                        ) {
                            val output = encoder.getOutputBuffer(index)
                            val accessUnit = output?.copyBytes(info.offset, info.size)
                            if (accessUnit != null) {
                                for (nalUnit in H264AccessUnitSplitter.nalUnits(accessUnit)) {
                                    encodedSink.onH264NalUnit(nalUnit, info.presentationTimeUs)
                                }
                            }
                        }
                    } finally {
                        runCatching { encoder.releaseOutputBuffer(index, false) }
                    }
                    if ((info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM) != 0) break
                }
                index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> Unit
                index == MediaCodec.INFO_TRY_AGAIN_LATER -> Unit
            }
        }
    }

    private fun findCameraId(facing: CameraFacing): String? {
        val expectedLens = when (facing) {
            CameraFacing.FRONT -> CameraCharacteristics.LENS_FACING_FRONT
            CameraFacing.BACK -> CameraCharacteristics.LENS_FACING_BACK
        }
        return cameraManager.cameraIdList.firstOrNull { id ->
            cameraManager.getCameraCharacteristics(id)
                .get(CameraCharacteristics.LENS_FACING) == expectedLens
        }
    }

    private fun requireCameraPermission() {
        require(
            ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED,
        ) { "Camera permission is required before starting video capture" }
    }

    private fun videoFormat(settings: VideoEncoderSettings): MediaFormat =
        MediaFormat.createVideoFormat(settings.mimeType, settings.width, settings.height).apply {
            setInteger(MediaFormat.KEY_BIT_RATE, settings.bitrateKbps * 1_000)
            setInteger(MediaFormat.KEY_FRAME_RATE, settings.fps)
            setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 1)
            setInteger(
                MediaFormat.KEY_COLOR_FORMAT,
                android.media.MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface,
            )
            setInteger(MediaFormat.KEY_PREPEND_HEADER_TO_SYNC_FRAMES, 1)
        }
}

class AndroidAudioRecordMicrophoneCaptureController(
    private val context: Context,
    private val encodedSink: EncodedAudioSink,
) : MicrophoneCaptureController {
    private var audioRecord: AudioRecord? = null
    private var codec: MediaCodec? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var captureJob: Job? = null
    @Volatile
    private var muted: Boolean = false
    @Volatile
    private var running: Boolean = false
    private var currentSettings: AudioEncoderSettings? = null

    override suspend fun start(settings: AudioEncoderSettings) {
        requireMicrophonePermission()
        withContext(Dispatchers.IO) {
            stopInternal()
            val channelMask = if (settings.channelCount == 1) {
                AudioFormat.CHANNEL_IN_MONO
            } else {
                AudioFormat.CHANNEL_IN_STEREO
            }
            val minBuffer = AudioRecord.getMinBufferSize(
                settings.sampleRateHz,
                channelMask,
                AudioFormat.ENCODING_PCM_16BIT,
            )
            require(minBuffer > 0) { "AudioRecord does not support requested settings: $settings" }

            val recorder = AudioRecord.Builder()
                .setAudioSource(MediaRecorder.AudioSource.MIC)
                .setAudioFormat(
                    AudioFormat.Builder()
                        .setSampleRate(settings.sampleRateHz)
                        .setChannelMask(channelMask)
                        .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                        .build(),
                )
                .setBufferSizeInBytes(minBuffer * 2)
                .build()

            val encoder = MediaCodec.createEncoderByType(settings.mimeType).apply {
                configure(audioFormat(settings), null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
                start()
            }

            audioRecord = recorder
            codec = encoder
            currentSettings = settings
            running = true
            recorder.startRecording()
            captureJob = scope.launch {
                captureAndDrainAudio(recorder, encoder, minBuffer)
            }
            muted = false
        }
    }

    override suspend fun mute(muted: Boolean) {
        this.muted = muted
    }

    override suspend fun stop() {
        withContext(Dispatchers.IO) {
            stopInternal()
        }
    }

    fun encodedSink(): EncodedAudioSink = encodedSink

    fun isMuted(): Boolean = muted

    fun currentSettings(): AudioEncoderSettings? = currentSettings

    private suspend fun stopInternal() {
        running = false
        captureJob?.cancelAndJoin()
        captureJob = null
        audioRecord?.run {
            runCatching { stop() }
            release()
        }
        audioRecord = null
        codec?.stopAndRelease()
        codec = null
        currentSettings = null
        muted = false
    }

    private suspend fun captureAndDrainAudio(
        recorder: AudioRecord,
        encoder: MediaCodec,
        minBufferSize: Int,
    ) {
        val pcm = ByteArray(minBufferSize)
        val info = BufferInfo()
        var sawInputError = false
        while (running && scope.isActive) {
            val read = if (muted) {
                recorder.read(pcm, 0, pcm.size)
                0
            } else {
                recorder.read(pcm, 0, pcm.size)
            }
            if (read < 0) {
                sawInputError = true
            } else if (read > 0) {
                queueAudioInput(encoder, pcm, read)
            }
            drainAudioEncoder(encoder, info)
            if (sawInputError) break
        }
    }

    private fun queueAudioInput(encoder: MediaCodec, pcm: ByteArray, read: Int) {
        val index = try {
            encoder.dequeueInputBuffer(10_000)
        } catch (_: IllegalStateException) {
            return
        }
        if (index < 0) return
        val input = encoder.getInputBuffer(index) ?: return
        input.clear()
        input.put(pcm, 0, read)
        encoder.queueInputBuffer(index, 0, read, System.nanoTime() / 1_000L, 0)
    }

    private suspend fun drainAudioEncoder(encoder: MediaCodec, info: BufferInfo) {
        while (true) {
            val index = try {
                encoder.dequeueOutputBuffer(info, 0)
            } catch (_: IllegalStateException) {
                break
            }
            when {
                index >= 0 -> {
                    try {
                        if (info.size > 0 && !muted && (info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG) == 0) {
                            val output = encoder.getOutputBuffer(index)
                            val frame = output?.copyBytes(info.offset, info.size)
                            if (frame != null) {
                                encodedSink.onOpusFrame(frame, info.presentationTimeUs)
                            }
                        }
                    } finally {
                        runCatching { encoder.releaseOutputBuffer(index, false) }
                    }
                    if ((info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM) != 0) break
                }
                index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> Unit
                index == MediaCodec.INFO_TRY_AGAIN_LATER -> break
            }
        }
    }

    private fun requireMicrophonePermission() {
        require(
            ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED,
        ) { "Microphone permission is required before starting audio capture" }
    }

    private fun audioFormat(settings: AudioEncoderSettings): MediaFormat =
        MediaFormat.createAudioFormat(settings.mimeType, settings.sampleRateHz, settings.channelCount).apply {
            setInteger(MediaFormat.KEY_BIT_RATE, settings.bitrateKbps * 1_000)
        }
}

internal object H264AccessUnitSplitter {
    fun nalUnits(accessUnit: ByteArray): List<ByteArray> {
        if (accessUnit.isEmpty()) return emptyList()
        val annexB = annexB(accessUnit)
        return if (annexB.isNotEmpty()) annexB else lengthPrefixed(accessUnit)
    }

    private fun annexB(accessUnit: ByteArray): List<ByteArray> {
        val starts = mutableListOf<Pair<Int, Int>>()
        var index = 0
        while (index < accessUnit.size - 3) {
            val prefixLength = when {
                accessUnit[index] == 0.toByte() &&
                    accessUnit[index + 1] == 0.toByte() &&
                    accessUnit[index + 2] == 1.toByte() -> 3
                index < accessUnit.size - 4 &&
                    accessUnit[index] == 0.toByte() &&
                    accessUnit[index + 1] == 0.toByte() &&
                    accessUnit[index + 2] == 0.toByte() &&
                    accessUnit[index + 3] == 1.toByte() -> 4
                else -> 0
            }
            if (prefixLength > 0) {
                starts += index to prefixLength
                index += prefixLength
            } else {
                index += 1
            }
        }
        if (starts.isEmpty()) return emptyList()

        return starts.mapIndexedNotNull { startIndex, (prefixStart, prefixLength) ->
            val nalStart = prefixStart + prefixLength
            val nalEnd = starts.getOrNull(startIndex + 1)?.first ?: accessUnit.size
            accessUnit.copyRangeOrNull(nalStart, nalEnd)
        }.filter(ByteArray::isH264NalPayload)
    }

    private fun lengthPrefixed(accessUnit: ByteArray): List<ByteArray> {
        val output = mutableListOf<ByteArray>()
        var index = 0
        while (index + 4 <= accessUnit.size) {
            val length = ((accessUnit[index].toInt() and 0xff) shl 24) or
                ((accessUnit[index + 1].toInt() and 0xff) shl 16) or
                ((accessUnit[index + 2].toInt() and 0xff) shl 8) or
                (accessUnit[index + 3].toInt() and 0xff)
            index += 4
            if (length <= 0 || index + length > accessUnit.size) {
                return if (output.isEmpty()) listOf(accessUnit) else output
            }
            accessUnit.copyOfRange(index, index + length)
                .takeIf(ByteArray::isH264NalPayload)
                ?.let(output::add)
            index += length
        }
        return if (output.isEmpty() && accessUnit.isH264NalPayload()) listOf(accessUnit) else output
    }
}

private fun ByteArray.isH264NalPayload(): Boolean =
    isNotEmpty() && (this[0].toInt() and 0x1f) in 1..23

private fun java.nio.ByteBuffer.copyBytes(offset: Int, size: Int): ByteArray {
    val duplicate = duplicate()
    duplicate.position(offset)
    duplicate.limit(offset + size)
    return ByteArray(size).also { duplicate.get(it) }
}

private fun ByteArray.copyRangeOrNull(start: Int, end: Int): ByteArray? =
    if (start < end && start in indices && end <= size) copyOfRange(start, end) else null

private fun MediaCodec.stopAndRelease() {
    runCatching { stop() }
    release()
}
