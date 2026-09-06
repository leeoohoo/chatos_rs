import Foundation

struct NativePluginSkillRuntimeSession: Sendable {
    var runID: String
    var pluginID: String
    var releaseID: String
    var artifactSHA256: String
    var componentKey: String
    var adapterSessionID: String
    var workspaceID: String?
    var projectID: String?
    var expectedSnapshot: NativeJSONValue
    var expiresAt: Date

    func validate(
        pluginID: String,
        releaseID: String,
        artifactSHA256: String,
        componentKey: String,
        workspaceID: String?,
        projectID: String?
    ) throws {
        guard expiresAt > Date(),
              self.pluginID == pluginID,
              self.releaseID == releaseID,
              self.artifactSHA256 == artifactSHA256.lowercased(),
              self.componentKey == componentKey,
              self.workspaceID == workspaceID,
              self.projectID == projectID else {
            throw NativePluginRuntimeError.invalidRequest(
                "Plugin Skill 会话与当前用户、Release 或运行作用域不匹配"
            )
        }
    }
}
