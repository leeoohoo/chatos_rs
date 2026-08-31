import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSProjectPlanServiceTests: XCTestCase {
    func testPlanDecodesHierarchyCountsAndRequirementDependencies() async throws {
        let transport = ProjectPlanTransport(responses: [
            "/api/chatos/projects/project-1/plan": #"""
            {
              "project_id":"project-1",
              "requirements":[
                {"id":"root","title":"根需求","priority":90,"status":"in_progress"},
                {"id":"child","parent_requirement_id":"root","title":"子需求","priority":50,"status":"draft"}
              ],
              "work_items":[{"id":"task-1","requirement_id":"root","title":"实现","status":"todo","priority":10}],
              "work_item_counts":{"total":1,"open":1,"done":0,"blocked":0},
              "dependency_graph":{"edges":[{"from":"requirement:root","to":"requirement:child","edge_type":"depends_on"}]}
            }
            """#,
        ])
        let service = makeService(transport: transport)

        let plan = try await service.fetchPlan(projectID: "project-1")

        XCTAssertEqual(plan.requirements.count, 2)
        XCTAssertEqual(plan.requirements.first(where: { $0.id == "child" })?.parentRequirementID, "root")
        XCTAssertEqual(plan.counts.total, 1)
        XCTAssertEqual(plan.edges, [ProjectPlanEdge(sourceID: "root", targetID: "child", kind: "depends_on")])
    }

    func testDocumentsDecodeMetadataUsedByNativeTwoColumnViewer() async throws {
        let transport = ProjectPlanTransport(responses: [
            "/api/chatos/projects/project-1/requirements/req-1/documents": #"""
            [
              {
                "id":"doc-1",
                "doc_type":"implementation_plan",
                "title":"实施计划",
                "format":"markdown",
                "content":"# 计划",
                "version":3,
                "updated_at":"2026-08-25T03:00:00Z"
              }
            ]
            """#,
        ])
        let service = makeService(transport: transport)

        let documents = try await service.fetchDocuments(projectID: "project-1", requirementID: "req-1")

        XCTAssertEqual(documents.first?.type, "implementation_plan")
        XCTAssertEqual(documents.first?.version, 3)
        XCTAssertNotNil(documents.first?.updatedAt)
    }

    func testExecutionPlanDecodesIdentityAndWorkbenchState() async throws {
        let transport = ProjectPlanTransport(responses: [
            "/api/chatos/projects/project-1/requirements/req-1/execution-plan": #"""
            {
              "found":true,
              "project_id":"project-1",
              "requirement_id":"req-1",
              "conversation_id":"conversation-1",
              "execution_group_id":"group-1",
              "message_id":"message-1",
              "contact_id":"contact-1",
              "status":"awaiting_confirmation",
              "confirmation_status":"awaiting_confirmation",
              "task_count":4,
              "has_started_runs":false,
              "include_prerequisite_dependents":true,
              "created_at":"2026-08-25T03:00:00Z"
            }
            """#,
        ])
        let service = makeService(transport: transport)

        let fetched = try await service.fetchExecution(projectID: "project-1", requirementID: "req-1")
        let launch = try XCTUnwrap(fetched)

        XCTAssertEqual(launch.executionGroupID, "group-1")
        XCTAssertEqual(launch.contactID, "contact-1")
        XCTAssertEqual(launch.overallStatus, "awaiting_confirmation")
        XCTAssertEqual(launch.taskCount, 4)
        XCTAssertTrue(launch.includePrerequisiteDependents)
        XCTAssertNotNil(launch.createdAt)
    }

    private func makeService(transport: ProjectPlanTransport) -> ChatOSProjectPlanService {
        ChatOSProjectPlanService(
            client: ChatOSAPIClient(
                configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
                accessToken: "token",
                transport: transport
            )
        )
    }
}

private actor ProjectPlanTransport: HTTPTransport {
    private let responses: [String: String]

    init(responses: [String: String]) {
        self.responses = responses
    }

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        let path = request.url.path
        guard let body = responses[path] else {
            return HTTPResponse(statusCode: 404, headers: [:], body: Data(#"{"detail":"missing fixture"}"#.utf8))
        }
        return HTTPResponse(statusCode: 200, headers: [:], body: Data(body.utf8))
    }
}
