import Foundation

public extension Notification.Name {
    /// Posted after an authenticated ChatOS API request proves that the
    /// currently stored access token is no longer valid.
    static let chatOSAuthenticationDidExpire = Notification.Name(
        "com.chatos.swift.authentication-did-expire"
    )
}
