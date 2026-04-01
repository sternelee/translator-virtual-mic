import CoreAudio
import Foundation

struct CLIOptions {
    var uid = "translator.virtual.mic.device"
    var expectedName: String? = "Translator Virtual Mic"
    var strict = true
    var expectedInputStreams = 1
    var expectedOutputStreams = 0
    var expectedInputChannels = 1
    var expectedOutputChannels = 0
    var expectedSampleRate = 48_000.0
    var expectedTransportType = kAudioDeviceTransportTypeVirtual
    var allowMissing = false
    var json = false
    var list = false
}

struct DeviceReport: Encodable {
    let objectID: UInt32
    let uid: String
    let name: String
    let manufacturer: String?
    let inputStreamCount: Int
    let outputStreamCount: Int
    let inputChannelCount: Int
    let outputChannelCount: Int
    let nominalSampleRate: Double
    let alive: Bool
    let isRunning: Bool
    let isRunningSomewhere: Bool
    let hogMode: Int32
    let transportType: UInt32
}

enum SmokeError: Error, CustomStringConvertible {
    case usage(String)
    case audio(OSStatus, String)
    case notFound(String)
    case mismatch(String)

    var description: String {
        switch self {
        case .usage(let message):
            return message
        case .audio(let status, let context):
            return "\(context) failed with OSStatus \(status)"
        case .notFound(let message):
            return message
        case .mismatch(let message):
            return message
        }
    }
}

func parseArguments() throws -> CLIOptions {
    var options = CLIOptions()
    var iterator = CommandLine.arguments.dropFirst().makeIterator()

    while let arg = iterator.next() {
        switch arg {
        case "--uid":
            guard let value = iterator.next() else { throw SmokeError.usage("missing value for --uid") }
            options.uid = value
        case "--name":
            guard let value = iterator.next() else { throw SmokeError.usage("missing value for --name") }
            options.expectedName = value
        case "--no-name-check":
            options.expectedName = nil
        case "--no-strict":
            options.strict = false
        case "--input-streams":
            guard let value = iterator.next(), let parsed = Int(value) else { throw SmokeError.usage("invalid value for --input-streams") }
            options.expectedInputStreams = parsed
        case "--output-streams":
            guard let value = iterator.next(), let parsed = Int(value) else { throw SmokeError.usage("invalid value for --output-streams") }
            options.expectedOutputStreams = parsed
        case "--input-channels":
            guard let value = iterator.next(), let parsed = Int(value) else { throw SmokeError.usage("invalid value for --input-channels") }
            options.expectedInputChannels = parsed
        case "--output-channels":
            guard let value = iterator.next(), let parsed = Int(value) else { throw SmokeError.usage("invalid value for --output-channels") }
            options.expectedOutputChannels = parsed
        case "--sample-rate":
            guard let value = iterator.next(), let parsed = Double(value) else { throw SmokeError.usage("invalid value for --sample-rate") }
            options.expectedSampleRate = parsed
        case "--transport-type":
            guard let value = iterator.next(), let parsed = UInt32(value) else { throw SmokeError.usage("invalid value for --transport-type") }
            options.expectedTransportType = parsed
        case "--allow-missing":
            options.allowMissing = true
        case "--json":
            options.json = true
        case "--list":
            options.list = true
        case "--help", "-h":
            throw SmokeError.usage("usage: hal-smoke-verifier [--uid <uid>] [--name <expected name> | --no-name-check] [--allow-missing] [--no-strict] [--input-streams <n>] [--output-streams <n>] [--input-channels <n>] [--output-channels <n>] [--sample-rate <hz>] [--transport-type <value>] [--json] [--list]")
        default:
            throw SmokeError.usage("unknown argument: \(arg)")
        }
    }

    return options
}

func getPropertyDataSize(objectID: AudioObjectID, address: inout AudioObjectPropertyAddress) throws -> UInt32 {
    var size: UInt32 = 0
    let status = AudioObjectGetPropertyDataSize(objectID, &address, 0, nil, &size)
    guard status == noErr else {
        throw SmokeError.audio(status, "AudioObjectGetPropertyDataSize selector=\(address.mSelector)")
    }
    return size
}

func getScalarProperty<T>(objectID: AudioObjectID, selector: AudioObjectPropertySelector, scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal, element: AudioObjectPropertyElement = kAudioObjectPropertyElementMain, as type: T.Type = T.self) throws -> T {
    var address = AudioObjectPropertyAddress(mSelector: selector, mScope: scope, mElement: element)
    var size = UInt32(MemoryLayout<T>.stride)
    let rawBuffer = UnsafeMutableRawPointer.allocate(byteCount: Int(size), alignment: MemoryLayout<T>.alignment)
    defer { rawBuffer.deallocate() }
    let status = AudioObjectGetPropertyData(objectID, &address, 0, nil, &size, rawBuffer)
    guard status == noErr else {
        throw SmokeError.audio(status, "AudioObjectGetPropertyData selector=\(selector)")
    }
    return rawBuffer.load(as: T.self)
}

