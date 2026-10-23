import Foundation

enum ChatOSModelCapabilitiesResourceBundle {
    private final class BundleAnchor {}

    static let bundle: Bundle = {
        let bundleName = "ChatOSSwift_ChatOSModelCapabilities.bundle"
        let containers = [Bundle.main, Bundle(for: BundleAnchor.self)]
        if let bundle = ChatOSPackageResourceBundleLocator.locate(named: bundleName, in: containers) {
            return bundle
        }

        #if DEBUG
        // An unpackaged SwiftPM executable keeps resource bundles in its products
        // directory. Packaged apps, including diagnostic Debug apps, must use the
        // sealed Contents/Resources copy above and never a build-machine path.
        return Bundle.module
        #else
        fatalError("Unable to locate packaged model capability resources")
        #endif
    }()
}
