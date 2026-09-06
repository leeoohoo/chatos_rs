import Foundation

public struct PetFilePresentationRequest: Sendable, Equatable {
    public var path: String
    public var targetLine: Int?
    public var prefersEditing: Bool

    public init(path: String, targetLine: Int? = nil, prefersEditing: Bool = false) {
        self.path = path
        self.targetLine = targetLine
        self.prefersEditing = prefersEditing
    }
}

public extension Notification.Name {
    static let chatOSPetOpenFileRequested = Notification.Name("ChatOS.pet.open-file-requested")
}
