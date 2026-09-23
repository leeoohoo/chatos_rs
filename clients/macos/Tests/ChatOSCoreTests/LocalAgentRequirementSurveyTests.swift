@testable import ChatOSCore
import XCTest

final class LocalAgentRequirementSurveyTests: XCTestCase {
    private let rankingQuestion = LocalAgentRequirementSurveyQuestion(
        id: "priority",
        prompt: "请排列优先级",
        kind: .ranking,
        options: [
            .init(id: "quality", label: "质量"),
            .init(id: "speed", label: "速度"),
            .init(id: "cost", label: "成本"),
        ]
    )

    func testRankingAnswerPreservesAndValidatesCompleteOrder() throws {
        let submission = LocalAgentRequirementSurveySubmission(answers: [
            .init(
                questionID: rankingQuestion.id,
                selectedOptionIDs: ["speed", "quality", "cost"]
            ),
        ])

        try LocalAgentRequirementSurvey.validate(
            submission: submission,
            questions: [rankingQuestion]
        )
        XCTAssertEqual(
            submission.answers.first?.selectedOptionIDs,
            ["speed", "quality", "cost"]
        )
    }

    func testRankingAnswerRejectsPartialOrder() {
        let submission = LocalAgentRequirementSurveySubmission(answers: [
            .init(questionID: rankingQuestion.id, selectedOptionIDs: ["speed", "quality"]),
        ])

        XCTAssertThrowsError(try LocalAgentRequirementSurvey.validate(
            submission: submission,
            questions: [rankingQuestion]
        ))
    }

    func testRankingQuestionLimitsOptionCountToTen() {
        let question = LocalAgentRequirementSurveyQuestion(
            id: "priority",
            prompt: "请排列优先级",
            kind: .ranking,
            options: (1...11).map { .init(id: "option-\($0)", label: "选项 \($0)") }
        )

        XCTAssertThrowsError(try question.validate())
    }
}
