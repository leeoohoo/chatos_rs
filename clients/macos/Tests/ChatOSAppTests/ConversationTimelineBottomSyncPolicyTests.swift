import ChatOSCore
@testable import ChatOSApp
import XCTest

final class ConversationTimelineBottomSyncPolicyTests: XCTestCase {
    func testPublishesWhenDetectedPositionChanges() {
        XCTAssertTrue(
            ConversationViewportPolicy.shouldPublishPinnedState(
                currentIsPinned: true,
                detectedIsPinned: false,
                unreadNewerCount: 0
            )
        )
        XCTAssertTrue(
            ConversationViewportPolicy.shouldPublishPinnedState(
                currentIsPinned: false,
                detectedIsPinned: true,
                unreadNewerCount: 0
            )
        )
    }

    func testRepublishesPinnedStateToClearUnreadContent() {
        XCTAssertTrue(
            ConversationViewportPolicy.shouldPublishPinnedState(
                currentIsPinned: true,
                detectedIsPinned: true,
                unreadNewerCount: 1
            )
        )
    }

    func testDoesNotRepublishStableStateWithoutUnreadContent() {
        XCTAssertFalse(
            ConversationViewportPolicy.shouldPublishPinnedState(
                currentIsPinned: true,
                detectedIsPinned: true,
                unreadNewerCount: 0
            )
        )
        XCTAssertFalse(
            ConversationViewportPolicy.shouldPublishPinnedState(
                currentIsPinned: false,
                detectedIsPinned: false,
                unreadNewerCount: 1
            )
        )
    }
}
