import ChatOSCore
import CryptoKit
import Foundation

public struct NativeLocalAgentExternalMCPTool: Sendable, Equatable {
  public let publicName: String
  public let upstreamName: String
  public let description: String
  public let inputSchema: LocalAgentJSONValue

  public init(
    publicName: String,
    upstreamName: String,
    description: String,
    inputSchema: LocalAgentJSONValue
  ) {
    self.publicName = publicName
    self.upstreamName = upstreamName
    self.description = description
    self.inputSchema = inputSchema
  }
}

/// A transient external MCP runtime resolved from Plugin Management. Headers
/// deliberately remain in the native process and are never published to the
/// Rust Host capability or persisted in its SQLite database.
public struct NativeLocalAgentExternalMCPConfig: Sendable, Equatable {
  public let resourceID: String
  public let serverName: String
  public let url: URL
  public let headers: [String: String]
  public let tools: [NativeLocalAgentExternalMCPTool]

  public init(
    resourceID: String,
    serverName: String,
    url: URL,
    headers: [String: String],
    tools: [NativeLocalAgentExternalMCPTool]
  ) {
    self.resourceID = resourceID
    self.serverName = serverName
    self.url = url
    self.headers = headers
    self.tools = tools
  }
}

actor NativeLocalAgentExternalMCPExecutor {
  private struct Route: Sendable {
    let config: NativeLocalAgentExternalMCPConfig
    let tool: NativeLocalAgentExternalMCPTool
  }

  private static let maximumResponseBytes = 16 * 1_024 * 1_024
  private static let forbiddenHeaders: Set<String> = [
    "accept", "connection", "content-length", "content-type", "host",
    "proxy-authenticate", "proxy-authorization", "te", "trailer",
    "transfer-encoding", "upgrade",
  ]

  private let runtime: NativeLocalAgentRuntimeClient
  private let session: URLSession
  private var routes: [String: Route] = [:]

  init(host: any LocalAgentHostClientServicing, session: URLSession = .shared) {
    runtime = .init(host: host)
    self.session = session
  }

  func configure(_ configs: [NativeLocalAgentExternalMCPConfig]) throws {
    var next: [String: Route] = [:]
    for config in configs {
      try Self.validate(config)
      for tool in config.tools {
        guard next[tool.publicName] == nil else {
          throw NativeLocalAgentExternalMCPError.duplicateTool
        }
        next[tool.publicName] = .init(config: config, tool: tool)
      }
    }
    routes = next
  }

  func reset() { routes.removeAll(keepingCapacity: false) }

  func contains(_ toolName: String) -> Bool { routes[toolName] != nil }

  func execute(
    ownerUserID: String,
    invocation: LocalAgentToolInvocationRecord
  ) async throws -> LocalAgentJSONValue {
    guard let route = routes[invocation.toolName] else {
      throw NativeLocalAgentExternalMCPError.unsupportedTool
    }
    let run = try await runtime.run(ownerUserID: ownerUserID, runID: invocation.runID)
    guard run.ownerUserID == ownerUserID,
      case .object(let input) = run.input,
      case .object(let options)? = input["tool_options"],
      case .array(let rawIDs)? = options["external_mcp_config_ids"]
    else {
      throw NativeLocalAgentExternalMCPError.invalidRunContext
    }
    let selected = Set(rawIDs.compactMap { value -> String? in
      guard case .string(let id) = value else { return nil }
      return id
    })
    guard selected.contains(route.config.resourceID) else {
      throw NativeLocalAgentExternalMCPError.capabilityNotSelected
    }
    var request = URLRequest(url: route.config.url)
    request.httpMethod = "POST"
    request.timeoutInterval = 120
    request.setValue("application/json", forHTTPHeaderField: "Accept")
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    for (name, value) in route.config.headers {
      request.setValue(value, forHTTPHeaderField: name)
    }
    let body: LocalAgentJSONValue = .object([
      "jsonrpc": .string("2.0"),
      "id": .string(invocation.callID.isEmpty ? UUID().uuidString : invocation.callID),
      "method": .string("tools/call"),
      "params": .object([
        "name": .string(route.tool.upstreamName),
        "arguments": invocation.arguments,
      ]),
    ])
    request.httpBody = try JSONEncoder().encode(body)
    let (bytes, response) = try await session.bytes(for: request)
    guard let http = response as? HTTPURLResponse,
      (200..<300).contains(http.statusCode)
    else {
      throw NativeLocalAgentExternalMCPError.remoteFailure
    }
    if let length = http.value(forHTTPHeaderField: "Content-Length").flatMap(Int.init),
      length > Self.maximumResponseBytes
    {
      throw NativeLocalAgentExternalMCPError.responseTooLarge
    }
    var data = Data()
    data.reserveCapacity(min(
      Self.maximumResponseBytes,
      http.value(forHTTPHeaderField: "Content-Length").flatMap(Int.init) ?? 64 * 1_024
    ))
    for try await byte in bytes {
      guard data.count < Self.maximumResponseBytes else {
        throw NativeLocalAgentExternalMCPError.responseTooLarge
      }
      data.append(byte)
    }
    let payload = try JSONDecoder().decode(LocalAgentJSONValue.self, from: data)
    guard case .object(let object) = payload else {
      throw NativeLocalAgentExternalMCPError.invalidResponse
    }
    if object["error"] != nil { throw NativeLocalAgentExternalMCPError.remoteFailure }
    guard let result = object["result"] else {
      throw NativeLocalAgentExternalMCPError.invalidResponse
    }
    return result
  }

  private static func validate(_ config: NativeLocalAgentExternalMCPConfig) throws {
    guard !config.resourceID.isEmpty,
      !config.serverName.isEmpty,
      config.headers.count <= 64,
      config.headers.reduce(0, { $0 + $1.key.utf8.count + $1.value.utf8.count }) <= 32 * 1_024,
      config.url.user == nil,
      config.url.password == nil,
      config.url.fragment == nil,
      config.url.host != nil,
      config.url.scheme?.lowercased() == "https" || Self.isLoopbackHTTP(config.url)
    else {
      throw NativeLocalAgentExternalMCPError.invalidConfiguration
    }
    for (rawName, value) in config.headers {
      let name = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
      guard !name.isEmpty,
        name.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-" || $0 == "_") }),
        !value.contains("\r"),
        !value.contains("\n"),
        !forbiddenHeaders.contains(name.lowercased())
      else {
        throw NativeLocalAgentExternalMCPError.invalidConfiguration
      }
    }
  }

  private static func isLoopbackHTTP(_ url: URL) -> Bool {
    guard url.scheme?.lowercased() == "http",
      let host = url.host?.lowercased() else { return false }
    return host == "localhost" || host == "127.0.0.1" || host == "::1"
  }
}

