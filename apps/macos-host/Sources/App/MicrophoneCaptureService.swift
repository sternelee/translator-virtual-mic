import AVFoundation
import AudioToolbox
import CoreMedia
import Foundation

enum MicrophoneCaptureError: Error {
    case deviceNotFound(String)
    case inputCreationFailed
}

final class MicrophoneCaptureService: NSObject, AVCaptureAudioDataOutputSampleBufferDelegate {
    struct PCMChunk {
        let samples: [Float]
        let frameCount: Int
        let channels: Int
        let sampleRate: Int
        let timestampNs: UInt64
        let rmsLevel: Float
    }

    private let session = AVCaptureSession()
    private let captureQueue = DispatchQueue(label: "translator-virtual-mic.capture")
    private var audioOutput: AVCaptureAudioDataOutput?
    private var onChunk: ((PCMChunk) -> Void)?

    func start(deviceUID: String?, onChunk: @escaping (PCMChunk) -> Void) throws {
        stop()

        self.onChunk = onChunk
        let device = try resolveAudioDevice(deviceUID: deviceUID)
        let input = try AVCaptureDeviceInput(device: device)
        let output = AVCaptureAudioDataOutput()
        output.audioSettings = [
            AVFormatIDKey: kAudioFormatLinearPCM,
            AVLinearPCMIsFloatKey: true,
            AVLinearPCMBitDepthKey: 32,
            AVLinearPCMIsNonInterleaved: false,
        ]
        output.setSampleBufferDelegate(self, queue: captureQueue)

        session.beginConfiguration()
        session.inputs.forEach { session.removeInput($0) }
        session.outputs.forEach { session.removeOutput($0) }

        guard session.canAddInput(input) else {
            session.commitConfiguration()
            throw MicrophoneCaptureError.inputCreationFailed
        }
        guard session.canAddOutput(output) else {
            session.commitConfiguration()
            throw MicrophoneCaptureError.inputCreationFailed
        }

        session.addInput(input)
        session.addOutput(output)
        session.commitConfiguration()

        audioOutput = output
        session.startRunning()
    }

    func stop() {
        if session.isRunning {
            session.stopRunning()
        }
        onChunk = nil
        audioOutput = nil
    }

    func captureOutput(
        _ output: AVCaptureOutput,
        didOutput sampleBuffer: CMSampleBuffer,
        from connection: AVCaptureConnection
    ) {
        guard let onChunk,
              let chunk = Self.makePCMChunk(from: sampleBuffer) else {
            return
        }
        onChunk(chunk)
    }

    private func resolveAudioDevice(deviceUID: String?) throws -> AVCaptureDevice {
        let devices = AVCaptureDevice.DiscoverySession(
            deviceTypes: [.microphone],
            mediaType: .audio,
            position: .unspecified
        ).devices

        if let deviceUID,
           let matched = devices.first(where: { $0.uniqueID == deviceUID }) {
            return matched
        }

        if let fallback = AVCaptureDevice.default(for: .audio) ?? devices.first {
            return fallback
        }

        throw MicrophoneCaptureError.deviceNotFound(deviceUID ?? "default")
    }

    private static func makePCMChunk(from sampleBuffer: CMSampleBuffer) -> PCMChunk? {
        guard let formatDescription = CMSampleBufferGetFormatDescription(sampleBuffer),
              let asbdPointer = CMAudioFormatDescriptionGetStreamBasicDescription(formatDescription) else {
            return nil
        }

        let asbd = asbdPointer.pointee
        let channelCount = Int(asbd.mChannelsPerFrame)
        let sampleRate = Int(asbd.mSampleRate)
        let frameCount = CMSampleBufferGetNumSamples(sampleBuffer)
        guard frameCount > 0, channelCount > 0 else { return nil }

        var blockBuffer: CMBlockBuffer?
        var bufferListSize = 0
        CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sampleBuffer,
            bufferListSizeNeededOut: &bufferListSize,
            bufferListOut: nil,
            bufferListSize: 0,
            blockBufferAllocator: nil,
            blockBufferMemoryAllocator: nil,
            flags: UInt32(kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment),
            blockBufferOut: nil
        )

        let audioBufferListPointer = UnsafeMutableRawPointer.allocate(
            byteCount: bufferListSize,
            alignment: MemoryLayout<AudioBufferList>.alignment
        )
        defer { audioBufferListPointer.deallocate() }