func getStringProperty(objectID: AudioObjectID, selector: AudioObjectPropertySelector, scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal) throws -> String {
    var address = AudioObjectPropertyAddress(mSelector: selector, mScope: scope, mElement: kAudioObjectPropertyElementMain)
    var value: Unmanaged<CFString>?
    var size = UInt32(MemoryLayout<Unmanaged<CFString>?>.stride)
    let status = withUnsafeMutablePointer(to: &value) { pointer in
        AudioObjectGetPropertyData(objectID, &address, 0, nil, &size, pointer)
    }
    guard status == noErr else {
        throw SmokeError.audio(status, "AudioObjectGetPropertyData string selector=\(selector)")
    }
    guard let cfValue = value?.takeRetainedValue() else {
        throw SmokeError.mismatch("selector=\(selector) returned nil string")
    }
    return cfValue as String
}

func getObjectIDArrayProperty(objectID: AudioObjectID, selector: AudioObjectPropertySelector, scope: AudioObjectPropertyScope) throws -> [AudioObjectID] {
    var address = AudioObjectPropertyAddress(mSelector: selector, mScope: scope, mElement: kAudioObjectPropertyElementMain)
    let size = try getPropertyDataSize(objectID: objectID, address: &address)
    if size == 0 {
        return []
    }
    let count = Int(size) / MemoryLayout<AudioObjectID>.stride
    var values = Array(repeating: AudioObjectID(0), count: count)
    var mutableSize = size
    let status = values.withUnsafeMutableBufferPointer { buffer in
        AudioObjectGetPropertyData(objectID, &address, 0, nil, &mutableSize, buffer.baseAddress!)
    }
    guard status == noErr else {
        throw SmokeError.audio(status, "AudioObjectGetPropertyData object array selector=\(selector)")
    }
    return values
}

func getChannelCount(objectID: AudioObjectID, scope: AudioObjectPropertyScope) throws -> Int {
    var address = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyStreamConfiguration, mScope: scope, mElement: kAudioObjectPropertyElementMain)
    let size = try getPropertyDataSize(objectID: objectID, address: &address)
    if size == 0 {
        return 0
    }

    let rawBuffer = UnsafeMutableRawPointer.allocate(byteCount: Int(size), alignment: MemoryLayout<AudioBufferList>.alignment)
    defer { rawBuffer.deallocate() }

    var mutableSize = size
    let status = AudioObjectGetPropertyData(objectID, &address, 0, nil, &mutableSize, rawBuffer)
    guard status == noErr else {
        throw SmokeError.audio(status, "AudioObjectGetPropertyData stream configuration")
    }

    let bufferList = rawBuffer.assumingMemoryBound(to: AudioBufferList.self)
    let audioBufferList = UnsafeMutableAudioBufferListPointer(bufferList)
    return audioBufferList.reduce(0) { $0 + Int($1.mNumberChannels) }
}

func allDevices() throws -> [AudioObjectID] {
    try getObjectIDArrayProperty(objectID: AudioObjectID(kAudioObjectSystemObject), selector: kAudioHardwarePropertyDevices, scope: kAudioObjectPropertyScopeGlobal)
}

func makeDeviceReport(objectID: AudioObjectID) throws -> DeviceReport {
    let uid = try getStringProperty(objectID: objectID, selector: kAudioDevicePropertyDeviceUID)
    let name = try getStringProperty(objectID: objectID, selector: kAudioObjectPropertyName)
    let manufacturer = try? getStringProperty(objectID: objectID, selector: kAudioObjectPropertyManufacturer)
    let inputStreams = try getObjectIDArrayProperty(objectID: objectID, selector: kAudioDevicePropertyStreams, scope: kAudioObjectPropertyScopeInput)
    let outputStreams = try getObjectIDArrayProperty(objectID: objectID, selector: kAudioDevicePropertyStreams, scope: kAudioObjectPropertyScopeOutput)
    let inputChannelCount = try getChannelCount(objectID: objectID, scope: kAudioObjectPropertyScopeInput)
    let outputChannelCount = try getChannelCount(objectID: objectID, scope: kAudioObjectPropertyScopeOutput)
    let sampleRate: Float64 = try getScalarProperty(objectID: objectID, selector: kAudioDevicePropertyNominalSampleRate)
    let alive: UInt32 = try getScalarProperty(objectID: objectID, selector: kAudioDevicePropertyDeviceIsAlive)
    let running: UInt32 = try getScalarProperty(objectID: objectID, selector: kAudioDevicePropertyDeviceIsRunning)
    let runningSomewhere: UInt32 = try getScalarProperty(objectID: objectID, selector: kAudioDevicePropertyDeviceIsRunningSomewhere)
    let hogMode: pid_t = try getScalarProperty(objectID: objectID, selector: kAudioDevicePropertyHogMode)
    let transportType: UInt32 = try getScalarProperty(objectID: objectID, selector: kAudioDevicePropertyTransportType)

    return DeviceReport(
        objectID: objectID,
        uid: uid,
        name: name,
        manufacturer: manufacturer,
        inputStreamCount: inputStreams.count,
        outputStreamCount: outputStreams.count,
        inputChannelCount: inputChannelCount,
        outputChannelCount: outputChannelCount,
        nominalSampleRate: sampleRate,
        alive: alive != 0,
        isRunning: running != 0,
        isRunningSomewhere: runningSomewhere != 0,
        hogMode: hogMode,
        transportType: transportType
    )
}

