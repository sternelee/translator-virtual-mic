import AVFoundation
import CoreAudio
import Foundation

struct SharedBufferMonitorSnapshot {
    let fileExists: Bool
    let sampleRate: UInt32
    let channelCount: UInt32
    let capacityFrames: UInt32
    let writeIndexFrames: UInt64
    let readIndexFrames: UInt64
    let lastTimestampNs: UInt64

    static let missing = SharedBufferMonitorSnapshot(
        fileExists: false,
        sampleRate: 0,
        channelCount: 0,
        capacityFrames: 0,
        writeIndexFrames: 0,
        readIndexFrames: 0,
        lastTimestampNs: 0
    )
}

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
    @Published var sharedBufferStatusText: String = "Shared buffer idle"

    private let captureService = MicrophoneCaptureService()
    private var engine: EngineBox?
    private var sharedBufferMonitorTask: Task<Void, Never>?
    private var lastSharedBufferSnapshot: SharedBufferMonitorSnapshot = .missing

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
        // 排除虚拟麦克风，选择真正的物理麦克风
        let physicalDevices = devices.filter { !$0.uid.contains("translator.virtual.mic") }

        // 如果当前选中的是虚拟麦克风，重新选择物理麦克风
        if let currentUID = selectedDeviceUID, currentUID.contains("translator.virtual.mic") {
            selectedDeviceID = nil
            selectedDeviceUID = nil
        }

        if selectedDeviceID == nil {
            if let firstPhysical = physicalDevices.first {
                selectedDeviceID = firstPhysical.id
                selectedDeviceUID = firstPhysical.uid
                appendLog("Auto-selected physical device: \(firstPhysical.name)")
            } else if let first = devices.first {
                selectedDeviceID = first.id
                selectedDeviceUID = first.uid
            }
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
        appendLog("Starting engine with device UID: \(selectedDeviceUID ?? "nil")")
        let engine = EngineBox(configJSON: "{\"target\":\"\(targetLanguage)\"}")
        guard engine.start() == 0 else {
            statusText = "Failed"
            appendLog("engine_start failed: \(engine.lastError())")
            return
        }

        _ = engine.setTargetLanguage(targetLanguage)
        let sharedResult = engine.enableSharedOutput(capacityFrames: 14_400, channels: 1, sampleRate: 48_000)
        appendLog("enableSharedOutput result: \(sharedResult)")
        if sharedResult != 0 {
            _ = engine.stop()
            statusText = "Failed"
            appendLog("enableSharedOutput failed: \(engine.lastError())")
            return
        }
        sharedOutputPath = engine.sharedOutputPath()
        appendLog("sharedOutputPath: \(sharedOutputPath)")
        if sharedOutputPath.isEmpty {
            _ = engine.stop()
            statusText = "Failed"
            appendLog("sharedOutputPath is empty after enabling shared output")
            return
        }
        refreshSharedBufferStatus(logOnChange: true)

        do {
            var chunkCount = 0
            try captureService.start(deviceUID: selectedDeviceUID) { [weak self] chunk in
                guard let self else { return }
                chunkCount += 1
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
                    // Log first few chunks for debugging
                    if chunkCount <= 3 {
                        self.appendLog("chunk#\(chunkCount): frames=\(chunk.frameCount) rms=\(String(format: "%.6f", chunk.rmsLevel)) samples=[\(chunk.samples.prefix(3).map { String(format: "%.4f", $0) }.joined(separator: ","))...]")
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
        startSharedBufferMonitor()
    }

    func stopEngine() {
        captureService.stop()
        stopSharedBufferMonitor()
        guard let engine else { return }
        _ = engine.stop()
        self.engine = nil
        statusText = "Idle"
        inputLevel = 0
        sharedOutputPath = ""
        sharedBufferStatusText = "Shared buffer idle"
        lastSharedBufferSnapshot = .missing
        appendLog("Engine stopped")
    }

    private func startSharedBufferMonitor() {
        stopSharedBufferMonitor()
        sharedBufferMonitorTask = Task { [weak self] in
            while !Task.isCancelled {
                await MainActor.run {
                    self?.refreshSharedBufferStatus(logOnChange: false)
                }
                try? await Task.sleep(for: .milliseconds(500))
            }
        }
    }

    private func stopSharedBufferMonitor() {
        sharedBufferMonitorTask?.cancel()
        sharedBufferMonitorTask = nil
    }

    private func refreshSharedBufferStatus(logOnChange: Bool) {
        let snapshot = Self.readSharedBufferSnapshot(path: sharedOutputPath)
        sharedBufferStatusText = Self.describeSharedBufferSnapshot(snapshot)

        guard snapshot.fileExists != lastSharedBufferSnapshot.fileExists ||
                snapshot.writeIndexFrames != lastSharedBufferSnapshot.writeIndexFrames ||
                snapshot.readIndexFrames != lastSharedBufferSnapshot.readIndexFrames ||
                snapshot.lastTimestampNs != lastSharedBufferSnapshot.lastTimestampNs
        else {
            return
        }

        if logOnChange || snapshot.fileExists || lastSharedBufferSnapshot.fileExists {
            appendLog("Shared buffer: \(sharedBufferStatusText)")
        }
        lastSharedBufferSnapshot = snapshot
    }

    private static func readSharedBufferSnapshot(path: String) -> SharedBufferMonitorSnapshot {
        guard !path.isEmpty else {
            return .missing
        }
        let url = URL(fileURLWithPath: path)
        guard let data = try? Data(contentsOf: url), data.count >= 48 else {
            return .missing
        }

        return SharedBufferMonitorSnapshot(
            fileExists: true,
            sampleRate: readUInt32LE(from: data, offset: 12),
            channelCount: readUInt32LE(from: data, offset: 8),
            capacityFrames: readUInt32LE(from: data, offset: 16),
            writeIndexFrames: readUInt64LE(from: data, offset: 24),
            readIndexFrames: readUInt64LE(from: data, offset: 32),
            lastTimestampNs: readUInt64LE(from: data, offset: 40)
        )
    }

    private static func describeSharedBufferSnapshot(_ snapshot: SharedBufferMonitorSnapshot) -> String {
        guard snapshot.fileExists else {
            return "Shared buffer missing"
        }

        let unreadFrames = snapshot.writeIndexFrames >= snapshot.readIndexFrames
            ? snapshot.writeIndexFrames - snapshot.readIndexFrames
            : 0
        return "Shared buffer sr=\(snapshot.sampleRate) ch=\(snapshot.channelCount) cap=\(snapshot.capacityFrames) write=\(snapshot.writeIndexFrames) read=\(snapshot.readIndexFrames) unread=\(unreadFrames) ts=\(snapshot.lastTimestampNs)"
    }

    private static func readUInt32LE(from data: Data, offset: Int) -> UInt32 {
        let range = offset..<(offset + 4)
        return data[range].withUnsafeBytes { rawBuffer in
            UInt32(littleEndian: rawBuffer.load(as: UInt32.self))
        }
    }

    private static func readUInt64LE(from data: Data, offset: Int) -> UInt64 {
        let range = offset..<(offset + 8)
        return data[range].withUnsafeBytes { rawBuffer in
            UInt64(littleEndian: rawBuffer.load(as: UInt64.self))
        }
    }

    private func appendLog(_ message: String) {
        fputs("[TranslatorVirtualMicHost] \(message)\n", stderr)
        logLines.append(message)
        if logLines.count > 200 {
            logLines.removeFirst(logLines.count - 200)
        }
    }
}
