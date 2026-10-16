import ChatOSConnector
import SwiftUI

enum RequirementPlanConfirmationPresentation {
    static let confirmedTitle = "方案已确认，本次不会自动执行任何任务"
    static let confirmedDetail = "确认只表示认可当前方案版本。需要评审、生成文档或开始实施时，请返回原对话说明下一步；新任务会固定使用这个已确认版本。"
    static let returnToConversationTitle = "返回原对话，安排下一步"
}

/// The answer receipt and the model's follow-up are separate facts. Only show
/// the exact source Run's user-facing final reply; submission alone is not success.
struct RequirementSurveyFollowUpView: View {
    @EnvironmentObject private var model: AppModel
    let survey: LocalAgentHostRequirementSurvey
    let client: NativeLocalAgentRequirementSurveyClient
    let eventHub: NativeLocalAgentEventHub?
    @State private var followUp: RequirementSurveyFollowUp?
    @State private var loadFailed = false
    @State private var plan: LocalAgentHostRequirementPlan?
    @State private var planFollowUp: RequirementSurveyFollowUp?
    @State private var confirming = false
    @State private var confirmationFailed = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Label("方案与后续回复", systemImage: "doc.text")
                .appFont(.title3.weight(.semibold))
            if loadFailed {
                Text("暂时无法读取后续结果，请重试。")
                Button("重试") { Task { await refresh() } }
            } else if let plan {
                HStack {
                    Text("方案版本")
                    Text(String(plan.revision)).monospacedDigit()
                    Spacer()
                    if plan.confirmedAtUnixMs != nil {
                        Label("方案已确认", systemImage: "checkmark.seal")
                    } else {
                        Label("等待确认", systemImage: "person.crop.circle.badge.questionmark")
                    }
                }
                MarkdownDocumentView(markdown: plan.markdown)
                if plan.confirmedAtUnixMs == nil {
                    Text("确认仅认可当前方案，不会自动开始任务。后续创建的任务将固定引用此版本；修改方案后需要重新确认。")
                        .foregroundStyle(.secondary)
                    Button("确认当前方案") { Task { await confirm(plan) } }
                        .disabled(confirming || !planGenerationSucceeded)
                    if !planGenerationSucceeded { statusMessage(planFollowUp) }
                } else {
                    VStack(alignment: .leading, spacing: 10) {
                        Label(RequirementPlanConfirmationPresentation.confirmedTitle, systemImage: "checkmark.circle")
                            .fontWeight(.medium)
                        Text(RequirementPlanConfirmationPresentation.confirmedDetail)
                            .foregroundStyle(.secondary)
                        Button {
                            _ = model.openConversation(plan.sourceConversationID)
                        } label: {
                            Label(RequirementPlanConfirmationPresentation.returnToConversationTitle, systemImage: "bubble.left.and.bubble.right")
                        }
                        .buttonStyle(.borderedProminent)
                    }
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.green.opacity(0.08), in: RoundedRectangle(cornerRadius: 10))
                }
                if confirmationFailed {
                    Text("确认未完成，方案可能已更新。请重新查看当前版本后再确认。")
                        .foregroundStyle(.secondary)
                    Button("刷新") { Task { await refresh() } }
                }
            } else {
                statusMessage(followUp)
                if case let .completed(markdown) = followUp {
                    Text("这份历史回复尚未保存为方案版本。确认仅认可方案，不会自动开始任务。")
                        .foregroundStyle(.secondary)
                    Button("保存并确认此方案") { Task { await adoptAndConfirm(markdown) } }
                        .disabled(confirming)
                    if confirmationFailed {
                        Text("保存或确认未完成，请刷新后查看当前方案。")
                        Button("刷新") { Task { await refresh() } }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(18)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
        .task(id: survey.version) {
            let updates = await eventHub?.updates()
            await refresh()
            if let updates {
                for await update in updates {
                    guard !Task.isCancelled else { return }
                    guard update.ownerUserID == survey.ownerUserID else { continue }
                    switch update.kind {
                    case .reconcile: await refresh()
                    case let .events(events):
                        if events.contains(where: {
                            ($0.runID == survey.sourceRunID || $0.runID == plan?.sourceRunID || $0.eventType.hasPrefix("requirement_plan_"))
                                && RequirementSurveyMonitoringPolicy.shouldRefresh(forEventTypes: [$0.eventType])
                        }) { await refresh() }
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func statusMessage(_ status: RequirementSurveyFollowUp?) -> some View {
                switch status {
                case let .completed(markdown):
                    MarkdownDocumentView(markdown: markdown)
                case .processing:
                    Label("正在根据你的回答整理需求与后续方案…", systemImage: "sparkles")
                case .waitingForInput:
                    Text("还需要补充信息，请查看待填写调研或原对话。")
                case .needsReview:
                    Text("后续处理已暂停，请在原对话中查看需要处理的事项。")
                case .failed:
                    Text("后续处理未完成，请在原对话中查看原因并重试。")
                case .cancelled:
                    Text("后续处理已取消，你提交的回答仍已保留。")
                case .completedWithoutReply:
                    Text("后续处理已结束，暂无方案回复。请在原对话中查看详情。")
                case .awaitingAnswers:
                    Text("请先填写并提交调研。")
                case nil:
                    ProgressView().controlSize(.small)
                }
    }

    private var planGenerationSucceeded: Bool {
        switch planFollowUp {
        case .completed, .completedWithoutReply: true
        default: false
        }
    }

    @MainActor
    private func refresh() async {
        do {
            let result = try await client.followUp(ownerUserID: survey.ownerUserID, survey: survey)
            let currentPlan = try await client.planClient.get(ownerUserID: survey.ownerUserID, survey: survey)
            var currentPlanFollowUp: RequirementSurveyFollowUp?
            if let currentPlan {
                currentPlanFollowUp = try await client.planClient.sourceFollowUp(ownerUserID: survey.ownerUserID, plan: currentPlan)
            }
            guard !Task.isCancelled else { return }
            followUp = result
            plan = currentPlan
            planFollowUp = currentPlanFollowUp
            loadFailed = false
        } catch {
            guard !Task.isCancelled else { return }
            loadFailed = true
        }
    }

    @MainActor
    private func confirm(_ currentPlan: LocalAgentHostRequirementPlan) async {
        guard !confirming else { return }
        confirming = true
        defer { confirming = false }
        do {
            plan = try await client.planClient.confirm(ownerUserID: survey.ownerUserID, survey: survey, plan: currentPlan)
            confirmationFailed = false
        } catch { confirmationFailed = true }
    }

    @MainActor
    private func adoptAndConfirm(_ markdown: String) async {
        guard !confirming else { return }
        confirming = true
        defer { confirming = false }
        do {
            let saved = try await client.planClient.saveSourceReply(ownerUserID: survey.ownerUserID, survey: survey, markdown: markdown)
            plan = try await client.planClient.confirm(ownerUserID: survey.ownerUserID, survey: survey, plan: saved)
            confirmationFailed = false
        } catch { confirmationFailed = true }
        await refresh()
    }
}
