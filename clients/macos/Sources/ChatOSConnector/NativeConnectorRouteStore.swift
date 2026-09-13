import Foundation

public final class NativeConnectorRouteStore: @unchecked Sendable {
    struct Route: Equatable, Sendable {
        let deviceID: String
        let workspaceID: String
    }

    private let lock = NSLock()
    private var route: Route?

    public init() {}

    func replace(deviceID: String?, workspaceID: String?) {
        let next = Self.route(deviceID: deviceID, workspaceID: workspaceID)
        lock.lock()
        route = next
        lock.unlock()
    }

    func requireCurrent() throws -> Route {
        lock.lock()
        let current = route
        lock.unlock()
        guard let current else {
            throw NativeConnectorRouteError.unavailable
        }
        return current
    }

    private static func route(deviceID: String?, workspaceID: String?) -> Route? {
        guard let deviceID = deviceID?.trimmedRouteIdentifier,
              let workspaceID = workspaceID?.trimmedRouteIdentifier else {
            return nil
        }
        return Route(deviceID: deviceID, workspaceID: workspaceID)
    }
}

enum NativeConnectorRouteError: LocalizedError, Equatable {
    case unavailable

    var errorDescription: String? {
        "当前 Local Connector 尚未建立有效的设备与工作区路由。"
    }
}

private extension String {
    var trimmedRouteIdentifier: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
