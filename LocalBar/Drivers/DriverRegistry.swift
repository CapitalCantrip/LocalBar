import Foundation

/// Static lookup table. Adding server type N = one new struct + one line here.
enum DriverRegistry {
    static let all: [ServerType: any ServerDriver] = [
        .mlxLM:  MLXLMDriver(),
        .ollama: OllamaDriver(),
    ]

    static func driver(for type: ServerType) -> any ServerDriver {
        guard let driver = all[type] else {
            preconditionFailure("No driver registered for server type '\(type.rawValue)'")
        }
        return driver
    }
}