        let audioBufferList = audioBufferListPointer.bindMemory(to: AudioBufferList.self, capacity: 1)
        let status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sampleBuffer,
            bufferListSizeNeededOut: nil,
            bufferListOut: audioBufferList,
            bufferListSize: bufferListSize,
            blockBufferAllocator: nil,
            blockBufferMemoryAllocator: nil,
            flags: UInt32(kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment),
            blockBufferOut: &blockBuffer
        )
        guard status == noErr else { return nil }

        let buffers = UnsafeMutableAudioBufferListPointer(audioBufferList)
        let interleavedSamples = extractSamples(
            from: buffers,
            frameCount: frameCount,
            channelCount: channelCount,
            asbd: asbd
        )
        let monoSamples = mixToMono(interleavedSamples, channelCount: channelCount)
        let rmsLevel = rms(samples: monoSamples)

        let timestamp = CMSampleBufferGetPresentationTimeStamp(sampleBuffer)
        let timestampNs = UInt64((CMTimeGetSeconds(timestamp) * 1_000_000_000).rounded())

        return PCMChunk(
            samples: monoSamples,
            frameCount: monoSamples.count,
            channels: 1,
            sampleRate: sampleRate,
            timestampNs: timestampNs,
            rmsLevel: rmsLevel
        )
    }

    private static func extractSamples(
        from buffers: UnsafeMutableAudioBufferListPointer,
        frameCount: Int,
        channelCount: Int,
        asbd: AudioStreamBasicDescription
    ) -> [Float] {
        let isFloat = (asbd.mFormatFlags & kAudioFormatFlagIsFloat) != 0
        let isSignedInteger = (asbd.mFormatFlags & kAudioFormatFlagIsSignedInteger) != 0
        let isNonInterleaved = (asbd.mFormatFlags & kAudioFormatFlagIsNonInterleaved) != 0
        let bitsPerChannel = Int(asbd.mBitsPerChannel)

        if isFloat && bitsPerChannel == 32 {
            return copyFloatSamples(
                from: buffers,
                frameCount: frameCount,
                channelCount: channelCount,
                nonInterleaved: isNonInterleaved
            )
        }

        if isSignedInteger && bitsPerChannel == 16 {
            return copyInt16Samples(
                from: buffers,
                frameCount: frameCount,
                channelCount: channelCount,
                nonInterleaved: isNonInterleaved
            )
        }

        return []
    }

    private static func copyFloatSamples(
        from buffers: UnsafeMutableAudioBufferListPointer,
        frameCount: Int,
        channelCount: Int,
        nonInterleaved: Bool
    ) -> [Float] {
        if nonInterleaved {
            var output = Array(repeating: Float.zero, count: frameCount * channelCount)
            for channel in 0..<min(channelCount, buffers.count) {
                guard let source = buffers[channel].mData?.assumingMemoryBound(to: Float.self) else { continue }
                for frame in 0..<frameCount {
                    output[frame * channelCount + channel] = source[frame]
                }
            }
            return output
        }

        guard let source = buffers.first?.mData?.assumingMemoryBound(to: Float.self) else {
            return []
        }
        let sampleCount = frameCount * channelCount
        return Array(UnsafeBufferPointer(start: source, count: sampleCount))
    }

    private static func copyInt16Samples(
        from buffers: UnsafeMutableAudioBufferListPointer,
        frameCount: Int,
        channelCount: Int,
        nonInterleaved: Bool
    ) -> [Float] {
        if nonInterleaved {
            var output = Array(repeating: Float.zero, count: frameCount * channelCount)
            for channel in 0..<min(channelCount, buffers.count) {
                guard let source = buffers[channel].mData?.assumingMemoryBound(to: Int16.self) else { continue }
                for frame in 0..<frameCount {
                    output[frame * channelCount + channel] = Float(source[frame]) / Float(Int16.max)
                }
            }
            return output
        }

        guard let source = buffers.first?.mData?.assumingMemoryBound(to: Int16.self) else {
            return []
        }
        let sampleCount = frameCount * channelCount
        return (0..<sampleCount).map { Float(source[$0]) / Float(Int16.max) }
    }

    private static func mixToMono(_ interleavedSamples: [Float], channelCount: Int) -> [Float] {
        guard channelCount > 1 else { return interleavedSamples }
        let frameCount = interleavedSamples.count / channelCount
        return (0..<frameCount).map { frame in
            let start = frame * channelCount
            let sum = interleavedSamples[start..<(start + channelCount)].reduce(Float.zero, +)
            return sum / Float(channelCount)
        }
    }

    private static func rms(samples: [Float]) -> Float {
        guard !samples.isEmpty else { return 0 }
        let meanSquare = samples.reduce(Float.zero) { partial, sample in
            partial + sample * sample
        } / Float(samples.count)
        return sqrt(meanSquare)
    }
}
