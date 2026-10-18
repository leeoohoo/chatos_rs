import ChatOSCore
import Foundation

public struct NativeLocalAgentMCPChoice: Sendable, Equatable {
  public let value: String
  public let title: String

  public init(value: String, title: String) {
    self.value = value
    self.title = title
  }
}

public enum NativeLocalAgentPlatformToolCatalog {
  public static let attachmentReadToolName = "local_attachment_read"
  public static let listTasksToolName = "list_tasks"
  public static let getTaskToolName = "get_task"
  public static let createTaskToolName = "create_task"
  public static let createTasksToolName = "create_tasks_with_prerequisites"
  public static let cancelTaskToolName = "cancel_task"
  public static let waitForTaskCompletionToolName = "wait_for_task_completion"
  public static let getTaskDependencyGraphToolName = "get_task_dependency_graph"
  public static let resolveTaskFollowupToolName = "resolve_task_followup"
  public static let mainChatTaskToolNames = [
    listTasksToolName,
    getTaskToolName,
    createTaskToolName,
    createTasksToolName,
    cancelTaskToolName,
    waitForTaskCompletionToolName,
    getTaskDependencyGraphToolName,
    resolveTaskFollowupToolName,
  ]

  static let projectReadOnlyToolNames = [
    "read_file_raw", "read_file_range", "list_dir", "search_text", "read_file",
    "search_files",
  ]
  private static let terminalReadOnlyToolNames = [
    "process_poll", "process_log", "process_wait",
  ]
  static let remoteConnectionToolPrefix = "remote_connection_controller_"
  static let taskExecutionRemoteReadOnlyToolNames: Set<String> = Set(
    ["test_connection", "list_directory", "read_file", "download_file"]
      .map { remoteConnectionToolPrefix + $0 }
  )
  static let taskExecutionRemoteToolNames: Set<String> = Set(
    NativeMCPRemoteConnectionController.toolNames.map { remoteConnectionToolPrefix + $0 }
  )
  private static let pluginReadOnlyToolNames =
    NativeAgentCapabilityBrokerToolCatalog.readOnlyToolNames.sorted()
  static let taskExecutionTerminalToolNames: Set<String> = [
    "execute_command", "process_poll", "process_log", "process_wait", "process_write",
    "process_kill",
  ]

  public static let readOnlyToolNames =
    [attachmentReadToolName]
    + projectReadOnlyToolNames + terminalReadOnlyToolNames
    + taskExecutionRemoteReadOnlyToolNames.sorted()
    + pluginReadOnlyToolNames
    + [
      pluginInstallerSearchNPMToolName, pluginInstallerResolveNPMToolName,
      pluginInstallerInspectNPMToolName, pluginInstallerInspectSourceToolName,
    ]
  public static let approvalExemptToolNames = [
    "open_edit_session", "stage_edit_batch", "abort_edit_session",
    NativeAgentCapabilityBrokerToolCatalog.invokeToolName,
    pluginInstallerInstallNPMToolName, pluginInstallerInstallSourceToolName,
  ]
}
