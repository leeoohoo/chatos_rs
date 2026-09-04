import SwiftUI

struct ProjectMessagesView: View {
    @EnvironmentObject private var model: AppModel
    let projectID: String

    var body: some View {
        let resource = model.projects.first(where: { $0.id == projectID })
        let projectName = resource?.title ?? projectID
        let contactName = resource?.contactName
            ?? model.localized("项目联系人", english: "Project Contact")

        Group {
            if let conversation = model.projectConversation {
                ConversationTimelineView(
                    conversation: conversation,
                    title: "\(projectName) · \(contactName)",
                    projectRootPath: model.workspaceProject(id: projectID)?.rootPath
                )
                .frame(minWidth: 620)
            } else if model.isPreparingProjectConversation(projectID: projectID) {
                VStack(spacing: 12) {
                    ProgressView()
                        .controlSize(.large)
                    Text(model.localized("正在连接“叽咕狸”…", english: "Connecting to Jiguli…"))
                        .appFont(.headline)
                    Text(model.localized(
                        "正在为项目准备首次会话，完成后即可直接发送消息。",
                        english: "Preparing the first project conversation. You can send messages when it is ready."
                    ))
                        .appFont(.subheadline)
                        .foregroundStyle(.secondary)
                }
            } else if let error = model.projectConversationPreparationError(projectID: projectID) {
                ContentUnavailableView {
                    Label(
                        model.localized("会话准备失败", english: "Conversation setup failed"),
                        systemImage: "exclamationmark.bubble"
                    )
                } description: {
                    Text(error)
                } actions: {
                    Button(model.localized("重试", english: "Retry")) {
                        model.retryProjectConversationPreparation(projectID: projectID)
                    }
                    .buttonStyle(.borderedProminent)
                }
            } else {
                ContentUnavailableView(
                    model.localized(
                        "项目还没有可用会话",
                        english: "No project conversation is available"
                    ),
                    systemImage: "bubble.left.and.bubble.right",
                    description: Text(model.localized(
                        "正在等待默认联系人“叽咕狸”可用。",
                        english: "Waiting for the default contact Jiguli to become available."
                    ))
                )
            }
        }
        .workspaceFill()
    }
}

struct ContactConversationView: View {
    @EnvironmentObject private var model: AppModel
    let contactID: String

    var body: some View {
        let contactName = model.contacts.first(where: { $0.id == contactID })?.title
            ?? model.localized("联系人", english: "Contact")

        Group {
            if let conversation = model.contactConversation {
                ConversationTimelineView(
                    conversation: conversation,
                    title: contactName
                )
            } else {
                ContentUnavailableView(
                    model.localized("还没有公开会话", english: "No conversation yet"),
                    systemImage: "bubble.left",
                    description: Text(model.localized(
                        "发送第一条消息后，会话会显示在这里。",
                        english: "The conversation will appear here after you send the first message."
                    ))
                )
            }
        }
        .workspaceFill()
        .navigationTitle(contactName)
    }
}
