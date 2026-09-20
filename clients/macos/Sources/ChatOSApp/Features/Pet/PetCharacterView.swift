import ChatOSCore
import SwiftUI

enum PetDragDirection: Equatable, Sendable {
    case left
    case right
}
@MainActor
final class PetOverlayInteractionState: ObservableObject {
    @Published var isDragging = false
    @Published var dragDirection: PetDragDirection = .right
    @Published var isMessageExpanded = false
    @Published var selectedActivityID: String?
    @Published var inspectedTaskActivity: PetActivity?
    @Published var isQuickChatPresented = false
    @Published var selectedQuickChatResourceID: String?
}

enum PetMessageActivityScope: Equatable {
    case primary
    case running

    func contains(_ activity: PetActivity) -> Bool {
        switch self {
        case .primary:
            return activity.kind != .working && activity.kind != .reviewing
        case .running:
            return activity.kind == .working || activity.kind == .reviewing
        }
    }
}

struct PetCharacterView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var store: PetOverlayStore
    @ObservedObject var interactionState: PetOverlayInteractionState

    var body: some View {
        PetSpriteAnimationView(
            animationState: store.presentation.animationState,
            isDragging: interactionState.isDragging,
            dragDirection: interactionState.dragDirection
        )
        .contentShape(Rectangle())
        .accessibilityLabel(accessibilityText)
        .help(accessibilityText)
    }

    private var accessibilityText: String {
        store.presentation.primaryActivity?.title
            ?? model.localized("ChatOS 宠物空闲中", english: "ChatOS pet is idle")
    }
}
