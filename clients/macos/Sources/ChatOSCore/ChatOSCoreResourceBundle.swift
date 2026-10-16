import Foundation

enum ChatOSCoreResourceBundle {
    private final class BundleAnchor {}

    static let bundle: Bundle = {
        #if DEBUG
        // SwiftPM owns the layout while running tests and local debug builds.
        return Bundle.module
        #else
        let bundleName = "ChatOSSwift_ChatOSCore.bundle"
        let containers = [Bundle.main, Bundle(for: BundleAnchor.self)]
        for container in containers {
            guard let resourceURL = container.resourceURL else { continue }
            let bundleURL = resourceURL.appendingPathComponent(bundleName, isDirectory: true)
            if let bundle = Bundle(url: bundleURL) {
                return bundle
            }
        }
        fatalError("Unable to locate packaged ChatOSCore resources")
        #endif
    }()
}
