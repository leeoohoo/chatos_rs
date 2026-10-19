import ChatOSCore
import Testing

struct AskUserPromptFormStateTests {
    @Test
    func initializesFieldValuesAndDefaultSelection() {
        let prompt = prompt(
            fields: [
                AskUserField(key: "name", label: "Name", defaultValue: "ChatOS"),
            ],
            choice: choice(
                allowsMultiple: true,
                defaults: ["b"]
            )
        )

        let state = AskUserPromptFormState(prompt: prompt)

        #expect(state.values == ["name": "ChatOS"])
        #expect(state.selection == ["b"])
    }

    @Test
    func requiredValuesAreValidatedAfterTrimmingWhitespace() {
        let prompt = prompt(fields: [
            AskUserField(key: "required", label: "Required", isRequired: true),
        ])
        var state = AskUserPromptFormState(prompt: prompt)

        state.values["required"] = "  \n "
        #expect(!state.isValid(for: prompt))

        state.values["required"] = " value "
        #expect(state.isValid(for: prompt))
    }

    @Test
    func choiceCountMustStayWithinMinimumAndMaximum() {
        let prompt = prompt(choice: choice(
            allowsMultiple: true,
            minimum: 1,
            maximum: 2
        ))
        var state = AskUserPromptFormState(prompt: prompt)

        #expect(!state.isValid(for: prompt))
        state.selection = ["a"]
        #expect(state.isValid(for: prompt))
        state.selection = ["a", "b", "c"]
        #expect(!state.isValid(for: prompt))
    }

    @Test
    func multiChoiceToggleHonorsMaximumAndAllowsRemoval() {
        let selection = choice(
            allowsMultiple: true,
            maximum: 2
        )
        var state = AskUserPromptFormState(prompt: prompt(choice: selection))

        state.toggleSelection("a", in: selection)
        state.toggleSelection("b", in: selection)
        state.toggleSelection("c", in: selection)
        #expect(state.selection == ["a", "b"])

        state.toggleSelection("a", in: selection)
        state.toggleSelection("c", in: selection)
        #expect(state.selection == ["b", "c"])
    }

    @Test
    func singleChoiceAlwaysReplacesCurrentSelection() {
        let selection = choice(allowsMultiple: false)
        var state = AskUserPromptFormState(prompt: prompt(choice: selection))

        state.toggleSelection("a", in: selection)
        state.toggleSelection("b", in: selection)

        #expect(state.selection == ["b"])
    }

    @Test
    func submissionKeepsFieldValuesAndOrdersChoicesByPromptOptions() {
        let prompt = prompt(
            fields: [AskUserField(key: "name", label: "Name")],
            choice: choice(allowsMultiple: true)
        )
        var state = AskUserPromptFormState(prompt: prompt)
        state.values["name"] = "ChatOS"
        state.selection = ["c", "a"]

        let submission = state.submission(for: prompt)

        #expect(submission.values == ["name": "ChatOS"])
        #expect(submission.selection == .multiple(["a", "c"]))
    }

    @Test
    func singleChoiceWithoutSelectionPreservesEmptyValueContract() {
        let prompt = prompt(choice: choice(allowsMultiple: false))
        let state = AskUserPromptFormState(prompt: prompt)

        #expect(state.submission(for: prompt).selection == .single(""))
    }

    @Test
    func resolvingPromptClearsOnlySecretValues() {
        let fields = [
            AskUserField(key: "token", label: "Token", isSecret: true),
            AskUserField(key: "note", label: "Note"),
        ]
        var state = AskUserPromptFormState(prompt: prompt(fields: fields))
        state.values = ["token": "sensitive", "note": "keep"]

        state.clearSecretValues(fields: fields)

        #expect(state.values == ["token": "", "note": "keep"])
    }

    private func prompt(
        fields: [AskUserField] = [],
        choice: AskUserChoice? = nil
    ) -> AskUserPrompt {
        AskUserPrompt(
            id: "prompt-1",
            sessionID: "session-1",
            turnID: "turn-1",
            kind: "form",
            status: .pending,
            title: "Input",
            message: "Provide input",
            allowsCancel: true,
            fields: fields,
            choice: choice
        )
    }

    private func choice(
        allowsMultiple: Bool,
        defaults: [String] = [],
        minimum: Int = 0,
        maximum: Int? = nil
    ) -> AskUserChoice {
        AskUserChoice(
            allowsMultiple: allowsMultiple,
            options: [
                AskUserChoiceOption(value: "a", label: "A"),
                AskUserChoiceOption(value: "b", label: "B"),
                AskUserChoiceOption(value: "c", label: "C"),
            ],
            defaultSelection: defaults,
            minimumSelectionCount: minimum,
            maximumSelectionCount: maximum
        )
    }
}