func printHumanReadable(_ report: DeviceReport) {
    let manufacturer = report.manufacturer ?? ""
    print("object_id=\(report.objectID)")
    print("uid=\(report.uid)")
    print("name=\(report.name)")
    print("manufacturer=\(manufacturer)")
    print("input_stream_count=\(report.inputStreamCount)")
    print("output_stream_count=\(report.outputStreamCount)")
    print("input_channel_count=\(report.inputChannelCount)")
    print("output_channel_count=\(report.outputChannelCount)")
    print("nominal_sample_rate=\(report.nominalSampleRate)")
    print("alive=\(report.alive)")
    print("is_running=\(report.isRunning)")
    print("is_running_somewhere=\(report.isRunningSomewhere)")
    print("hog_mode=\(report.hogMode)")
    print("transport_type=\(report.transportType)")
}

func validateInstalledDevice(_ report: DeviceReport, options: CLIOptions) throws {
    if !report.alive {
        throw SmokeError.mismatch("device is not alive")
    }
    if report.inputStreamCount != options.expectedInputStreams {
        throw SmokeError.mismatch("input stream count mismatch: expected \(options.expectedInputStreams) got \(report.inputStreamCount)")
    }
    if report.outputStreamCount != options.expectedOutputStreams {
        throw SmokeError.mismatch("output stream count mismatch: expected \(options.expectedOutputStreams) got \(report.outputStreamCount)")
    }
    if report.inputChannelCount != options.expectedInputChannels {
        throw SmokeError.mismatch("input channel count mismatch: expected \(options.expectedInputChannels) got \(report.inputChannelCount)")
    }
    if report.outputChannelCount != options.expectedOutputChannels {
        throw SmokeError.mismatch("output channel count mismatch: expected \(options.expectedOutputChannels) got \(report.outputChannelCount)")
    }
    if abs(report.nominalSampleRate - options.expectedSampleRate) > 0.5 {
        throw SmokeError.mismatch("sample rate mismatch: expected \(options.expectedSampleRate) got \(report.nominalSampleRate)")
    }
    if report.transportType != options.expectedTransportType {
        throw SmokeError.mismatch("transport type mismatch: expected \(options.expectedTransportType) got \(report.transportType)")
    }
}

func run() throws {
    let options = try parseArguments()
    let devices = try allDevices().map(makeDeviceReport)

    if options.list {
        if options.json {
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            let data = try encoder.encode(devices)
            FileHandle.standardOutput.write(data)
            FileHandle.standardOutput.write("\n".data(using: .utf8)!)
        } else {
            print("device_count=\(devices.count)")
            for device in devices {
                print("uid=\(device.uid) name=\(device.name) input_streams=\(device.inputStreamCount) output_streams=\(device.outputStreamCount)")
            }
        }
        return
    }

    guard let report = devices.first(where: { $0.uid == options.uid }) else {
        if options.allowMissing {
            print("status=missing")
            print("uid=\(options.uid)")
            return
        }
        throw SmokeError.notFound("device not found for uid=\(options.uid)")
    }

    if let expectedName = options.expectedName, report.name != expectedName {
        throw SmokeError.mismatch("device name mismatch: expected '\(expectedName)' got '\(report.name)'")
    }
    if options.strict {
        try validateInstalledDevice(report, options: options)
    }

    if options.json {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(report)
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write("\n".data(using: .utf8)!)
    } else {
        print("status=ok")
        printHumanReadable(report)
    }
}

do {
    try run()
} catch {
    fputs("hal-smoke-verifier: \(error)\n", stderr)
    exit(1)
}
