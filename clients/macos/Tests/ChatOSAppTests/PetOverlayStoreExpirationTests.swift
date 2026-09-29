import ChatOSCore
import Foundation
import Testing
@testable import ChatOSApp

@Suite("Pet overlay expiration scheduling")
struct PetOverlayStoreExpirationTests {
    @Test("only the earliest future expiration is scheduled")
    func nextExpirationIgnoresPersistentAndExpiredActivities() throws {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        let future = now.addingTimeInterval(5)
        let later = now.addingTimeInterval(10)
        let activities = [
            makeActivity(id: "persistent", expiresAt: nil),
            makeActivity(id: "expired", expiresAt: now.addingTimeInterval(-1)),
            makeActivity(id: "later", expiresAt: later),
            makeActivity(id: "future", expiresAt: future),
        ]

        #expect(PetExpirationSchedulingPolicy.nextExpiration(
            in: activities,
            after: now
        ) == future)
    }

    @Test("an earlier activity reschedules expiration without a polling loop")
    @MainActor
    func earlierActivityReschedulesExpiration() async throws {
        let store = PetOverlayStore()
        store.startExpirationMonitoring()
        let later = makeActivity(
            id: "later",
            expiresAt: Date().addingTimeInterval(5)
        )
        let earlier = makeActivity(
            id: "earlier",
            expiresAt: Date().addingTimeInterval(0.05)
        )
        store.apply(.upsert(later))
        store.apply(.upsert(earlier))

        for _ in 0..<100 where store.activities.contains(where: { $0.id == earlier.id }) {
            try await Task.sleep(for: .milliseconds(20))
        }

        #expect(!store.activities.contains(where: { $0.id == earlier.id }))
        #expect(store.activities.contains(where: { $0.id == later.id }))
    }

    private func makeActivity(id: String, expiresAt: Date?) -> PetActivity {
        PetActivity(
            id: id,
            source: .chat,
            kind: .succeeded,
            title: id,
            expiresAt: expiresAt
        )
    }
}
