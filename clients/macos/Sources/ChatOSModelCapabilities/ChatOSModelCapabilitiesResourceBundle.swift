import Foundation

enum ChatOSModelCapabilitiesResourceBundle {
    private final class BundleAnchor {}

    static let bundle: Bundle = {
        #if DEBUG
        // SwiftPM owns the layout while running tests and local debug builds.
        return Bundle.module
        #else
        let bundleName = "ChatOSSwift_ChatOSModelCapabilities.bundle"
        let containers = [Bundle.main, Bundle(for: BundleAnchor.self)]
        if let bundle = ChatOSPackageResourceBundleLocator.locate(named: bundleName, in: containers) {
            return bundle
        }
        fatalError("Unable to locate packaged model capability resources")
        #endif
    }()
}
