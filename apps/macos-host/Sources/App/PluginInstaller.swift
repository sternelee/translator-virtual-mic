import Foundation

/// Manages installation of the TranslatorVirtualMic CoreAudio driver bundle.
enum PluginInstaller {
    static let halDir = "/Library/Audio/Plug-Ins/HAL"
    static let bundleName = "TranslatorVirtualMic.driver"
    static let installedPath = "\(halDir)/\(bundleName)"

    /// Returns true if the driver bundle exists in the system HAL directory.
    static func isInstalled() -> Bool {
        FileManager.default.fileExists(atPath: installedPath)
    }

    /// Returns the path to a usable plugin bundle, searching:
    /// 1. Inside the app bundle (Resources/ or MacOS/../)
    /// 2. Relative to the source repo (for dev builds)
    static func findBundlePath() -> String? {
        let fm = FileManager.default
        let candidates = bundleCandidates()
        for path in candidates {
            if fm.fileExists(atPath: path) {
                return path
            }
        }
        return nil
    }

    /// Installs the bundle to /Library/Audio/Plug-Ins/HAL using AppleScript
    /// for GUI privilege elevation, then reloads CoreAudio.
    static func install(completion: @escaping (Result<Void, Error>) -> Void) {
        guard let sourcePath = findBundlePath() else {
            completion(.failure(PluginInstallerError.bundleNotFound))
            return
        }

        let scriptSource = """
        do shell script "mkdir -p \(halDir) && rm -rf \(installedPath) && cp -R '\(sourcePath)' '\(installedPath)'" with administrator privileges
        """

        var errorInfo: NSDictionary?
        guard let appleScript = NSAppleScript(source: scriptSource) else {
            completion(.failure(PluginInstallerError.appleScriptCreationFailed))
            return
        }

        appleScript.executeAndReturnError(&errorInfo)

        if let errorInfo {
            let message = errorInfo["NSAppleScriptErrorMessage"] as? String ?? "unknown AppleScript error"
            completion(.failure(PluginInstallerError.appleScriptFailed(message)))
            return
        }

        reloadCoreAudio(completion: completion)
    }

    /// Uninstalls the driver bundle and reloads CoreAudio.
    static func uninstall(completion: @escaping (Result<Void, Error>) -> Void) {
        let scriptSource = """
        do shell script "rm -rf \(installedPath)" with administrator privileges
        """

        var errorInfo: NSDictionary?
        guard let appleScript = NSAppleScript(source: scriptSource) else {
            completion(.failure(PluginInstallerError.appleScriptCreationFailed))
            return
        }

        appleScript.executeAndReturnError(&errorInfo)

        if let errorInfo {
            let message = errorInfo["NSAppleScriptErrorMessage"] as? String ?? "unknown AppleScript error"
            completion(.failure(PluginInstallerError.appleScriptFailed(message)))
            return
        }

        reloadCoreAudio(completion: completion)
    }

    /// Restarts the CoreAudio daemon so the HAL picks up the new driver.
    static func reloadCoreAudio(completion: @escaping (Result<Void, Error>) -> Void) {
        let task = Process()
        task.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        task.arguments = ["kickstart", "-k", "system/com.apple.audio.coreaudiod"]

        let pipe = Pipe()
        task.standardOutput = pipe
        task.standardError = pipe

        do {
            try task.run()
            task.waitUntilExit()
            if task.terminationStatus == 0 {
                completion(.success(()))
            } else {
                let data = pipe.fileHandleForReading.readDataToEndOfFile()
                let message = String(data: data, encoding: .utf8) ?? "launchctl exit \(task.terminationStatus)"
                completion(.failure(PluginInstallerError.reloadFailed(message)))
            }
        } catch {
            completion(.failure(error))
        }
    }

    // MARK: - Private

    private static func bundleCandidates() -> [String] {
        var candidates: [String] = []
        let fm = FileManager.default

        // 1. Bundled inside the app (copy during package-app.sh or Xcode build phase)
        if let resourcesURL = Bundle.main.resourceURL {
            candidates.append(resourcesURL.appendingPathComponent(bundleName).path)
        }
        if let executableURL = Bundle.main.executableURL {
            let executableDir = executableURL.deletingLastPathComponent()
            // Next to executable
            candidates.append(executableDir.appendingPathComponent(bundleName).path)
            // Two levels up (MyApp.app/Contents/MacOS → MyApp.app/)
            candidates.append(executableDir.deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent(bundleName).path)
        }

        // 2. Repo-relative from source file location (dev builds)
        let sourceFileURL = URL(fileURLWithPath: #filePath)
        let repoRoot = sourceFileURL
            .deletingLastPathComponent()  // App
            .deletingLastPathComponent()  // Sources
            .deletingLastPathComponent()  // macos-host
            .deletingLastPathComponent()  // apps
            .deletingLastPathComponent()  // repo root
        let buildBundle = repoRoot
            .appendingPathComponent("native/macos/build")
            .appendingPathComponent(bundleName)
        candidates.append(buildBundle.path)

        // 3. Current working directory
        let cwd = fm.currentDirectoryPath
        candidates.append(URL(fileURLWithPath: cwd).appendingPathComponent("native/macos/build/\(bundleName)").path)

        return Array(NSOrderedSet(array: candidates)) as? [String] ?? candidates
    }
}

enum PluginInstallerError: Error, LocalizedError {
    case bundleNotFound
    case appleScriptCreationFailed
    case appleScriptFailed(String)
    case reloadFailed(String)

    var errorDescription: String? {
        switch self {
        case .bundleNotFound:
            return "TranslatorVirtualMic.driver bundle not found. Build it with ./native/macos/scripts/build-plugin-bundle.sh"
        case .appleScriptCreationFailed:
            return "Failed to create AppleScript for privilege elevation"
        case .appleScriptFailed(let message):
            return "Installation failed: \(message)"
        case .reloadFailed(let message):
            return "CoreAudio reload failed: \(message)"
        }
    }
}
