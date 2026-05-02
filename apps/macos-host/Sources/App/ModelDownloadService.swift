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
        let modelOnnx = dir.appendingPathComponent("model.onnx")
        let espeak = dir.appendingPathComponent("espeak-ng-data")
        return FileManager.default.fileExists(atPath: modelOnnx.path)
            && FileManager.default.fileExists(atPath: espeak.path)
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
        downloadTtsArchive()
    }

    func cancelTtsDownload() {
        currentTtsTask?.cancel()
        currentTtsTask = nil
        currentTtsModel = nil
        ttsState = .idle
    }

    private func downloadTtsArchive() {
        guard let model = currentTtsModel else { return }

        // Skip if already extracted
        if isTtsModelDownloaded(model) {
            ttsState = .completed
            currentTtsTask = nil
            currentTtsModel = nil
            return
        }

        guard let url = URL(string: model.tarballUrl) else {
            ttsState = .failed("Invalid tarball URL for \(model.id)")
            return
        }

        let ttsDir = modelsDir.appendingPathComponent("tts")
        try? FileManager.default.createDirectory(at: ttsDir, withIntermediateDirectories: true)

        let session = URLSession(configuration: .default, delegate: self, delegateQueue: .main)
        let task = session.downloadTask(with: url)
        currentTtsTask = task
        let archiveName = url.lastPathComponent
        ttsState = .downloading(progress: DownloadProgress(
            modelId: model.id,
            fileName: archiveName,
            downloadedBytes: 0,
            totalBytes: model.tarballSizeBytes,
            fileIndex: 0,
            totalFiles: 1
        ))
        task.resume()
    }

    private func extractTtsArchive(from tmp: URL, model: TtsModelInfo) {
        let ttsDir = modelsDir.appendingPathComponent("tts")
        let archiveDest = ttsDir.appendingPathComponent("\(model.id).tar.bz2")
        try? FileManager.default.removeItem(at: archiveDest)
        do {
            try FileManager.default.moveItem(at: tmp, to: archiveDest)
        } catch {
            ttsState = .failed("Failed to stage archive: \(error.localizedDescription)")
            return
        }

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let proc = Process()
            proc.executableURL = URL(fileURLWithPath: "/usr/bin/tar")
            proc.arguments = ["xjf", archiveDest.path, "-C", ttsDir.path]
            do {
                try proc.run()
                proc.waitUntilExit()
            } catch {
                DispatchQueue.main.async {
                    self?.ttsState = .failed("tar extraction failed: \(error.localizedDescription)")
                }
                return
            }
            try? FileManager.default.removeItem(at: archiveDest)
            DispatchQueue.main.async {
                guard let self else { return }
                if self.isTtsModelDownloaded(model) {
                    self.ttsState = .completed
                } else {
                    self.ttsState = .failed("Extraction incomplete — espeak-ng-data missing")
                }
                self.currentTtsTask = nil
                self.currentTtsModel = nil
            }
        }
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
            let total = totalBytesExpectedToWrite > 0 ? totalBytesExpectedToWrite : model.tarballSizeBytes
            let archiveName = URL(string: model.tarballUrl)?.lastPathComponent ?? "\(model.id).tar.bz2"
            ttsState = .downloading(progress: DownloadProgress(
                modelId: model.id,
                fileName: archiveName,
                downloadedBytes: totalBytesWritten,
                totalBytes: total,
                fileIndex: 0,
                totalFiles: 1
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
            extractTtsArchive(from: location, model: model)
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
