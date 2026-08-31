import Foundation

struct ConversationFocusRequest: Sendable, Equatable {
    var id = UUID()
    var turnID: String?
    var promptID: String?
    var taskID: String?
    var runID: String?
}
