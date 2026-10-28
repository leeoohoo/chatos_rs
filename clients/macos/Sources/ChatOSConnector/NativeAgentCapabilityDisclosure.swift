import Foundation

/// Pages at JSON boundaries, before generic runtime text budgets can damage a schema.
enum NativeAgentCapabilityDisclosure {
    static let maximumResponseCharacters = 8_000

    static func response(
        _ descriptor: DescribeResponse, toolOption: String? = nil,
        query: String? = nil, offset: Int = 0, limit: Int = 8, schemaOffset: Int? = nil
    ) throws -> NativeJSONValue {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        let encoded = try JSONDecoder().decode(NativeJSONValue.self, from: encoder.encode(descriptor))
        guard var root = encoded.jsonObject else { throw Failure.invalidArguments }
        root["disclosure_version"] = .number(2)
        guard offset >= 0, (1...20).contains(limit), schemaOffset == nil || schemaOffset! >= 0 else {
            throw Failure.invalidArguments
        }
        let allTools = root["tools"]?.jsonArray ?? []
        if let toolOption {
            guard query == nil, offset == 0,
                  let tool = allTools.first(where: { $0.jsonObject?["tool_option"]?.jsonString == toolOption }),
                  var object = tool.jsonObject, let schema = object["input_schema"] else {
                throw Failure.invalidArguments
            }
            root["view"] = .string("tool")
            let required = Set(descriptor.tools.first { $0.toolOption == toolOption }?.requiredSkillRefs ?? [])
            root["skills"] = .array((root["skills"]?.jsonArray ?? []).filter {
                required.contains($0.jsonObject?["skill_ref"]?.jsonString ?? "")
            })
            let text = schema.canonicalJSONString
            object["schema_sha256"] = .string(NativePluginManifestLoader.sha256(text))
            object["schema_total_chars"] = .number(Double(text.unicodeScalars.count))
            root["tools"] = .array([.object(object)])
            if schemaOffset == nil, fits(root) { return .object(root) }
            object.removeValue(forKey: "input_schema")
            let scalars = Array(text.unicodeScalars)
            let start = schemaOffset ?? 0
            guard start <= scalars.count else { throw Failure.invalidArguments }
            object["schema_delivery"] = .string("paged")
            object["next_schema_offset"] = .number(Double(start))
            root["tools"] = .array([.object(object)])
            root["guidance"] = .string("Read this tool's schema with capability_describe using the same plugin_option and tool_option plus schema_offset=next_schema_offset. Concatenate schema_chunk pages in offset order, with the same schema_sha256, until next_schema_offset is null. These are explicit pages, not a truncated schema; do not guess omitted parameters.")
            guard fits(root) else { return overflow(descriptor, toolOption: toolOption) }
            guard schemaOffset != nil else { return .object(root) }
            var count = min(3_000, scalars.count - start)
            while true {
                object["schema_offset"] = .number(Double(start))
                object["schema_chunk"] = .string(String(String.UnicodeScalarView(scalars[start..<(start + count)])))
                object["next_schema_offset"] = start + count < scalars.count ? .number(Double(start + count)) : .null
                root["tools"] = .array([.object(object)])
                if fits(root) { return .object(root) }
                guard count > 1 else { return overflow(descriptor, toolOption: toolOption) }
                count /= 2
            }
        }
        guard schemaOffset == nil else { throw Failure.invalidArguments }
        let terms = (query ?? "").lowercased().split(whereSeparator: { $0.isWhitespace }).map(String.init)
        let filtered = allTools.filter { tool in
            guard !terms.isEmpty else { return true }
            let object = tool.jsonObject ?? [:]
            let text = "\(object["name"]?.jsonString ?? "") \(object["description"]?.jsonString ?? "")".lowercased()
            return terms.contains(where: text.contains)
        }
        guard offset <= filtered.count else { throw Failure.invalidArguments }
        let catalogSkills = root["skills"]?.jsonArray ?? []
        root["view"] = .string("catalog")
        root["total_available_tools"] = .number(Double(allTools.count))
        root["total_matching_tools"] = .number(Double(filtered.count))
        root["offset"] = .number(Double(offset))
        root["guidance"] = .string("This catalog includes as many complete frozen tool schemas as fit safely. Entries with input_schema, schema_sha256, and skill_gate are ready for capability_invoke after activating their exact required_skill_refs; pass up to four refs together in agent_skill_activate.skill_refs. Brief-only entries must be read with capability_describe(plugin_option, tool_option) before invocation. Use query or next_offset to narrow or continue the catalog.")
        var page: [NativeJSONValue] = []
        for tool in filtered.dropFirst(offset).prefix(limit) {
            let object = tool.jsonObject ?? [:]
            let brief = object.filter { key, _ in
                ["tool_option", "name", "description", "effect", "invocation_mode", "required_skill_refs"].contains(key)
            }
            var complete = object
            if let schema = complete["input_schema"] {
                let text = schema.canonicalJSONString
                complete["schema_sha256"] = .string(NativePluginManifestLoader.sha256(text))
                complete["schema_total_chars"] = .number(Double(text.unicodeScalars.count))
            }
            let completeCandidate = page + [.object(complete)]
            root["tools"] = .array(completeCandidate)
            root["skills"] = .array(skillsRequired(by: completeCandidate, from: catalogSkills))
            root["next_offset"] = .number(Double(offset + page.count + 1))
            if complete["input_schema"] != nil, fits(root) {
                page.append(.object(complete))
                continue
            }
            let briefCandidate = page + [.object(brief)]
            root["tools"] = .array(briefCandidate)
            root["skills"] = .array(skillsRequired(by: briefCandidate, from: catalogSkills))
            guard fits(root) else { break }
            page.append(.object(brief))
        }
        root["tools"] = .array(page)
        root["skills"] = .array(skillsRequired(by: page, from: catalogSkills))
        let next = offset + page.count
        root["next_offset"] = next < filtered.count ? .number(Double(next)) : .null
        guard fits(root), next == filtered.count || !page.isEmpty else {
            return overflow(descriptor, toolOption: nil)
        }
        return .object(root)
    }

    private static func skillsRequired(
        by tools: [NativeJSONValue],
        from skills: [NativeJSONValue]
    ) -> [NativeJSONValue] {
        let required = Set(tools.flatMap { tool in
            tool.jsonObject?["required_skill_refs"]?.jsonArray?.compactMap(\.jsonString) ?? []
        })
        return skills.filter {
            required.contains($0.jsonObject?["skill_ref"]?.jsonString ?? "")
        }
    }

    private static func fits(_ value: [String: NativeJSONValue]) -> Bool {
        NativeJSONValue.object(value).canonicalJSONString.unicodeScalars.count <= maximumResponseCharacters
    }

    private static func overflow(_ descriptor: DescribeResponse, toolOption: String?) -> NativeJSONValue {
        .object([
            "disclosure_version": .number(2), "view": .string("error"),
            "error": .string("capability_description_entry_too_large"),
            "plugin_option": .string(descriptor.pluginOption),
            "tool_option": toolOption.map(NativeJSONValue.string) ?? .null,
            "guidance": .string("The catalog or tool metadata alone exceeds the safe response budget. No schema was truncated. The capability publisher must reduce this metadata before this entry can be used."),
        ])
    }

    enum Failure: Error { case invalidArguments }
}
