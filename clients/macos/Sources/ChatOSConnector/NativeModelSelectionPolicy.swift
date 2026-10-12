import ChatOSCore

extension GatewayModelConfigDTO {
    func isSelectable(for scope: LocalConnectorModelSelectionScope) -> Bool {
        LocalConnectorModelSelectionPolicy.permits(
            enabled: enabled != false, hasAPIKey: hasAPIKey != false,
            taskEnabled: taskEnabled != false, scope: scope
        )
    }
}
