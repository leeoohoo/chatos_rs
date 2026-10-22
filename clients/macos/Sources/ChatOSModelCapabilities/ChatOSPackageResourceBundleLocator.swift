import Foundation

/// Release apps use sealed Contents/Resources. SwiftPM XCTest bundles instead
/// sit beside their resource bundles in the products directory. Never fall back
/// to an embedded build-machine path or a sibling of a packaged application.
package enum ChatOSPackageResourceBundleLocator {
    package static func locate(named name: String, in containers: [Bundle]) -> Bundle? {
        for container in containers {
            if let resources = container.resourceURL,
               let bundle = Bundle(url: resources.appendingPathComponent(name, isDirectory: true)) {
                return bundle
            }
        }
        for container in containers where container.bundleURL.pathExtension == "xctest" {
            let sibling = container.bundleURL.deletingLastPathComponent()
                .appendingPathComponent(name, isDirectory: true)
            if let bundle = Bundle(url: sibling) {
                return bundle
            }
        }
        return nil
    }
}
