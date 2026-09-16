import AppKit
import ChatOSAPI
import SwiftUI

struct WeChatCompanionSettingsView: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var viewModel: WeChatCompanionSettingsViewModel
    @State private var showsUnbindConfirmation = false

    init(service: ChatOSWeChatCompanionService) {
        _viewModel = StateObject(wrappedValue: WeChatCompanionSettingsViewModel(service: service))
    }

    var body: some View {
        SettingsGroupedPage {
            bindingCard
            sessionsCard
        }
        .task { await viewModel.load() }
        .onDisappear { viewModel.stopPolling() }
        .confirmationDialog(
            model.localized("解除微信绑定？", english: "Unbind WeChat?"),
            isPresented: $showsUnbindConfirmation,
            titleVisibility: .visible
        ) {
            Button(model.localized("解除绑定", english: "Unbind"), role: .destructive) {
                Task { await viewModel.unbind() }
            }
        } message: {
            Text(model.localized(
                "所有微信小程序登录会立即失效。桌面设备、项目和会话不会被删除。",
                english: "All Mini Program sessions will be revoked. Desktop devices, projects, and conversations will not be deleted."
            ))
        }
    }

    private var bindingCard: some View {
        LocalConnectorCard(
            model.localized("微信账号绑定", english: "WeChat Account Binding"),
            subtitle: model.localized(
                "扫码后仍需在这台已登录的 Mac 上确认。",
                english: "Scanning still requires confirmation on this signed-in Mac."
            ),
            systemImage: "qrcode.viewfinder"
        ) {
            VStack(alignment: .leading, spacing: 16) {
                if viewModel.isLoading && viewModel.binding == nil {
                    HStack(spacing: 10) {
                        ProgressView().controlSize(.small)
                        Text(model.localized("正在读取绑定状态…", english: "Loading binding status…"))
                            .foregroundStyle(.secondary)
                    }
                } else if viewModel.binding?.bound == true {
                    boundContent
                } else if let ticket = viewModel.ticket {
                    ticketContent(ticket)
                } else {
                    unboundContent
                }

                if let notice = viewModel.notice {
                    Label(notice, systemImage: "checkmark.circle.fill")
                        .appFont(.caption)
                        .foregroundStyle(.green)
                }
                if let error = viewModel.errorMessage {
                    Label(error, systemImage: "exclamationmark.triangle.fill")
                        .appFont(.caption)
                        .foregroundStyle(.orange)
                        .textSelection(.enabled)
                }
            }
        }
    }

    private var boundContent: some View {
        HStack(alignment: .center, spacing: 14) {
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 34))
                .foregroundStyle(.green)
            VStack(alignment: .leading, spacing: 4) {
                Text(model.localized("微信小程序已绑定", english: "WeChat Mini Program is bound"))
                    .appFont(.headline)
                Text(model.localized(
                    "可从微信查看设备、继续会话和处理交互请求。",
                    english: "WeChat can view devices, continue conversations, and answer requests."
                ))
                .appFont(.caption)
                .foregroundStyle(.secondary)
            }
            Spacer()
            Button(model.localized("解除绑定", english: "Unbind"), role: .destructive) {
                showsUnbindConfirmation = true
            }
            .disabled(viewModel.isMutating)
        }
    }

    private var unboundContent: some View {
        HStack(alignment: .center, spacing: 16) {
            VStack(alignment: .leading, spacing: 4) {
                Text(model.localized("尚未绑定微信", english: "WeChat is not bound"))
                    .appFont(.headline)
                Text(model.localized(
                    "小程序码两分钟内有效，并且只能使用一次。",
                    english: "The Mini Program code is valid for two minutes and can only be used once."
                ))
                .appFont(.caption)
                .foregroundStyle(.secondary)
            }
            Spacer()
            Button(model.localized("生成小程序码", english: "Generate Code")) {
                Task { await viewModel.issueTicket() }
            }
            .buttonStyle(.borderedProminent)
            .disabled(viewModel.isMutating)
        }
    }

    private func ticketContent(_ ticket: WeChatBindTicket) -> some View {
        HStack(alignment: .top, spacing: 24) {
            Group {
                if let image = NSImage(data: ticket.codeImageData) {
                    Image(nsImage: image)
                        .resizable()
                        .interpolation(.none)
                        .scaledToFit()
                } else {
                    Image(systemName: "qrcode")
                        .font(.system(size: 72))
                        .foregroundStyle(.secondary)
                }
            }
            .frame(width: 190, height: 190)
            .padding(10)
            .background(.white, in: RoundedRectangle(cornerRadius: 18))
            .overlay { RoundedRectangle(cornerRadius: 18).stroke(.quaternary) }

            VStack(alignment: .leading, spacing: 12) {
                Text(viewModel.ticketStatus == "claimed"
                     ? model.localized("微信设备请求绑定", english: "A WeChat device requested binding")
                     : model.localized("用微信扫描小程序码", english: "Scan with WeChat"))
                    .appFont(.headline)
                Text(viewModel.ticketStatus == "claimed"
                     ? model.localized(
                        "请确认刚才是你本人扫码。确认后，该微信账号将能访问当前账号的 Companion 功能。",
                        english: "Confirm that you scanned the code. The WeChat account will receive Companion access."
                     )
                     : model.localized(
                        "扫码后此处会出现二次确认。不要确认来自截图转发或陌生设备的请求。",
                        english: "A second confirmation appears here. Never approve a forwarded screenshot or unknown device."
                     ))
                    .appFont(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Text(viewModel.expiryText)
                    .appFont(.caption.monospacedDigit())
                    .foregroundStyle(viewModel.ticketStatus == "expired" ? .red : .secondary)
                HStack {
                    if viewModel.ticketStatus == "claimed" {
                        Button(model.localized("确认绑定", english: "Confirm Binding")) {
                            Task { await viewModel.confirmTicket() }
                        }
                        .buttonStyle(.borderedProminent)
                    }
                    Button(model.localized("重新生成", english: "Regenerate")) {
                        Task { await viewModel.issueTicket() }
                    }
                    .disabled(viewModel.isMutating)
                }
            }
            Spacer(minLength: 0)
        }
    }

    private var sessionsCard: some View {
        LocalConnectorCard(
            model.localized("小程序登录设备", english: "Mini Program Sessions"),
            subtitle: model.localized(
                "撤销丢失设备或不再使用的登录。",
                english: "Revoke access for lost devices or sessions you no longer use."
            ),
            systemImage: "iphone.gen3"
        ) {
            if viewModel.sessions.isEmpty {
                Text(model.localized("暂无小程序登录记录。", english: "No Mini Program sessions."))
                    .foregroundStyle(.secondary)
            } else {
                VStack(spacing: 0) {
                    ForEach(Array(viewModel.sessions.enumerated()), id: \.element.id) { index, session in
                        if index > 0 { Divider().padding(.vertical, 12) }
                        HStack(spacing: 12) {
                            Image(systemName: session.revokedAt == nil ? "iphone" : "iphone.slash")
                                .frame(width: 26)
                                .foregroundStyle(
                                    session.revokedAt == nil ? Color.accentColor : Color.secondary
                                )
                            VStack(alignment: .leading, spacing: 3) {
                                Text(model.localized("微信小程序", english: "WeChat Mini Program"))
                                    .appFont(.headline)
                                Text(sessionDetail(session))
                                    .appFont(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            if session.revokedAt == nil {
                                Button(model.localized("撤销", english: "Revoke"), role: .destructive) {
                                    Task { await viewModel.revoke(session) }
                                }
                                .disabled(viewModel.isMutating)
                            } else {
                                Text(model.localized("已撤销", english: "Revoked"))
                                    .appFont(.caption.weight(.semibold))
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }
        }
    }

    private func sessionDetail(_ session: WeChatClientSession) -> String {
        let date = ISO8601DateFormatter().date(from: session.lastSeenAt)
        let formatted = date?.formatted(date: .abbreviated, time: .shortened) ?? session.lastSeenAt
        return model.localized("最近活动：\(formatted)", english: "Last active: \(formatted)")
    }
}

@MainActor
final class WeChatCompanionSettingsViewModel: ObservableObject {
    @Published private(set) var binding: WeChatBindingStatus?
    @Published private(set) var sessions: [WeChatClientSession] = []
    @Published private(set) var ticket: WeChatBindTicket?
    @Published private(set) var ticketStatus = ""
    @Published private(set) var expiryText = ""
    @Published private(set) var isLoading = false
    @Published private(set) var isMutating = false
    @Published private(set) var errorMessage: String?
    @Published private(set) var notice: String?

    private let service: ChatOSWeChatCompanionService
    private var pollingTask: Task<Void, Never>?

    init(service: ChatOSWeChatCompanionService) {
        self.service = service
    }

    func load() async {
        guard !isLoading else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            async let binding = service.bindingStatus()
            async let sessions = service.clientSessions()
            self.binding = try await binding
            self.sessions = try await sessions
            errorMessage = nil
            if let ticket, ticketStatus == "issued" {
                startPolling(ticket)
            }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func issueTicket() async {
        guard !isMutating else { return }
        isMutating = true
        stopPolling()
        defer { isMutating = false }
        do {
            let ticket = try await service.issueBindTicket()
            self.ticket = ticket
            ticketStatus = "issued"
            notice = nil
            errorMessage = nil
            updateExpiryText(ticket)
            startPolling(ticket)
        } catch {
            errorMessage = error.localizedDescription
            if let ticket,
               ticketStatus == "issued",
               Date().timeIntervalSince1970 < Double(ticket.expiresAtUnix) {
                startPolling(ticket)
            }
        }
    }

    func confirmTicket() async {
        guard let ticket, ticketStatus == "claimed", !isMutating else { return }
        isMutating = true
        defer { isMutating = false }
        do {
            try await service.confirmBindTicket(id: ticket.id)
            await finishConfirmedBinding()
        } catch {
            let confirmationError = error
            do {
                let latestBinding = try await service.bindingStatus()
                if latestBinding.bound {
                    binding = latestBinding
                    await finishConfirmedBinding()
                } else {
                    errorMessage = confirmationError.localizedDescription
                }
            } catch {
                errorMessage = confirmationError.localizedDescription
            }
        }
    }

    func revoke(_ session: WeChatClientSession) async {
        guard !isMutating, session.revokedAt == nil else { return }
        isMutating = true
        defer { isMutating = false }
        do {
            try await service.revokeClientSession(id: session.id)
            sessions = try await service.clientSessions()
            notice = "已撤销该小程序登录。"
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func unbind() async {
        guard !isMutating else { return }
        isMutating = true
        defer { isMutating = false }
        do {
            try await service.unbind()
            stopPolling()
            ticket = nil
            binding = WeChatBindingStatus(bound: false, createdAt: nil, lastLoginAt: nil)
            sessions = try await service.clientSessions()
            notice = "微信账号已解除绑定，所有小程序登录已撤销。"
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func stopPolling() {
        pollingTask?.cancel()
        pollingTask = nil
    }

    private func startPolling(_ ticket: WeChatBindTicket) {
        pollingTask?.cancel()
        pollingTask = Task { [weak self] in
            guard let self else { return }
            var retryDelaySeconds = 2
            while !Task.isCancelled {
                updateExpiryText(ticket)
                if Date().timeIntervalSince1970 >= Double(ticket.expiresAtUnix) {
                    ticketStatus = "expired"
                    expiryText = "小程序码已过期，请重新生成。"
                    return
                }
                do {
                    try await Task.sleep(for: .seconds(retryDelaySeconds))
                    let status = try await service.bindTicketStatus(id: ticket.id)
                    ticketStatus = status.status
                    errorMessage = nil
                    retryDelaySeconds = 2
                    if status.status == "claimed" || status.status == "expired" {
                        updateExpiryText(ticket)
                        return
                    }
                } catch is CancellationError {
                    return
                } catch {
                    errorMessage = error.localizedDescription
                    retryDelaySeconds = min(retryDelaySeconds * 2, 8)
                }
            }
        }
    }

    private func finishConfirmedBinding() async {
        stopPolling()
        ticket = nil
        ticketStatus = ""
        binding = WeChatBindingStatus(bound: true, createdAt: nil, lastLoginAt: nil)
        notice = "微信小程序绑定已确认。"
        errorMessage = nil
        try? await Task.sleep(for: .seconds(2))
        await load()
    }

    private func updateExpiryText(_ ticket: WeChatBindTicket) {
        let remaining = max(0, ticket.expiresAtUnix - Int64(Date().timeIntervalSince1970))
        expiryText = remaining > 0
            ? "小程序码将在 \(remaining) 秒后过期"
            : "小程序码已过期，请重新生成。"
    }
}
