import CoreAudio
import Foundation

struct AudioDevice: Identifiable, Hashable {
    let id: AudioDeviceID
    let uid: String
    let name: String

    static func enumerateInputDevices() -> [AudioDevice] {
        var propertyAddress = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDevices,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )

        var dataSize: UInt32 = 0
        let systemObjectID = AudioObjectID(kAudioObjectSystemObject)
        guard AudioObjectGetPropertyDataSize(systemObjectID, &propertyAddress, 0, nil, &dataSize) == noErr else {
            return []
        }

        let count = Int(dataSize) / MemoryLayout<AudioDeviceID>.size
        var ids = Array(repeating: AudioDeviceID(), count: count)
        guard AudioObjectGetPropertyData(systemObjectID, &propertyAddress, 0, nil, &dataSize, &ids) == noErr else {
            return []
        }

        return ids.compactMap { id in
            guard hasInputStream(deviceID: id) else { return nil }
            return AudioDevice(id: id, uid: deviceUID(deviceID: id), name: deviceName(deviceID: id))
        }
    }

    private static func hasInputStream(deviceID: AudioDeviceID) -> Bool {
        var propertyAddress = AudioObjectPropertyAddress(
            mSelector: kAudioDevicePropertyStreams,
            mScope: kAudioDevicePropertyScopeInput,
            mElement: kAudioObjectPropertyElementMain
        )
        var dataSize: UInt32 = 0
        return AudioObjectGetPropertyDataSize(deviceID, &propertyAddress, 0, nil, &dataSize) == noErr && dataSize > 0
    }

    private static func deviceName(deviceID: AudioDeviceID) -> String {
        stringProperty(deviceID: deviceID, selector: kAudioObjectPropertyName) ?? "Unknown Device"
    }

    private static func deviceUID(deviceID: AudioDeviceID) -> String {
        stringProperty(deviceID: deviceID, selector: kAudioDevicePropertyDeviceUID) ?? "\(deviceID)"
    }

    private static func stringProperty(deviceID: AudioDeviceID, selector: AudioObjectPropertySelector) -> String? {
        var propertyAddress = AudioObjectPropertyAddress(
            mSelector: selector,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var value: CFString = "" as CFString
        var dataSize = UInt32(MemoryLayout<CFString>.size)
        let status = AudioObjectGetPropertyData(deviceID, &propertyAddress, 0, nil, &dataSize, &value)
        guard status == noErr else { return nil }
        return value as String
    }
}
