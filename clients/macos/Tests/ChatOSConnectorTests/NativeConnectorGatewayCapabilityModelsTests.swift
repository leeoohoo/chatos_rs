import Foundation
@testable import ChatOSConnector
import XCTest

final class NativeConnectorGatewayCapabilityModelsTests: XCTestCase {
    func testUnifiedSystemAskUserResolvesToLocalBuiltinKind() throws {
        let runtime = try JSONDecoder().decode(
            GatewayMCPRuntimeDTO.self,
            from: Data(
                #"{"kind":"system","system_key":"ask_user","builtin_kind":null,"server_name":"ask_user","url":null,"headers":{}}"#.utf8
            )
        )

        XCTAssertEqual(runtime.resolvedBuiltinKind, "AskUser")
        XCTAssertFalse(runtime.isExternalHTTP)
    }

    func testLegacyBuiltinKindRemainsSupported() throws {
        let runtime = try JSONDecoder().decode(
            GatewayMCPRuntimeDTO.self,
            from: Data(
                #"{"kind":"builtin","system_key":null,"builtin_kind":"CodeMaintainerRead","server_name":"project-read","url":null,"headers":{}}"#.utf8
            )
        )

        XCTAssertEqual(runtime.resolvedBuiltinKind, "CodeMaintainerRead")
        XCTAssertFalse(runtime.isExternalHTTP)
    }

    func testOnlyHTTPRuntimeWithURLIsExternal() throws {
        let runtime = try JSONDecoder().decode(
            GatewayMCPRuntimeDTO.self,
            from: Data(
                #"{"kind":"http","system_key":null,"builtin_kind":null,"server_name":"remote-tools","url":"https://example.com/mcp","headers":{}}"#.utf8
            )
        )

        XCTAssertNil(runtime.resolvedBuiltinKind)
        XCTAssertTrue(runtime.isExternalHTTP)
    }
}
