/// `taskEnabled` is a create_task candidate-list preference, not a global model switch.
public enum LocalConnectorModelSelectionScope: Sendable {
    case general
    case taskCreation
}

public enum LocalConnectorModelSelectionPolicy {
    public static func permits(
        enabled: Bool, hasAPIKey: Bool, taskEnabled: Bool,
        scope: LocalConnectorModelSelectionScope
    ) -> Bool {
        guard enabled && hasAPIKey else { return false }
        switch scope {
        case .general: return true
        case .taskCreation: return taskEnabled
        }
    }

    public static func models(
        from models: [LocalConnectorModelConfig], scope: LocalConnectorModelSelectionScope
    ) -> [LocalConnectorModelConfig] {
        models.filter {
            permits(enabled: $0.enabled, hasAPIKey: $0.hasAPIKey,
                    taskEnabled: $0.taskEnabled, scope: scope)
        }
    }
}
