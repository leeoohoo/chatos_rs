import Foundation
import Testing
@testable import ChatOSConnector

@Suite("Native system permission onboarding")
struct NativeSystemPermissionOnboardingTests {
    @Test("permission refresh uses a low-frequency timer only until granted")
    func refreshPolicyStopsAfterGrant() {
        #expect(NativeSystemPermissionRefreshPolicy.interval == 1)
        #expect(NativeSystemPermissionRefreshPolicy.shouldContinue(granted: false))
        #expect(!NativeSystemPermissionRefreshPolicy.shouldContinue(granted: true))
    }
}
