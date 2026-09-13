import XCTest
@testable import LocalBar

final class PathScannerTests: XCTestCase {

    // MARK: - envWhich

    func test_envWhich_knownTool_returnsPath() {
        // /bin/sh is guaranteed on macOS.
        let path = PathScanner.envWhich("sh")
        XCTAssertNotNil(path)
        XCTAssertTrue(path?.contains("sh") == true)
        XCTAssertTrue(FileManager.default.fileExists(atPath: path!))
    }

    func test_envWhich_unknownTool_returnsNil() {
        let path = PathScanner.envWhich("zzz_no_such_tool_\(UUID().uuidString)")
        XCTAssertNil(path)
    }

    func test_envWhich_ls_returnsExecutable() {
        guard let path = PathScanner.envWhich("ls") else {
            XCTFail("ls must be findable via /usr/bin/env which")
            return
        }
        XCTAssertTrue(FileManager.default.isExecutableFile(atPath: path))
    }

    // MARK: - which (async, uses login shell)

    func test_which_knownTool_returnsPath() async {
        let path = await PathScanner.which("sh")
        XCTAssertNotNil(path)
        XCTAssertTrue(path?.contains("sh") == true)
    }

    func test_which_unknownTool_returnsNil() async {
        let path = await PathScanner.which("zzz_no_such_tool_\(UUID().uuidString)")
        XCTAssertNil(path)
    }
}
