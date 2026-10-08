@testable import ChatOSApp
import XCTest

final class ConversationTimelineBottomSyncPolicyTests: XCTestCase {
    func testPublishesWhenDetectedPositionChanges() {
        XCTAssertTrue(
            ConversationTimelineBottomSyncPolicy.shouldPublish(
                currentIsPinned: true,
                detectedIsPinned: false,
                unreadNewerCount: 0
            )
        )
        XCTAssertTrue(
            ConversationTimelineBottomSyncPolicy.shouldPublish(
                currentIsPinned: false,
                detectedIsPinned: true,
                unreadNewerCount: 0
            )
        )
    }

    func testRepublishesPinnedStateToClearUnreadContent() {
        XCTAssertTrue(
            ConversationTimelineBottomSyncPolicy.shouldPublish(
                currentIsPinned: true,
                detectedIsPinned: true,
                unreadNewerCount: 1
            )
        )
    }

    func testDoesNotRepublishStableStateWithoutUnreadContent() {
        XCTAssertFalse(
            ConversationTimelineBottomSyncPolicy.shouldPublish(
                currentIsPinned: true,
                detectedIsPinned: true,
                unreadNewerCount: 0
            )
        )
        XCTAssertFalse(
            ConversationTimelineBottomSyncPolicy.shouldPublish(
                currentIsPinned: false,
                detectedIsPinned: false,
                unreadNewerCount: 1
            )
        )
    }
}
