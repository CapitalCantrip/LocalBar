import XCTest
@testable import LocalBar

/// Tests for ModelMetadata memory-footprint estimation helpers.
/// These exercise the private `bitsPerParam` and `parseParamCountToDouble`
/// indirectly via the public `estimatedWeightBytes` / `estimatedKVCacheBytes` APIs.
final class ModelMetadataTests: XCTestCase {

    // MARK: - estimatedWeightBytes — directSizeBytes preferred

    func test_weightBytes_prefersDirect() {
        var meta = ModelMetadata()
        meta.parameterCount = "7B"
        meta.quantization = "q4"
        // directSizeBytes should win over the param-count heuristic.
        XCTAssertEqual(meta.estimatedWeightBytes(directSizeBytes: 9_999), 9_999)
    }

    func test_weightBytes_directZeroFallsBackToHeuristic() {
        var meta = ModelMetadata()
        meta.parameterCount = "1B"
        meta.quantization = nil // → 16.0 bits
        // 1e9 * 16 / 8 = 2_000_000_000
        XCTAssertEqual(meta.estimatedWeightBytes(directSizeBytes: 0), 2_000_000_000)
    }

    func test_weightBytes_nilWhenNoParamCount() {
        let meta = ModelMetadata()
        XCTAssertNil(meta.estimatedWeightBytes(directSizeBytes: nil))
    }

    func test_weightBytes_nilWhenBadParamCountFormat() {
        var meta = ModelMetadata()
        meta.parameterCount = "sevenbillion"
        XCTAssertNil(meta.estimatedWeightBytes(directSizeBytes: nil))
    }

    // MARK: - bitsPerParam via estimatedWeightBytes

    private func bits(for quantization: String?) -> Double {
        var meta = ModelMetadata()
        meta.parameterCount = "1B"  // 1e9 params
        meta.quantization = quantization
        guard let bytes = meta.estimatedWeightBytes(directSizeBytes: nil) else { return -1 }
        return Double(bytes) / 1_000_000_000.0 * 8.0
    }

    func test_bits_q4Prefix() {
        XCTAssertEqual(bits(for: "q4_k_m"), 4.5, accuracy: 0.01)
        XCTAssertEqual(bits(for: "Q4_K_M"), 4.5, accuracy: 0.01)
        XCTAssertEqual(bits(for: "q4_0"), 4.5, accuracy: 0.01)
    }

    func test_bits_q5Prefix() {
        XCTAssertEqual(bits(for: "q5_k_s"), 5.5, accuracy: 0.01)
    }

    func test_bits_q6Prefix() {
        XCTAssertEqual(bits(for: "q6_k"), 6.5, accuracy: 0.01)
    }

    func test_bits_q8Prefix() {
        XCTAssertEqual(bits(for: "q8_0"), 8.0, accuracy: 0.01)
    }

    func test_bits_4bitKeyword() {
        XCTAssertEqual(bits(for: "4bit"), 4.5, accuracy: 0.01)
        XCTAssertEqual(bits(for: "4-bit"), 4.5, accuracy: 0.01)
    }

    func test_bits_8bitKeyword() {
        XCTAssertEqual(bits(for: "8bit"), 8.0, accuracy: 0.01)
        XCTAssertEqual(bits(for: "8-bit"), 8.0, accuracy: 0.01)
    }

    func test_bits_f16() {
        XCTAssertEqual(bits(for: "f16"), 16.0, accuracy: 0.01)
    }

    func test_bits_fp16() {
        XCTAssertEqual(bits(for: "fp16"), 16.0, accuracy: 0.01)
    }

    func test_bits_bf16() {
        XCTAssertEqual(bits(for: "bf16"), 16.0, accuracy: 0.01)
    }

    func test_bits_unknownDefaultsSixteen() {
        XCTAssertEqual(bits(for: "weird-format"), 16.0, accuracy: 0.01)
    }

    func test_bits_nilDefaultsSixteen() {
        XCTAssertEqual(bits(for: nil), 16.0, accuracy: 0.01)
    }

    // MARK: - parseParamCountToDouble (via estimatedWeightBytes)

    func test_paramCount_7B() {
        var meta = ModelMetadata(); meta.parameterCount = "7B"; meta.quantization = nil
        let bytes = meta.estimatedWeightBytes(directSizeBytes: nil)!
        // 7e9 * 16 / 8 = 14_000_000_000
        XCTAssertEqual(bytes, 14_000_000_000)
    }

    func test_paramCount_1point5B() {
        var meta = ModelMetadata(); meta.parameterCount = "1.5B"; meta.quantization = nil
        let bytes = meta.estimatedWeightBytes(directSizeBytes: nil)!
        XCTAssertEqual(bytes, Int64(1.5e9 * 16.0 / 8.0))
    }

    func test_paramCount_lowercaseb() {
        var meta = ModelMetadata(); meta.parameterCount = "7b"; meta.quantization = nil
        XCTAssertNotNil(meta.estimatedWeightBytes(directSizeBytes: nil))
    }

    func test_paramCount_noSuffixReturnsNil() {
        var meta = ModelMetadata(); meta.parameterCount = "7"; meta.quantization = nil
        XCTAssertNil(meta.estimatedWeightBytes(directSizeBytes: nil))
    }

    // MARK: - estimatedKVCacheBytes

    func test_kvCache_knownArchitecture() {
        var meta = ModelMetadata()
        meta.numHiddenLayers = 32
        meta.numKVHeads = 8
        meta.headDim = 128
        // 2 * 32 * 8 * 128 * 4096 * 2 (bytes at bf16) = 2 * 32 * 8 * 128 * 4096 * 2
        let expected = Int64(2.0 * 32 * 8 * 128 * 4096 * 2)
        XCTAssertEqual(meta.estimatedKVCacheBytes(contextLength: 4096, kvCacheBits: 16), expected)
    }

    func test_kvCache_nilWhenMissingLayers() {
        var meta = ModelMetadata()
        meta.numKVHeads = 8; meta.headDim = 128
        XCTAssertNil(meta.estimatedKVCacheBytes(contextLength: 2048))
    }

    func test_kvCache_nilWhenMissingKVHeads() {
        var meta = ModelMetadata()
        meta.numHiddenLayers = 32; meta.headDim = 128
        XCTAssertNil(meta.estimatedKVCacheBytes(contextLength: 2048))
    }

    func test_kvCache_nilWhenMissingHeadDim() {
        var meta = ModelMetadata()
        meta.numHiddenLayers = 32; meta.numKVHeads = 8
        XCTAssertNil(meta.estimatedKVCacheBytes(contextLength: 2048))
    }

    func test_kvCache_4bitHalfTheSize() {
        var meta = ModelMetadata()
        meta.numHiddenLayers = 32; meta.numKVHeads = 8; meta.headDim = 128
        let bf16 = meta.estimatedKVCacheBytes(contextLength: 2048, kvCacheBits: 16)!
        let int4 = meta.estimatedKVCacheBytes(contextLength: 2048, kvCacheBits: 8)!
        XCTAssertEqual(bf16 / 2, int4)
    }
}
