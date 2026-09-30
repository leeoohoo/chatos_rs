import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    public func fetchPluginVisualSessions(
        loadFrameDataForAdapterSessionIDs: Set<String>? = nil,
        knownFrameSequencesByAdapterSessionID: [String: UInt64] = [:]
    ) async -> [PluginVisualSession] {
        let descriptors = await pluginRuntimeStore.visualDescriptors()
        return NativePluginVisualSessionReader.read(
            descriptors: descriptors,
            loadFrameDataForAdapterSessionIDs: loadFrameDataForAdapterSessionIDs,
            knownFrameSequencesByAdapterSessionID: knownFrameSequencesByAdapterSessionID
        )
    }
}
