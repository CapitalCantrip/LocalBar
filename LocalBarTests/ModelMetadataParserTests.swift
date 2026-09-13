import XCTest
@testable import LocalBar

final class ModelMetadataParserTests: XCTestCase {

    // MARK: - parseParameterCount

    func test_parseParamCount_7B() {
        XCTAssertEqual(ModelMetadataParser.parseParameterCount(from: "llama-7B-instruct"), "7B")
    }

    func test_parseParamCount_1point5B() {
        XCTAssertEqual(ModelMetadataParser.parseParameterCount(from: "mistral-1.5B-v0.1"), "1.5B")
    }

    func test_parseParamCount_0point5b_lowercase() {
        XCTAssertEqual(ModelMetadataParser.parseParameterCount(from: "model-0.5b"), "0.5B")
    }

    func test_parseParamCount_70B() {
        XCTAssertEqual(ModelMetadataParser.parseParameterCount(from: "Meta-Llama-3-70B"), "70B")
    }

    func test_parseParamCount_noMatch() {
        XCTAssertNil(ModelMetadataParser.parseParameterCount(from: "text-embedding-ada"))
    }

    func test_parseParamCount_bareB_notMatched() {
        // Single "b" with no digits before it should not match.
        XCTAssertNil(ModelMetadataParser.parseParameterCount(from: "b"))
    }

    func test_parseParamCount_takesFirst() {
        // "7B" appears before "13B" — first match wins.
        XCTAssertEqual(ModelMetadataParser.parseParameterCount(from: "model-7B-finetuned-13B"), "7B")
    }

    // MARK: - parseQuantization

    func test_parseQuantization_q4KM() {
        XCTAssertEqual(ModelMetadataParser.parseQuantization(from: "model-Q4_K_M.gguf"), "Q4_K_M")
    }

    func test_parseQuantization_q8_0() {
        XCTAssertEqual(ModelMetadataParser.parseQuantization(from: "llama-q8_0"), "Q8_0")
    }

    func test_parseQuantization_q5KS() {
        XCTAssertEqual(ModelMetadataParser.parseQuantization(from: "model-q5_k_s"), "Q5_K_S")
    }

    func test_parseQuantization_4bit() {
        XCTAssertEqual(ModelMetadataParser.parseQuantization(from: "mlx-community/model-4bit"), "4bit")
    }

    func test_parseQuantization_4dash_bit() {
        XCTAssertEqual(ModelMetadataParser.parseQuantization(from: "model-4-bit-quantized"), "4bit")
    }

    func test_parseQuantization_8bit() {
        XCTAssertEqual(ModelMetadataParser.parseQuantization(from: "model-8bit"), "8bit")
    }

    func test_parseQuantization_bf16() {
        XCTAssertEqual(ModelMetadataParser.parseQuantization(from: "model-bf16"), "bf16")
    }

    func test_parseQuantization_fp16() {
        XCTAssertEqual(ModelMetadataParser.parseQuantization(from: "model-fp16"), "fp16")
    }

    func test_parseQuantization_f16() {
        XCTAssertEqual(ModelMetadataParser.parseQuantization(from: "model-f16"), "f16")
    }

    func test_parseQuantization_noMatch() {
        XCTAssertNil(ModelMetadataParser.parseQuantization(from: "llama3-instruct"))
    }

    func test_parseQuantization_bf16_notMatchedAsF16() {
        // "bf16" contains "f16" — must return "bf16", not "f16".
        let result = ModelMetadataParser.parseQuantization(from: "bf16-model")
        XCTAssertEqual(result, "bf16")
    }

    // MARK: - detectFormat

    func test_detectFormat_gguf_fromFile() {
        let result = ModelMetadataParser.detectFormat(
            modelKey: "some/model",
            fileNames: ["model.gguf", "config.json"]
        )
        XCTAssertEqual(result, .gguf)
    }

    func test_detectFormat_mlxCommunity_prefix() {
        let result = ModelMetadataParser.detectFormat(
            modelKey: "mlx-community/Llama-3-8B-4bit",
            fileNames: ["config.json", "model.safetensors"]
        )
        XCTAssertEqual(result, .mlx)
    }

    func test_detectFormat_npz_isMlx() {
        let result = ModelMetadataParser.detectFormat(
            modelKey: "somemodel",
            fileNames: ["weights.npz", "config.json"]
        )
        XCTAssertEqual(result, .mlx)
    }

    func test_detectFormat_safetensors() {
        let result = ModelMetadataParser.detectFormat(
            modelKey: "org/model",
            fileNames: ["model.safetensors", "config.json"]
        )
        XCTAssertEqual(result, .safetensors)
    }

    func test_detectFormat_defaultMlx() {
        let result = ModelMetadataParser.detectFormat(
            modelKey: "org/model",
            fileNames: ["config.json"]
        )
        XCTAssertEqual(result, .mlx)
    }

    func test_detectFormat_gguf_beatsSafetensors() {
        let result = ModelMetadataParser.detectFormat(
            modelKey: "model",
            fileNames: ["weights.safetensors", "model.gguf"]
        )
        XCTAssertEqual(result, .gguf)
    }

    // MARK: - parseArchitectureFields

    func test_parseArch_standardHFConfig() {
        let config: [String: Any] = [
            "num_hidden_layers": 32,
            "num_key_value_heads": 8,
            "num_attention_heads": 32,
            "hidden_size": 4096,
        ]
        var meta = ModelMetadata()
        ModelMetadataParser.parseArchitectureFields(from: config, into: &meta)
        XCTAssertEqual(meta.numHiddenLayers, 32)
        XCTAssertEqual(meta.numKVHeads, 8)           // GQA — prefers num_key_value_heads
        XCTAssertEqual(meta.headDim, 128)             // 4096 / 32
    }