enum NativeLocalAgentExternalMCPNaming {
  static func toolName(server: String, tool: String) -> String {
    "external_mcp__\(segment(server, fallback: "server"))__\(segment(tool, fallback: "tool"))"
  }

  static func disambiguatedToolName(
    server: String,
    tool: String,
    resourceID: String,
    used: inout Set<String>
  ) -> String {
    let base = String(toolName(server: server, tool: tool).prefix(64))
    guard !used.contains(base) else {
      let digest = SHA256.hash(data: Data("\(resourceID)\u{0}\(tool)".utf8))
        .prefix(4).map { String(format: "%02x", $0) }.joined()
      let candidate = "\(String(base.prefix(55)))_\(digest)"
      used.insert(candidate)
      return candidate
    }
    used.insert(base)
    return base
  }

  private static func segment(_ value: String, fallback: String) -> String {
    var output = ""
    var separator = false
    for character in value.trimmingCharacters(in: .whitespacesAndNewlines) {
      if character.isASCII && (character.isLetter || character.isNumber || character == "_" || character == "-") {
        output.append(character)
        separator = false
      } else if !separator {
        output.append("_")
        separator = true
      }
    }
    let normalized = output.trimmingCharacters(in: CharacterSet(charactersIn: "_"))
    return normalized.isEmpty ? fallback : normalized
  }
}

private enum NativeLocalAgentExternalMCPError: LocalizedError {
  case unsupportedTool
  case duplicateTool
  case invalidConfiguration
  case invalidRunContext
  case capabilityNotSelected
  case remoteFailure
  case responseTooLarge
  case invalidResponse

  var errorDescription: String? {
    switch self {
    case .unsupportedTool: "The external MCP tool is unavailable."
    case .duplicateTool: "The external MCP tool catalog contains a duplicate name."
    case .invalidConfiguration: "The external MCP runtime configuration is invalid."
    case .invalidRunContext: "The external MCP Run context is invalid."
    case .capabilityNotSelected: "This Task did not select the external MCP configuration."
    case .remoteFailure: "The external MCP request failed."
    case .responseTooLarge: "The external MCP response exceeds the local limit."
    case .invalidResponse: "The external MCP returned an invalid response."
    }
  }
}
