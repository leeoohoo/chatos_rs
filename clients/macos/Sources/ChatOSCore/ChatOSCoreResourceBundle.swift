import Foundation
import ChatOSModelCapabilities

enum ChatOSCoreResourceBundle {
    private final class BundleAnchor {}

    static let bundle: Bundle = {
        #if DEBUG
        // SwiftPM owns the layout while running tests and local debug builds.
        return Bundle.module
        #else
        let bundleName = "ChatOSSwift_ChatOSCore.bundle"
        let containers = [Bundle.main, Bundle(for: BundleAnchor.self)]
        if let bundle = ChatOSPackageResourceBundleLocator.locate(named: bundleName, in: containers) {
            return bundle
        }
        fatalError("Unable to locate packaged ChatOSCore resources")
        #endif
    }()
}
