// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation

public enum LocalAgentRuntimePreferencesError: LocalizedError, Sendable {
    case invalidPolicy

    public var errorDescription: String? {
        "Agent 运行设置无效，请检查设置中的范围。"
    }
}

public struct AgentContextPolicy: Codable, Equatable, Sendable {
    public var windowTokens = 250_000
    public var outputReserveTokens = 30_000
    public var compactionThresholdTokens = 220_000
    public var maximumCompactionPasses = 8
    public var summaryTimeoutSeconds = 120
    public var summaryPollSeconds = 10

    public init() {}

    public var reservedInputLimit: Int { windowTokens - outputReserveTokens }
    public var hardInputLimit: Int { windowTokens }

    public func validate() throws {
        guard (2_048...2_000_000).contains(windowTokens),
              (256..<windowTokens).contains(outputReserveTokens),
              (512...reservedInputLimit).contains(compactionThresholdTokens),
              (1...16).contains(maximumCompactionPasses),
              (5...1_800).contains(summaryTimeoutSeconds),
              (1...30).contains(summaryPollSeconds)
        else {
            throw LocalAgentRuntimePreferencesError.invalidPolicy
        }
    }
}

public struct AgentRunPolicy: Codable, Equatable, Sendable {
    public var maximumModelCalls = 600
    public var requestTimeoutSeconds = 180
    public var runTimeoutSeconds = 7_200
    public var maximumRequestRetries = 5
    public var maximumNoProgressRounds = 8
    public var context: AgentContextPolicy? = nil

    public init() {}

    public func validate() throws {
        guard (1...10_000).contains(maximumModelCalls),
              (5...1_800).contains(requestTimeoutSeconds),
              (10...86_400).contains(runTimeoutSeconds),
              (0...10).contains(maximumRequestRetries),
              (1...100).contains(maximumNoProgressRounds)
        else {
            throw LocalAgentRuntimePreferencesError.invalidPolicy
        }
        try (context ?? AgentContextPolicy()).validate()
    }
}

public struct AgentRuntimePreferences: Codable, Equatable, Sendable {
    public enum Profile: Sendable {
        case approval
        case story
    }

    public var global = AgentRunPolicy()
    public var approvalMaximumCalls: Int?
    public var storyMaximumCalls: Int?

    public init() {}

    public func effective(_ profile: Profile) -> AgentRunPolicy {
        var policy = global
        if let value = profile == .approval ? approvalMaximumCalls : storyMaximumCalls {
            policy.maximumModelCalls = value
        }
        return policy
    }

    public func validate() throws {
        try global.validate()
        try effective(.approval).validate()
        try effective(.story).validate()
    }
}

public protocol AgentRuntimePreferencesProviding: Sendable {
    func load(ownerUserID: String) async throws -> AgentRuntimePreferences
}
