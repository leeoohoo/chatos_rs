public protocol CredentialStoring: Sendable {
    func loadAccessToken() async throws -> String?
    func saveAccessToken(_ token: String) async throws
    func deleteAccessToken() async throws
}
