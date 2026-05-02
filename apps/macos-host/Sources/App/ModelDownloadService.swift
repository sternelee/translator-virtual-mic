import Foundation
import Combine

struct DownloadProgress: Equatable {
    let modelId: String
    let fileName: String
    let downloadedBytes: Int64
    let totalBytes: Int64
    let fileIndex: Int
    let totalFiles: Int
}

enum DownloadState: Equatable {
    case idle
    case downloading(progress: DownloadProgress)
    case completed
    case failed(String)
}

/// Downloads STT model files from HuggingFace using URLSession.
final class ModelDownloadService: NSObject, ObservableObject {
    @Published var state: DownloadState = .idle
    @Published var mtState: DownloadState = .idle
    @Published var ttsState: DownloadState = .idle

    private var currentTask: URLSessionDownloadTask?
    private var currentModel: SttModel?
    private var currentFileIndex: Int = 0

    private var currentMtTask: URLSessionDownloadTask?
    private var currentMtModel: MtModelInfo?
    private var currentMtFileIndex: Int = 0

    private var currentTtsTask: URLSessionDownloadTask?
    private var currentTtsModel: TtsModelInfo?
    private var currentTtsFileIndex: Int = 0
    private var modelsDir: URL {
        FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)
            .first!
            .appendingPathComponent("translator-virtual-mic/models")
    }

    func isModelDownloaded(_ model: SttModel) -> Bool {
        let dir = modelsDir.appendingPathComponent(model.id)
        return model.files.allSatisfy { file in
            let path = dir.appendingPathComponent(file.relativePath)
            return FileManager.default.fileExists(atPath: path.path)
                && ((try? FileManager.default.attributesOfItem(atPath: path.path)[.size] as? Int64) ?? 0) > 0
        }
    }

    func deleteModel(_ model: SttModel) {
        let dir = modelsDir.appendingPathComponent(model.id)
        try? FileManager.default.removeItem(at: dir)
        if currentModel?.id == model.id {
            currentTask?.cancel()
            state = .idle
        }
    }

    func startDownload(_ model: SttModel) {
        guard currentTask == nil || currentTask?.state != .running else { return }
        currentModel = model
        currentFileIndex = 0
        state = .idle
        downloadNextFile()
    }

    func cancel() {
        currentTask?.cancel()
        currentTask = nil
        currentModel = nil
        state = .idle
    }

    // MARK: - MT Model Downloads

    func isMtModelDownloaded(_ model: MtModelInfo) -> Bool {
        let dir = modelsDir.appendingPathComponent(model.id)
        return model.files.allSatisfy { file in
            let path = dir.appendingPathComponent(file.relativePath)
            return FileManager.default.fileExists(atPath: path.path)
                && ((try? FileManager.default.attributesOfItem(atPath: path.path)[.size] as? Int64) ?? 0) > 0
        }
    }

    func deleteMtModel(_ model: MtModelInfo) {
        let dir = modelsDir.appendingPathComponent(model.id)
        try? FileManager.default.removeItem(at: dir)
        if currentMtModel?.id == model.id {
            currentMtTask?.cancel()
            mtState = .idle
        }
    }

    func startMtDownload(_ model: MtModelInfo) {
        guard currentMtTask == nil || currentMtTask?.state != .running else { return }
        currentMtModel = model
        currentMtFileIndex = 0
        mtState = .idle
        downloadNextMtFile()
    }

    func cancelMtDownload() {
        currentMtTask?.cancel()
        currentMtTask = nil
        currentMtModel = nil
        mtState = .idle
    }

    // MARK: - TTS Model Downloads

    func isTtsModelDownloaded(_ model: TtsModelInfo) -> Bool {
        let dir = modelsDir.appendingPathComponent("tts").appendingPathComponent(model.id)
        return model.files.allSatisfy { file in
            let path = dir.appendingPathComponent(file.relativePath)
            return FileManager.default.fileExists(atPath: path.path)
                && ((try? FileManager.default.attributesOfItem(atPath: path.path)[.size] as? Int64) ?? 0) > 0
        }
    }

    func deleteTtsModel(_ model: TtsModelInfo) {
        let dir = modelsDir.appendingPathComponent("tts").appendingPathComponent(model.id)
        try? FileManager.default.removeItem(at: dir)
        if currentTtsModel?.id == model.id {
            currentTtsTask?.cancel()
            ttsState = .idle
        }
    }

    func startTtsDownload(_ model: TtsModelInfo) {
        guard currentTtsTask == nil || currentTtsTask?.state != .running else { return }
        currentTtsModel = model
        currentTtsFileIndex = 0
        ttsState = .idle
        downloadNextTtsFile()
    }

    func cancelTtsDownload() {
        currentTtsTask?.cancel()
        currentTtsTask = nil
        currentTtsModel = nil
        ttsState = .idle
    }

    private func downloadNextTtsFile() {
        guard let model = currentTtsModel, currentTtsFileIndex < model.files.count else {
            ttsState = .completed
            currentTtsTask = nil
            currentTtsModel = nil
            return
        }

        let file = model.files[currentTtsFileIndex]
        let dir = modelsDir
            .appendingPathComponent("tts")
            .appendingPathComponent(model.id)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let dest = dir.appendingPathComponent(file.relativePath)

        if FileManager.default.fileExists(atPath: dest.path),
           let attrs = try? FileManager.default.attributesOfItem(atPath: dest.path),
           let size = attrs[.size] as? Int64, size > 0 {
            currentTtsFileIndex += 1
            downloadNextTtsFile()
            return
        }

        guard let url = URL(string: file.url) else {
            ttsState = .failed("Invalid URL for \(file.relativePath)")
            return
        }

        let session = URLSession(configuration: .default, delegate: self, delegateQueue: .main)
        let task = session.downloadTask(with: url)
        currentTtsTask = task
        ttsState = .downloading(progress: DownloadProgress(
            modelId: model.id,
            fileName: file.relativePath,
            downloadedBytes: 0,
            totalBytes: file.sizeBytes,
            fileIndex: currentTtsFileIndex,
            totalFiles: model.files.count
        ))
        task.resume()
    }

    private func downloadNextMtFile() {
        guard let model = currentMtModel, currentMtFileIndex < model.files.count else {
            mtState = .completed
            currentMtTask = nil
            currentMtModel = nil
            return
        }

        let file = model.files[currentMtFileIndex]
        let dir = modelsDir.appendingPathComponent(model.id)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let dest = dir.appendingPathComponent(file.relativePath)

        if FileManager.default.fileExists(atPath: dest.path),
           let attrs = try? FileManager.default.attributesOfItem(atPath: dest.path),
           let size = attrs[.size] as? Int64, size > 0 {
            currentMtFileIndex += 1
            downloadNextMtFile()
            return
        }

        guard let url = URL(string: file.url) else {
            mtState = .failed("Invalid URL for \(file.relativePath)")
            return
        }

        let session = URLSession(configuration: .default, delegate: self, delegateQueue: .main)
        let task = session.downloadTask(with: url)
        currentMtTask = task
        mtState = .downloading(progress: DownloadProgress(
            modelId: model.id,
            fileName: file.relativePath,
            downloadedBytes: 0,
            totalBytes: file.sizeBytes,
            fileIndex: currentMtFileIndex,
            totalFiles: model.files.count
        ))
        task.resume()
    }

    private func downloadNextFile() {
        guard let model = currentModel, currentFileIndex < model.files.count else {
            state = .completed
            currentTask = nil
            currentModel = nil
            return
        }

        let file = model.files[currentFileIndex]
        let dir = modelsDir.appendingPathComponent(model.id)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let dest = dir.appendingPathComponent(file.relativePath)

        if FileManager.default.fileExists(atPath: dest.path),
           let attrs = try? FileManager.default.attributesOfItem(atPath: dest.path),
           let size = attrs[.size] as? Int64, size > 0 {
            currentFileIndex += 1
            downloadNextFile()
            return
        }

        guard let url = URL(string: file.url) else {
            state = .failed("Invalid URL for \(file.relativePath)")
            return
        }

        let session = URLSession(configuration: .default, delegate: self, delegateQueue: .main)
        let task = session.downloadTask(with: url)
        currentTask = task
        state = .downloading(progress: DownloadProgress(
            modelId: model.id,
            fileName: file.relativePath,
            downloadedBytes: 0,
            totalBytes: file.sizeBytes,
            fileIndex: currentFileIndex,
            totalFiles: model.files.count
        ))
        task.resume()
    }

    private func moveDownloadedFile(from tmp: URL, to dest: URL) {
        try? FileManager.default.removeItem(at: dest)
        try? FileManager.default.moveItem(at: tmp, to: dest)
    }
}

