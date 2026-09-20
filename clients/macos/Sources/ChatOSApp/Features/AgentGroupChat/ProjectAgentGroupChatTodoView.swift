import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation
import SwiftUI

struct TeamTodoBoardView: View {
    let todos: [LocalAgentTodo]
    let profilesByID: [String: LocalAgentProfile]

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                if todos.isEmpty {
                    ContentUnavailableView(
                        "还没有团队任务",
                        systemImage: "checklist",
                        description: Text("项目经理创建的任务会显示在这里。")
                    )
                    .padding(.top, 70)
                }
                ForEach(todos) { todo in
                    VStack(alignment: .leading, spacing: 10) {
                        HStack(alignment: .top) {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(todo.title).appFont(.headline)
                                Text(profilesByID[todo.agentID]?.draft.name ?? "Agent")
                                    .appFont(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Text(todoStatusLabel(todo.status))
                                .appFont(.caption2.weight(.semibold))
                                .padding(.horizontal, 8)
                                .padding(.vertical, 4)
                                .background(todoStatusColor(todo.status).opacity(0.14), in: Capsule())
                                .foregroundStyle(todoStatusColor(todo.status))
                        }
                        Text(todo.executionContract.objective)
                            .appFont(.body)
                            .textSelection(.enabled)
                        if !todo.executionContract.scope.isEmpty {
                            LabeledContent("范围", value: todo.executionContract.scope)
                                .appFont(.caption)
                        }
                        contractList("交付物", todo.executionContract.expectedOutputs)
                        contractList("验收条件", todo.executionContract.acceptanceCriteria)
                        if !todo.executionContract.constraints.isEmpty {
                            contractList("约束", todo.executionContract.constraints)
                        }
                        if !todo.blockedReason.isEmpty {
                            Label(todo.blockedReason, systemImage: "exclamationmark.octagon")
                                .appFont(.caption)
                                .foregroundStyle(.orange)
                        }
                        if !todo.result.isEmpty {
                            Divider()
                            MarkdownDocumentView(markdown: todo.result)
                        }
                    }
                    .padding(14)
                    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 12))
                    .overlay {
                        RoundedRectangle(cornerRadius: 12)
                            .stroke(Color(nsColor: .separatorColor), lineWidth: 1)
                    }
                }
            }
            .padding(18)
        }
    }

    private func contractList(_ title: String, _ values: [String]) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).appFont(.caption.weight(.semibold)).foregroundStyle(.secondary)
            ForEach(Array(values.enumerated()), id: \.offset) { _, value in
                Text("• \(value)").appFont(.caption).textSelection(.enabled)
            }
        }
    }

    private func todoStatusLabel(_ status: LocalAgentTodoStatus) -> String {
        switch status {
        case .pending: "待执行"
        case .inProgress: "执行中"
        case .blocked: "已阻塞"
        case .completed: "已完成"
        case .cancelled: "已取消"
        }
    }

    private func todoStatusColor(_ status: LocalAgentTodoStatus) -> Color {
        switch status {
        case .pending: .secondary
        case .inProgress: .blue
        case .blocked: .orange
        case .completed: .green
        case .cancelled: .red
        }
    }
}
