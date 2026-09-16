import Foundation

/// Lazily opens the account-scoped local Agent chat database. No remote service is required.
public actor NativeAgentGroupChatService {
    private let databaseURL: URL
    private var openedStore: SQLiteAgentGroupChatStore?

    public init(databaseURL: URL) {
        self.databaseURL = databaseURL
    }

    public func store() throws -> SQLiteAgentGroupChatStore {
        if let openedStore { return openedStore }
        let store = try SQLiteAgentGroupChatStore(databaseURL: databaseURL)
        openedStore = store
        return store
    }
}
