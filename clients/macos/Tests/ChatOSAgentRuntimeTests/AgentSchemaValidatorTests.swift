import ChatOSAgentRuntime
import Foundation
import XCTest

final class AgentSchemaValidatorTests: XCTestCase {
    func testDefaultAnnotationDoesNotRejectAnOtherwiseValidArgument() throws {
        let schema = Data(
            #"{"type":"object","properties":{"include_terminal":{"type":"boolean","default":false}},"additionalProperties":false}"#.utf8
        )

        XCTAssertNoThrow(
            try AgentSchemaValidator.validate(
                arguments: #"{"include_terminal":true}"#,
                schema: schema
            )
        )
    }

    func testUniqueItemsAcceptsDistinctValuesAndRejectsDuplicates() throws {
        let schema = Data(
            #"{"type":"object","properties":{"refs":{"type":"array","items":{"type":"string"},"uniqueItems":true}},"required":["refs"],"additionalProperties":false}"#.utf8
        )

        XCTAssertNoThrow(
            try AgentSchemaValidator.validate(
                arguments: #"{"refs":["first","second"]}"#,
                schema: schema
            )
        )
        XCTAssertThrowsError(
            try AgentSchemaValidator.validate(
                arguments: #"{"refs":["same","same"]}"#,
                schema: schema
            )
        ) { error in
            XCTAssertEqual(error.localizedDescription, "工具参数不符合 Schema：$.refs")
        }
    }

    func testUnknownSchemaKeywordStillFailsClosed() throws {
        let schema = Data(
            #"{"type":"object","unsupported_keyword":true}"#.utf8
        )

        XCTAssertThrowsError(
            try AgentSchemaValidator.validate(arguments: "{}", schema: schema)
        ) { error in
            XCTAssertEqual(error.localizedDescription, "工具参数不符合 Schema：unsupported schema")
        }
    }

    func testDocumentAtProductByteLimitFitsInsideToolArgumentEnvelope() throws {
        let maximumDocumentBytes = 2 * 1_024 * 1_024
        let markdown = String(repeating: "\\", count: maximumDocumentBytes)
        let arguments = try JSONSerialization.data(
            withJSONObject: ["markdown": markdown],
            options: [.sortedKeys]
        )
        let schema = Data(
            """
            {"type":"object","properties":{"markdown":{"type":"string","maxLength":\(maximumDocumentBytes)}},"required":["markdown"],"additionalProperties":false}
            """.utf8
        )

        XCTAssertGreaterThan(arguments.count, maximumDocumentBytes)
        XCTAssertLessThan(arguments.count, AgentSchemaValidator.maximumArgumentBytes)
        XCTAssertNoThrow(
            try AgentSchemaValidator.validate(
                arguments: String(decoding: arguments, as: UTF8.self),
                schema: schema
            )
        )
    }
}
