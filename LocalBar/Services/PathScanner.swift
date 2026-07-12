import Foundation

/// Detects executable paths for supported server runtimes.
///
/// Runs `which` via Process off the main actor (Process blocks), falling
/// back to well-known install locations when `which` finds nothing.
actor PathScanner {

    // MARK: Public API

    /// Detect the best executable for mlx-lm.
    ///
    /// Priority order:
    ///   1. `uvx`  — runs mlx-lm via uv without a manual install (recommended)
    ///   2. `python3` — must have `mlx_lm` installed in its environment
    ///
    /// Returns the full path of the found executable, or nil.
    static func detectMLXExecutable() async -> String? {
        // Prefer uvx — zero-install path for the user.
        if let uvx = await detect(tool: "uvx", fallbacks: commonUvxPaths) {
            return uvx
        }
        // Fall back to a Python that might have mlx_lm installed.
        return await detect(tool: "python3", fallbacks: commonPythonPaths)
    }

    /// Run `which ollama` via Process, fall back to common paths.
    static func detectOllama() async -> String? {
        await detect(tool: "ollama", fallbacks: commonOllamaPaths)
    }

    // MARK: Known locations

    private static let commonUvxPaths = [
        "/usr/local/bin/uvx",
        "/opt/homebrew/bin/uvx",
        (NSHomeDirectory() as NSString).appendingPathComponent(".local/bin/uvx"),
    ]
    private static let commonPythonPaths = [
        "/opt/homebrew/bin/python3",
        "/usr/local/bin/python3",
        "/usr/bin/python3",
    ]
    private static let commonOllamaPaths = [
        "/opt/homebrew/bin/ollama",
        "/usr/local/bin/ollama",
    ]

    // MARK: Detection

    private static func detect(tool: String, fallbacks: [String]) async -> String? {
        if let found = await which(tool), FileManager.default.isExecutableFile(atPath: found) {
            return found
        }
        return fallbacks.first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    /// Runs `/usr/bin/env which <tool>` and returns the trimmed first line
    /// of stdout, or nil on any failure. Executed on a background queue
    /// because Process.waitUntilExit blocks the calling thread.
    private static func which(_ tool: String) async -> String? {
        await withCheckedContinuation { (continuation: CheckedContinuation<String?, Never>) in
            DispatchQueue.global(qos: .userInitiated).async {
                let process = Process()
                process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
                process.arguments = ["which", tool]

                let stdout = Pipe()
                process.standardOutput = stdout
                process.standardError = Pipe()

                do {
                    try process.run()
                } catch {
                    continuation.resume(returning: nil)
                    return
                }

                let data = stdout.fileHandleForReading.readDataToEndOfFile()
                process.waitUntilExit()

                guard process.terminationStatus == 0,
                      let output = String(data: data, encoding: .utf8) else {
                    continuation.resume(returning: nil)
                    return
                }

                // `which` may print multiple lines; take the first non-empty one.
                let path = output
                    .split(separator: "\n")
                    .map { $0.trimmingCharacters(in: .whitespaces) }
                    .first { !$0.isEmpty }

                guard let path, FileManager.default.fileExists(atPath: path) else {
                    continuation.resume(returning: nil)
                    return
                }
                continuation.resume(returning: path)
            }
        }
    }
}
