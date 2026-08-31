import ChatOSCore
import XCTest

final class MessageTaskGraphNormalizerTests: XCTestCase {
    func testReducedGraphCollapsesProjectStagesAndRemovesTransitiveEdge() {
        let graph = MessageTaskGraphSnapshot(
            rootTaskIDs: ["review"],
            nodes: [
                node("prepare", title: "准备", prerequisites: []),
                node("implement", title: "实现", prerequisites: ["prepare"], projectTaskID: "project-1"),
                node("review", title: "Review 实现", prerequisites: ["implement"], projectTaskID: "project-1"),
                node("release", title: "发布", prerequisites: ["prepare", "review"]),
            ],
            edges: []
        )

        let reduced = MessageTaskGraphNormalizer.normalize(graph, mode: .reduced)

        XCTAssertEqual(Set(reduced.nodes.map(\.id)), Set(["prepare", "implement", "release"]))
        XCTAssertEqual(
            Set(reduced.edges.map { "\($0.sourceID)->\($0.targetID)" }),
            Set(["prepare->implement", "implement->release"])
        )
        XCTAssertEqual(
            reduced.nodes.first(where: { $0.id == "implement" })?.groupedTasks.count,
            2
        )
    }

    func testFullGraphIncludesContextDependency() {
        var source = task("source", title: "生成上下文", prerequisites: [])
        source.executionClientRef = "client-source"
        var target = task("target", title: "使用上下文", prerequisites: [])
        target.dependencyContextRefs = ["client-source"]
        let graph = MessageTaskGraphSnapshot(
            rootTaskIDs: ["target"],
            nodes: [
                MessageTaskGraphNode(task: source, depth: 0, isRoot: false, isCurrentMessage: true),
                MessageTaskGraphNode(task: target, depth: 0, isRoot: true, isCurrentMessage: true),
            ],
            edges: []
        )

        let full = MessageTaskGraphNormalizer.normalize(graph, mode: .full)

        XCTAssertEqual(full.edges.count, 1)
        XCTAssertEqual(full.edges[0].kind, "context")
        XCTAssertEqual(full.edges[0].sourceID, "source")
        XCTAssertEqual(full.edges[0].targetID, "target")
    }

    private func node(
        _ id: String,
        title: String,
        prerequisites: [String],
        projectTaskID: String? = nil
    ) -> MessageTaskGraphNode {
        var task = task(id, title: title, prerequisites: prerequisites)
        task.projectTaskID = projectTaskID
        return MessageTaskGraphNode(task: task, depth: 0, isRoot: false, isCurrentMessage: true)
    }

    private func task(
        _ id: String,
        title: String,
        prerequisites: [String]
    ) -> MessageTask {
        MessageTask(id: id, title: title, status: "ready", prerequisiteTaskIDs: prerequisites)
    }
}
