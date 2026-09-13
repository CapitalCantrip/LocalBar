import Foundation

/// Detects executable paths for supported server runtimes.
///
/// Uses the user's login shell for `which` so PATH entries from ~/.zshrc /
/// ~/.bash_profile are respected, then falls back to well-known install
/// locations when the shell search finds nothing.
actor PathScanner {

    // MARK: Public API

    /// Detect the best executable for mlx-lm.
    ///
    /// Priority order:
    ///   1. `uvx`     — runs mlx-lm via uv without a manual install (recommended)
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

    /// Run `which ollama` via the login shell, fall back to common paths.
    static func detectOllama() async -> String? {
        await detect(tool: "ollama", fallbacks: commonOllamaPaths)
    }

    // MARK: Known locations

    private static let home = NSHomeDirectory()

    private static let commonUvxPaths: [String] = {
        let h = NSHomeDirectory()
        return [
            "/usr/local/bin/uvx",
            "/opt/homebrew/bin/uvx",
            (h as NSString).appendingPathComponent(".local/bin/uvx"),
        ]
    }()

    private static let commonPythonPaths: [String] = {
        let h = NSHomeDirectory()
        return [
            // pipx-managed mlx-lm install
            (h as NSString).appendingPathComponent(".local/pipx/venvs/mlx-lm/bin/python"),
            // Homebrew Python
            "/opt/homebrew/bin/python3",
            "/usr/local/bin/python3",
            // Miniconda / Anaconda base env (most common single-env case)
            (h as NSString).appendingPathComponent("miniconda3/bin/python3"),
            (h as NSString).appendingPathComponent("anaconda3/bin/python3"),
            // System Python (last resort — almost certainly missing mlx-lm)
            "/usr/bin/python3",
        ]
    }()

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

    /// Runs `which <tool>` inside the user's login shell so that PATH
    /// additions from ~/.zshrc / ~/.bash_profile are visible.
    ///
    /// Falls back to `/usr/bin/env which` if the login shell can't be
    /// determined. Executed on a background queue because
    /// `Process.waitUntilExit` blocks the calling thread.
    static func which(_ tool: String) async -> String? {
        await withCheckedContinuation { (continuation: CheckedContinuation<String?, Never>) in
            DispatchQueue.global(qos: .userInitiated).async {
                // Determine the user's login shell (SHELL env var, or /bin/zsh fallback).
                let shell = ProcessInfo.processInfo.environment["SHELL"] ?? "/bin/zsh"
                let shellURL = URL(fileURLWithPath: shell)

                let process = Process()
                // -l  = login shell (sources ~/.zshrc / ~/.bash_profile)
                // -c  = run the command string
                process.executableURL = shellURL
                process.arguments = ["-l", "-c", "which \(tool)"]

                let stdout = Pipe()
                process.standardOutput = stdout
                process.standardError = Pipe()

                do {
                    try process.run()
                } catch {
                    // Shell unavailable — try env-which as a last resort.
                    continuation.resume(returning: Self.envWhich(tool))
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

    /// Synchronous `/usr/bin/env which` used as a fallback when the login
    /// shell is unavailable. Must be called from a background thread.
    static func envWhich(_ tool: String) -> String? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["which", tool]

        let stdout = Pipe()
        process.standardOutput = stdout
        process.standardError = Pipe()

        guard (try? process.run()) != nil else { return nil }
        let data = stdout.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()

        guard process.terminationStatus == 0,
              let output = String(data: data, encoding: .utf8) else { return nil }

        let path = output
            .split(separator: "\n")
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .first { !$0.isEmpty }

        guard let path, FileManager.default.fileExists(atPath: path) else { return nil }
        return path
    }
}
