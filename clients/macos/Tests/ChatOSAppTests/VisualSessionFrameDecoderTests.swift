import ChatOSCore
import Foundation
import Testing
@testable import ChatOSApp

@Suite("Visual session frame decoding")
struct VisualSessionFrameDecoderTests {
    @Test("frame bytes are decoded once and removed from presentation state")
    func frameBytesAreDecodedOffPresentationState() async throws {
        let frame = try #require(Data(base64Encoded:
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
        ))
        let session = makeSession(sequence: 7, frameData: frame)

        let prepared = await VisualSessionFrameDecoder.prepare([session], reusing: [:])

        let result = try #require(prepared.first)
        #expect(result.session.frameData == nil)
        #expect(result.frameImage?.image.width == 1)
        #expect(result.frameImage?.image.height == 1)
    }

    @Test("an unchanged frame reuses the already decoded image")
    func unchangedFrameReusesDecodedImage() async throws {
        let frame = try #require(Data(base64Encoded:
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
        ))
        let first = try #require(await VisualSessionFrameDecoder.prepare(
            [makeSession(sequence: 8, frameData: frame)],
            reusing: [:]
        ).first)
        let image = try #require(first.frameImage)
        let identity = VisualSessionFrameIdentity(
            adapterSessionID: first.session.adapterSessionID,
            frameSequence: first.session.frameSequence
        )

        let repeated = try #require(await VisualSessionFrameDecoder.prepare(
            [makeSession(sequence: 8, frameData: nil)],
            reusing: [identity: image]
        ).first)

        #expect(repeated.frameImage === image)
        #expect(repeated.session.frameData == nil)
    }

    private func makeSession(sequence: UInt64, frameData: Data?) -> PluginVisualSession {
        PluginVisualSession(
            id: "visual-1",
            adapterSessionID: "adapter-1",
            pluginID: "plugin-1",
            componentKey: "browser-cdp",
            pluginDisplayName: "Browser",
            title: "Browser activity",
            frameSequence: sequence,
            frameData: frameData,
            mimeType: "image/png",
            owner: .init(conversationID: "conversation-1")
        )
    }
}
