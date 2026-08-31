import Foundation

public struct PluginVisualSessionOwner: Sendable, Equatable {
    public var conversationID: String
    public var turnID: String?
    public var sourceUserMessageID: String?
    public var taskID: String?
    public var taskRunID: String?
    public var taskTitle: String?

    public init(
        conversationID: String,
        turnID: String? = nil,
        sourceUserMessageID: String? = nil,
        taskID: String? = nil,
        taskRunID: String? = nil,
        taskTitle: String? = nil
    ) {
        self.conversationID = conversationID
        self.turnID = turnID
        self.sourceUserMessageID = sourceUserMessageID
        self.taskID = taskID
        self.taskRunID = taskRunID
        self.taskTitle = taskTitle
    }
}

public struct PluginVisualSession: Sendable, Identifiable, Equatable {
    public var id: String
    public var adapterSessionID: String
    public var pluginID: String
    public var componentKey: String
    public var pluginDisplayName: String
    public var title: String
    public var targetApplication: String?
    public var frameSequence: UInt64
    public var capturedAt: Date?
    public var frameData: Data?
    public var mimeType: String?
    public var width: Int?
    public var height: Int?
    public var owner: PluginVisualSessionOwner

    public init(
        id: String,
        adapterSessionID: String,
        pluginID: String,
        componentKey: String,
        pluginDisplayName: String,
        title: String,
        targetApplication: String? = nil,
        frameSequence: UInt64 = 0,
        capturedAt: Date? = nil,
        frameData: Data? = nil,
        mimeType: String? = nil,
        width: Int? = nil,
        height: Int? = nil,
        owner: PluginVisualSessionOwner
    ) {
        self.id = id
        self.adapterSessionID = adapterSessionID
        self.pluginID = pluginID
        self.componentKey = componentKey
        self.pluginDisplayName = pluginDisplayName
        self.title = title
        self.targetApplication = targetApplication
        self.frameSequence = frameSequence
        self.capturedAt = capturedAt
        self.frameData = frameData
        self.mimeType = mimeType
        self.width = width
        self.height = height
        self.owner = owner
    }
}
