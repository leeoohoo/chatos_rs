import Foundation

struct GlobalUtilitySearchItem: Codable, Hashable, Sendable {
    let id: String
    let title: String
    let subtitle: String?
}

@MainActor
protocol GlobalUtilityContext: AnyObject {
    var globalUtilityIsEnglish: Bool { get }
    var globalUtilityProjects: [GlobalUtilitySearchItem] { get }
    var globalUtilityContacts: [GlobalUtilitySearchItem] { get }

    func globalUtilityOpenProject(_ projectID: String)
    func globalUtilityOpenContact(_ contactID: String)
    func globalUtilityOpenSettings(runtimePermissions: Bool)
    func globalUtilityOpenTranslationImage(data: Data, suggestedName: String)
}

extension AppModel: GlobalUtilityContext {
    var globalUtilityIsEnglish: Bool {
        interfaceLanguage == .english
    }

    var globalUtilityProjects: [GlobalUtilitySearchItem] {
        projects.map {
            GlobalUtilitySearchItem(id: $0.id, title: $0.title, subtitle: $0.subtitle)
        }
    }

    var globalUtilityContacts: [GlobalUtilitySearchItem] {
        contacts.map {
            GlobalUtilitySearchItem(id: $0.id, title: $0.title, subtitle: $0.subtitle)
        }
    }

    func globalUtilityOpenProject(_ projectID: String) {
        openGlobalSearchProject(projectID)
    }

    func globalUtilityOpenContact(_ contactID: String) {
        openGlobalSearchContact(contactID)
    }

    func globalUtilityOpenSettings(runtimePermissions: Bool) {
        openGlobalSearchSettings(tab: runtimePermissions ? .runtime : nil)
    }

    func globalUtilityOpenTranslationImage(data: Data, suggestedName: String) {
        openPetTranslationImage(data: data, suggestedName: suggestedName)
    }
}
