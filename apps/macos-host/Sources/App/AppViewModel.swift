import AVFoundation
import CoreAudio
import Foundation

@MainActor
final class AppViewModel: ObservableObject {
    @Published var devices: [AudioDevice] = []
    @Published var selectedDeviceID: AudioDeviceID?
    @Published var selectedDeviceUID: String?
    @Published var statusText: String = "Idle"
    @Published var logLines: [String] = []
    @Published var targetLanguage: String = "en"
    @Published var microphonePermissionGranted: Bool = false
    @Published var inputLevel: Float = 0
    @Published var metricsJSON: String = "{}"
    @Published var sharedOutputPath: String = ""

    private let captureService = MicrophoneCaptureService()
    private var engine: EngineBox?

    init() {
        refreshDevices()
        requestMicrophonePermission()
    }

    func requestMicrophonePermission() {
        AVCaptureDevice.requestAccess(for: .audio) { granted in
            Task { @MainActor in
                self.microphonePermissionGranted = granted
                self.appendLog(granted ? "Microphone permission granted" : "Microphone permission denied")
            }
        }
    }

    func refreshDevices() {
        devices = AudioDevice.enumerateInputDevices()
        if let first = devices.first, selectedDeviceID == nil {
            selectedDeviceID = first.id
            selectedDeviceUID = first.uid
        } else if let selectedDeviceID,
                  let device = devices.first(where: { $0.id == selectedDeviceID }) {
            selectedDeviceUID = device.uid
        }
        appendLog("Enumerated \(devices.count) input device(s)")
    }

    func selectDevice(_ device: AudioDevice) {
        selectedDeviceID = device.id
        selectedDeviceUID = device.uid
        appendLog("Selected input device: \(device.name)")
    }

    func startEngine() {
        stopEngine()
        let engine = EngineBox(configJSON: "{\"target\":\"\(targetLanguage)\"}")
        guard engine.start() == 0 else {
            statusText = "Failed"
            appendLog("engine_start failed: \(engine.lastError())")
            return
        }

        _ = engine.setTargetLanguage(targetLanguage)
        _ = engine.enableSharedOutput(capacityFrames: 14_400, channels: 1, sampleRate: 48_000)
        sharedOutputPath = engine.sharedOutputPath()

        do {
            try captureService.start(deviceUID: selectedDeviceUID) { [weak self] chunk in
                guard let self else { return }
                let result = engine.pushInputPCM(
                    samples: chunk.samples,
                    frameCount: Int32(chunk.frameCount),
                    channels: Int32(chunk.channels),
                    sampleRate: Int32(chunk.sampleRate),
                    timestampNs: chunk.timestampNs
                )

                Task { @MainActor in
                    self.inputLevel = chunk.rmsLevel
                    self.metricsJSON = engine.metricsJSON()
                    if result != 0 {
                        self.statusText = "Degraded"
                        self.appendLog("engine_push_input_pcm failed: \(engine.lastError())")
                    }
                }
            }
        } catch {
            _ = engine.stop()
            statusText = "Failed"
            appendLog("Microphone capture start failed: \(error)")
            return
        }

        self.engine = engine
        statusText = "Listening"
        metricsJSON = engine.metricsJSON()
        appendLog("Engine started")
        if !sharedOutputPath.isEmpty {
            appendLog("Shared output file: \(sharedOutputPath)")
        }
    }

    func stopEngine() {
        captureService.stop()
        guard let engine else { return }
        _ = engine.stop()
        self.engine = nil
        statusText = "Idle"
        inputLevel = 0
        sharedOutputPath = ""
        appendLog("Engine stopped")
    }

    private func appendLog(_ message: String) {
        logLines.append(message)
        if logLines.count > 200 {
            logLines.removeFirst(logLines.count - 200)
        }
    }
}
