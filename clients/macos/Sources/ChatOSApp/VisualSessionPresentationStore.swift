import Combine

@MainActor
final class VisualSessionPresentationStore: ObservableObject {
    @Published private(set) var presentations: [VisualSessionPresentation] = []
    @Published private(set) var selectedAdapterSessionID: String?

    var selectedPresentation: VisualSessionPresentation? {
        guard let selectedAdapterSessionID else { return presentations.first }
        return presentations.first {
            $0.session.adapterSessionID == selectedAdapterSessionID
        }
    }

    var selectedIndex: Int? {
        guard let selectedAdapterSessionID else { return presentations.indices.first }
        return presentations.firstIndex {
            $0.session.adapterSessionID == selectedAdapterSessionID
        }
    }

    func update(
        _ presentations: [VisualSessionPresentation],
        selectedAdapterSessionID: String?
    ) {
        let resolvedSelection = selectedAdapterSessionID.flatMap { candidate in
            presentations.contains(where: { $0.session.adapterSessionID == candidate })
                ? candidate
                : nil
        } ?? presentations.first?.session.adapterSessionID
        guard self.presentations != presentations
                || self.selectedAdapterSessionID != resolvedSelection else { return }
        self.presentations = presentations
        self.selectedAdapterSessionID = resolvedSelection
    }

    func updatePresentation(_ presentation: VisualSessionPresentation) {
        guard let index = presentations.firstIndex(where: {
            $0.session.adapterSessionID == presentation.session.adapterSessionID
        }) else { return }
        guard presentations[index] != presentation else { return }
        presentations[index] = presentation
    }

    func select(adapterSessionID: String) {
        guard selectedAdapterSessionID != adapterSessionID,
              presentations.contains(where: {
                  $0.session.adapterSessionID == adapterSessionID
              }) else { return }
        selectedAdapterSessionID = adapterSessionID
    }
}
