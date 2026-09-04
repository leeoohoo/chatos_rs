import Foundation
import Testing
@testable import ChatOSApp

@MainActor
struct PetDefaultFileHandlerPromptControllerTests {
    @Test("offers to make an explicitly opened supported type the default")
    func offersForSupportedNonDefaultFile() {
        var requests: [(String, String)] = []
        let controller = makeController(
            setter: { contentType, bundleIdentifier in
                requests.append((contentType, bundleIdentifier))
                return noErr
            }
        )

        controller.offerAfterOpening([URL(fileURLWithPath: "/tmp/example.jsx")])
        #expect(controller.prompt?.fileExtension == "jsx")
        #expect(requests.isEmpty)
        controller.makeDefault()

        #expect(requests.count == 1)
        #expect(requests[0].0 == "dyn.test.jsx")
        #expect(requests[0].1 == "com.chatos.swift-client")
    }

    @Test("dismissing leaves the existing default unchanged")
    func dismissDoesNotSetDefault() {
        var setterWasCalled = false
        let controller = makeController(
            setter: { _, _ in
                setterWasCalled = true
                return noErr
            }
        )

        controller.offerAfterOpening([URL(fileURLWithPath: "/tmp/example.jsx")])
        controller.dismiss()

        #expect(!setterWasCalled)
        #expect(controller.prompt == nil)
    }

    @Test("does not ask when ChatOS is already the default")
    func skipsCurrentDefault() {
        let controller = makeController(
            defaultApplication: { _ in "com.chatos.swift-client" }
        )

        controller.offerAfterOpening([URL(fileURLWithPath: "/tmp/example.jsx")])

        #expect(controller.prompt == nil)
    }

    @Test("does not ask for unsupported files")
    func skipsUnsupportedFile() {
        let controller = makeController(
            supported: { _, _ in false }
        )

        controller.offerAfterOpening([URL(fileURLWithPath: "/tmp/example.bin")])

        #expect(controller.prompt == nil)
    }

    private func makeController(
        defaultApplication: @escaping (URL) -> String? = { _ in "com.cursor" },
        supported: @escaping (URL, String) -> Bool = { _, _ in true },
        setter: @escaping (String, String) -> OSStatus = { _, _ in noErr }
    ) -> PetDefaultFileHandlerPromptController {
        PetDefaultFileHandlerPromptController(
            bundleIdentifier: "com.chatos.swift-client",
            contentTypeProvider: { _ in "dyn.test.jsx" },
            defaultApplicationProvider: defaultApplication,
            supportedFileProvider: supported,
            defaultHandlerSetter: setter
        )
    }
}