    func test_parseArch_noGQA_fallsBackToAttentionHeads() {
        let config: [String: Any] = [
            "num_hidden_layers": 24,
            "num_attention_heads": 16,
            "hidden_size": 2048,
        ]
        var meta = ModelMetadata()
        ModelMetadataParser.parseArchitectureFields(from: config, into: &meta)
        XCTAssertEqual(meta.numKVHeads, 16)
        XCTAssertEqual(meta.headDim, 128)
    }

    func test_parseArch_explicitHeadDim() {
        let config: [String: Any] = [
            "num_hidden_layers": 28,
            "num_attention_heads": 16,
            "head_dim": 256,
            "hidden_size": 4096,
        ]
        var meta = ModelMetadata()
        ModelMetadataParser.parseArchitectureFields(from: config, into: &meta)
        XCTAssertEqual(meta.headDim, 256) // explicit wins over derived
    }

    func test_parseArch_nilConfig_noOp() {
        var meta = ModelMetadata()
        ModelMetadataParser.parseArchitectureFields(from: nil, into: &meta)
        XCTAssertNil(meta.numHiddenLayers)
        XCTAssertNil(meta.numKVHeads)
        XCTAssertNil(meta.headDim)
    }

    func test_parseArch_missingKeys_nilFields() {
        let config: [String: Any] = ["model_type": "llama"]
        var meta = ModelMetadata()
        ModelMetadataParser.parseArchitectureFields(from: config, into: &meta)
        XCTAssertNil(meta.numHiddenLayers)
        XCTAssertNil(meta.numKVHeads)
        XCTAssertNil(meta.headDim)
    }

    // MARK: - detectCapabilities

    func test_detectCapabilities_vision_fromConfigKey() {
        let config: [String: Any] = ["vision_config": ["image_size": 336]]
        let caps = ModelMetadataParser.detectCapabilities(
            directoryPath: "/dev/null", modelKey: "org/model", configJSON: config)
        XCTAssertTrue(caps.contains(.vision))
    }

    func test_detectCapabilities_code_fromModelType() {
        let config: [String: Any] = ["model_type": "codellama"]
        let caps = ModelMetadataParser.detectCapabilities(
            directoryPath: "/dev/null", modelKey: "org/model", configJSON: config)
        XCTAssertTrue(caps.contains(.code))
    }

    func test_detectCapabilities_code_fromKeyName() {
        let caps = ModelMetadataParser.detectCapabilities(
            directoryPath: "/dev/null", modelKey: "org/deepseek-coder", configJSON: [:])
        XCTAssertTrue(caps.contains(.code))
    }

    func test_detectCapabilities_embedding_fromModelType() {
        let config: [String: Any] = ["model_type": "bert"]
        let caps = ModelMetadataParser.detectCapabilities(
            directoryPath: "/dev/null", modelKey: "org/model", configJSON: config)
        XCTAssertTrue(caps.contains(.embedding))
    }

    func test_detectCapabilities_embedding_fromKeyName() {
        let caps = ModelMetadataParser.detectCapabilities(
            directoryPath: "/dev/null", modelKey: "org/text-embed-model", configJSON: [:])
        XCTAssertTrue(caps.contains(.embedding))
    }

    func test_detectCapabilities_toolUse_fromChatTemplate() throws {
        // Write a temp tokenizer_config.json with "tool" in the chat template.
        let tmp = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: tmp, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: tmp) }

        let tokenizerConfig: [String: Any] = ["chat_template": "{% if tool_calls %}…{% endif %}"]
        let data = try JSONSerialization.data(withJSONObject: tokenizerConfig)
        try data.write(to: tmp.appendingPathComponent("tokenizer_config.json"))

        let caps = ModelMetadataParser.detectCapabilities(
            directoryPath: tmp.path, modelKey: "org/model", configJSON: [:])
        XCTAssertTrue(caps.contains(.toolUse))
    }

    func test_detectCapabilities_noCapabilities_emptySet() {
        let caps = ModelMetadataParser.detectCapabilities(
            directoryPath: "/dev/null", modelKey: "org/plain-model", configJSON: [:])
        XCTAssertTrue(caps.isEmpty)
    }

    // MARK: - parse (integration, uses temp filesystem)

    func test_parse_integration_populatesMetadata() throws {
        let tmp = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: tmp, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: tmp) }

        let configJSON: [String: Any] = [
            "model_type": "llama",
            "num_hidden_layers": 32,
            "num_attention_heads": 32,
            "hidden_size": 4096,
        ]
        let data = try JSONSerialization.data(withJSONObject: configJSON)
        try data.write(to: tmp.appendingPathComponent("config.json"))
        // Add a safetensors weight file to confirm format detection.
        FileManager.default.createFile(atPath: tmp.appendingPathComponent("model.safetensors").path, contents: nil)

        let meta = ModelMetadataParser.parse(directoryPath: tmp.path, modelKey: "org/llama-7B")
        XCTAssertEqual(meta.parameterCount, "7B")
        XCTAssertEqual(meta.numHiddenLayers, 32)
        XCTAssertEqual(meta.headDim, 128)
        // mlx-lm driver overrides format to .mlx post-parse, but the parser itself returns .safetensors here.
        XCTAssertEqual(meta.modelFormat, .safetensors)
    }
}
