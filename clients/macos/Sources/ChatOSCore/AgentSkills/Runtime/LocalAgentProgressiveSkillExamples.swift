import Foundation

/// Individually authored examples for every profession and project-type Skill.
/// The entry instructions stay compact; examples are disclosed only when the resource is read.
enum LocalAgentProgressiveSkillExampleCatalog {
    struct Profile: Sendable {
        let situationZH: String
        let situationEN: String
        let goodExampleZH: String
        let goodExampleEN: String
        let evidenceZH: String
        let evidenceEN: String
        let counterexampleZH: String
        let counterexampleEN: String
        let correctionZH: String
        let correctionEN: String

        init(
            zh: (
                situation: String,
                good: String,
                evidence: String,
                counterexample: String,
                correction: String
            ),
            en: (
                situation: String,
                good: String,
                evidence: String,
                counterexample: String,
                correction: String
            )
        ) {
            situationZH = zh.situation
            goodExampleZH = zh.good
            evidenceZH = zh.evidence
            counterexampleZH = zh.counterexample
            correctionZH = zh.correction
            situationEN = en.situation
            goodExampleEN = en.good
            evidenceEN = en.evidence
            counterexampleEN = en.counterexample
            correctionEN = en.correction
        }
    }

    static func profile(
        kind: LocalAgentProgressiveSkillKind,
        key: String
    ) -> Profile? {
        switch kind {
        case .profession: professionProfiles[key]
        case .projectType: projectTypeProfiles[key]
        }
    }

    static func resource(
        kind: LocalAgentProgressiveSkillKind,
        key: String,
        label: String,
        language: ChatOSLanguage
    ) -> LocalAgentProgressiveSkillResource {
        guard let profile = profile(kind: kind, key: key) else {
            assertionFailure("Missing worked example for \(kind.rawValue):\(key)")
            return missingProfileResource(label: label, language: language)
        }
        let isEnglish = language == .english
        let markdown: String
        if isEnglish {
            markdown = """
            # \(label): worked example and critical counterexample

            ## Situation

            \(profile.situationEN)

            ## Good example

            \(profile.goodExampleEN)

            ## Expected evidence

            \(profile.evidenceEN)

            ## Critical counterexample — do not copy

            \(profile.counterexampleEN)

            ## Why it fails and how to correct it

            \(profile.correctionEN)

            ## Usage boundary

            This example demonstrates a method, not project facts or extra authority. Replace every concrete value with current evidence, preserve the Human's scope, and never report the example outcome as work performed in the current project.
            """
        } else {
            markdown = """
            # \(label)：完整示例与关键反例

            ## 场景

            \(profile.situationZH)

            ## 正确示例

            \(profile.goodExampleZH)

            ## 应有证据

            \(profile.evidenceZH)

            ## 关键反例（不要照做）

            \(profile.counterexampleZH)

            ## 为什么错，以及如何修正

            \(profile.correctionZH)

            ## 使用边界

            示例只演示方法，不代表当前项目事实，也不扩大工具或执行权限。执行时必须把具体值替换为当前证据，保持 Human 给定范围，不得把示例结果当成当前项目已经完成的工作。
            """
        }
        return .init(
            relativePath: "references/worked-examples-and-counterexamples.md",
            title: isEnglish ? "Worked example and critical counterexample" : "完整示例与关键反例",
            summary: isEnglish
                ? "An individually authored end-to-end example with expected evidence, a critical anti-pattern, and its correction."
                : "逐项编写的端到端示例，包含应有证据、关键反例及修正方式。",
            markdown: markdown.trimmingCharacters(in: .whitespacesAndNewlines)
        )
    }

    private static func missingProfileResource(
        label: String,
        language: ChatOSLanguage
    ) -> LocalAgentProgressiveSkillResource {
        let english = language == .english
        return .init(
            relativePath: "references/worked-examples-and-counterexamples.md",
            title: english ? "Missing worked example" : "缺少专属示例",
            summary: english ? "Catalog validation must reject this Skill." : "目录校验必须拒绝此 Skill。",
            markdown: english
                ? "# \(label): missing worked example\n\nThis catalog entry is incomplete and must not be released."
                : "# \(label)：缺少专属示例\n\n此目录条目不完整，不得发布。"
        )
    }
}
