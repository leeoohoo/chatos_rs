import ChatOSCore
import ChatOSModelCapabilities

extension GatewayModelConfigDTO {
    func isSelectable(for scope: LocalConnectorModelSelectionScope) -> Bool {
        ModelWorkload.forModel(model) == .chat && LocalConnectorModelSelectionPolicy.permits(
            enabled: enabled != false, hasAPIKey: hasAPIKey != false,
            taskEnabled: taskEnabled != false, scope: scope
        )
    }
}
