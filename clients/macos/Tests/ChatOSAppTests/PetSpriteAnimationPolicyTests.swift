import ChatOSCore
import Testing
@testable import ChatOSApp

@Suite("Pet sprite animation policy")
struct PetSpriteAnimationPolicyTests {
    @Test("sprite atlas accepts only the bounded production dimensions")
    func validatesAtlasDimensions() {
        #expect(PetSpriteAtlasPolicy.hasExpectedDimensions(width: 1_536, height: 2_288))
        #expect(!PetSpriteAtlasPolicy.hasExpectedDimensions(width: 3_072, height: 4_576))
        #expect(PetSpriteAtlasPolicy.sourcePixelCount == 1_536 * 2_288)
        #expect(PetSpriteAtlasPolicy.maximumBytes == 20 * 1_024 * 1_024)
    }

    @Test("idle animation is capped at one frame per second")
    func idleAnimationIsLowFrequency() {
        let configuration = PetSpriteAnimationPolicy.configuration(
            animationState: .idle,
            isDragging: false,
            dragDirection: .right,
            isAnimationActive: true,
            reduceMotion: false
        )

        #expect(configuration.row == 0)
        #expect(configuration.frameCount == 7)
        #expect(configuration.frameDuration == 1.0)
        #expect(configuration.shouldAnimate)
    }

    @Test("active work remains smoother than persistent attention states")
    func activeWorkUsesHigherFrequency() {
        let running = PetSpriteAnimationPolicy.configuration(
            animationState: .running,
            isDragging: false,
            dragDirection: .right,
            isAnimationActive: true,
            reduceMotion: false
        )
        let waiting = PetSpriteAnimationPolicy.configuration(
            animationState: .waiting,
            isDragging: false,
            dragDirection: .right,
            isAnimationActive: true,
            reduceMotion: false
        )

        #expect(running.frameDuration == 0.20)
        #expect(waiting.frameDuration == 0.50)
        #expect(running.frameDuration < waiting.frameDuration)
    }

    @Test("inactive and reduced-motion pets use a static frame")
    func inactivePetUsesStaticFrame() {
        let inactive = PetSpriteAnimationPolicy.configuration(
            animationState: .running,
            isDragging: false,
            dragDirection: .left,
            isAnimationActive: false,
            reduceMotion: false
        )
        let reducedMotion = PetSpriteAnimationPolicy.configuration(
            animationState: .running,
            isDragging: false,
            dragDirection: .left,
            isAnimationActive: true,
            reduceMotion: true
        )

        #expect(!inactive.shouldAnimate)
        #expect(!reducedMotion.shouldAnimate)
    }

    @Test("drag direction selects the directional gait")
    func dragDirectionSelectsGait() {
        let left = PetSpriteAnimationPolicy.configuration(
            animationState: .idle,
            isDragging: true,
            dragDirection: .left,
            isAnimationActive: true,
            reduceMotion: false
        )
        let right = PetSpriteAnimationPolicy.configuration(
            animationState: .idle,
            isDragging: true,
            dragDirection: .right,
            isAnimationActive: true,
            reduceMotion: false
        )

        #expect(left.row == 2)
        #expect(right.row == 1)
        #expect(left.frameDuration == 0.10)
        #expect(right.frameDuration == 0.10)
    }
}