extension ModelDownloadService: URLSessionDownloadDelegate {
    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64, totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        if let model = currentModel, downloadTask == currentTask {
            let file = model.files[currentFileIndex]
            let total = totalBytesExpectedToWrite > 0 ? totalBytesExpectedToWrite : file.sizeBytes
            state = .downloading(progress: DownloadProgress(
                modelId: model.id,
                fileName: file.relativePath,
                downloadedBytes: totalBytesWritten,
                totalBytes: total,
                fileIndex: currentFileIndex,
                totalFiles: model.files.count
            ))
        } else if let model = currentMtModel, downloadTask == currentMtTask {
            let file = model.files[currentMtFileIndex]
            let total = totalBytesExpectedToWrite > 0 ? totalBytesExpectedToWrite : file.sizeBytes
            mtState = .downloading(progress: DownloadProgress(
                modelId: model.id,
                fileName: file.relativePath,
                downloadedBytes: totalBytesWritten,
                totalBytes: total,
                fileIndex: currentMtFileIndex,
                totalFiles: model.files.count
            ))
        } else if let model = currentTtsModel, downloadTask == currentTtsTask {
            let file = model.files[currentTtsFileIndex]
            let total = totalBytesExpectedToWrite > 0 ? totalBytesExpectedToWrite : file.sizeBytes
            ttsState = .downloading(progress: DownloadProgress(
                modelId: model.id,
                fileName: file.relativePath,
                downloadedBytes: totalBytesWritten,
                totalBytes: total,
                fileIndex: currentTtsFileIndex,
                totalFiles: model.files.count
            ))
        }
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        if let model = currentModel, downloadTask == currentTask {
            let file = model.files[currentFileIndex]
            let dest = modelsDir
                .appendingPathComponent(model.id)
                .appendingPathComponent(file.relativePath)
            moveDownloadedFile(from: location, to: dest)
            currentFileIndex += 1
            downloadNextFile()
        } else if let model = currentMtModel, downloadTask == currentMtTask {
            let file = model.files[currentMtFileIndex]
            let dest = modelsDir
                .appendingPathComponent(model.id)
                .appendingPathComponent(file.relativePath)
            moveDownloadedFile(from: location, to: dest)
            currentMtFileIndex += 1
            downloadNextMtFile()
        } else if let model = currentTtsModel, downloadTask == currentTtsTask {
            let file = model.files[currentTtsFileIndex]
            let dest = modelsDir
                .appendingPathComponent("tts")
                .appendingPathComponent(model.id)
                .appendingPathComponent(file.relativePath)
            moveDownloadedFile(from: location, to: dest)
            currentTtsFileIndex += 1
            downloadNextTtsFile()
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        if let error = error as NSError?, error.code == NSURLErrorCancelled {
            return
        }
        if let error {
            if task == currentTask {
                state = .failed(error.localizedDescription)
                currentTask = nil
                currentModel = nil
            } else if task == currentMtTask {
                mtState = .failed(error.localizedDescription)
                currentMtTask = nil
                currentMtModel = nil
            } else if task == currentTtsTask {
                ttsState = .failed(error.localizedDescription)
                currentTtsTask = nil
                currentTtsModel = nil
            }
        }
    }
}
