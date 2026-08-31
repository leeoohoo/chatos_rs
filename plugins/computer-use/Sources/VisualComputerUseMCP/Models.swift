import Foundation

struct PointDTO: Codable, Sendable, Equatable {
    let x: Double
    let y: Double
}

struct RectDTO: Codable, Sendable, Equatable {
    let x: Double
    let y: Double
    let width: Double
    let height: Double
}

struct DisplayDTO: Codable, Sendable, Equatable {
    let id: UInt32
    let isMain: Bool
    let frame: RectDTO
    let nativePixelWidth: Int
    let nativePixelHeight: Int
    let nativePixelsPerPointX: Double
    let nativePixelsPerPointY: Double
}

struct ObservationDTO: Codable, Sendable, Equatable {
    let coordinateSystem: String
    let activeApplication: ActiveApplicationDTO
    let globalDesktopBounds: RectDTO
    let selectedDisplay: DisplayDTO
    let displays: [DisplayDTO]
    let captureRegionGlobal: RectDTO
    let virtualCursorGlobal: PointDTO
    let cursorScreenshotPixel: PointDTO?
    let virtualCursorIsInCaptureRegion: Bool
    let cursorVisualization: String
    let screenshotPixelWidth: Int
    let screenshotPixelHeight: Int
    let globalPointsPerScreenshotPixelX: Double
    let globalPointsPerScreenshotPixelY: Double
    let imageFormat: String
    let encodedByteCount: Int
    let cursorMarkerIncluded: Bool
    let capturedAt: String
}

struct PermissionDTO: Codable, Sendable, Equatable {
    let screenRecording: Bool
    let accessibility: Bool
    let allGranted: Bool
    let missingPermissions: [String]
    let applicationName: String
    let bundleIdentifier: String?
    let executable: String
    let authorizationTarget: String
    let runningFromAppBundle: Bool
    let onboardingPresented: Bool
    let restartMayBeRequired: Bool
    let permissions: [PermissionItemDTO]
    let guidance: [String]
}

struct PermissionItemDTO: Codable, Sendable, Equatable {
    let kind: String
    let title: String
    let granted: Bool
    let purpose: String
    let systemSettingsTitle: String
    let settingsURL: String
    let nextStep: String
}

struct ActiveApplicationDTO: Codable, Sendable, Equatable {
    let name: String?
    let bundleIdentifier: String?
    let processIdentifier: Int32?
}

struct ActionReceiptDTO: Codable, Sendable, Equatable {
    let action: String
    let screenshotReturned: Bool
    let virtualCursorGlobal: PointDTO
    let activeApplication: ActiveApplicationDTO
}

struct ShortcutDefinition: Codable, Sendable, Equatable {
    let id: String
    let title: String
    let keys: [String]
    let description: String?
}

struct ShortcutListDTO: Codable, Sendable, Equatable {
    let application: ActiveApplicationDTO
    let shortcuts: [ShortcutDefinition]
    let source: String
}

enum VisualComputerUseError: LocalizedError {
    case invalidArgument(String)
    case displayNotFound(UInt32)
    case pointOutsideDisplays(Double, Double)
    case screenCapturePermissionRequired
    case screenCaptureFailed(UInt32)
    case imageEncodingFailed
    case invalidEncodedImage
    case eventCreationFailed(String)
    case accessibilityPermissionRequired
    case unsupportedKey(String)
    case invalidShortcut(String)
    case applicationNotFound(String)
    case applicationActivationFailed(String)

    var errorDescription: String? {
        switch self {
        case .invalidArgument(let message):
            return message
        case .displayNotFound(let id):
            return "No active display has id \(id)."
        case .pointOutsideDisplays(let x, let y):
            return "Point (\(x), \(y)) is outside every active display."
        case .screenCapturePermissionRequired:
            return "Screen Recording permission is required. Call request_permissions to open the macOS guide, enable \(PermissionSupport.authorizationTargetURL.path) in System Settings > Privacy & Security > Screen & System Audio Recording, then reconnect the MCP."
        case .screenCaptureFailed(let id):
            return "Could not capture display \(id)."
        case .imageEncodingFailed:
            return "Could not render the captured image."
        case .invalidEncodedImage:
            return "The screenshot encoder could not produce a structurally valid image after recovery attempts. No image was returned."
        case .eventCreationFailed(let kind):
            return "Could not create a CoreGraphics \(kind) event."
        case .accessibilityPermissionRequired:
            return "Accessibility permission is required for real mouse and keyboard events. Call request_permissions to open the macOS guide, then enable \(PermissionSupport.authorizationTargetURL.path) in System Settings > Privacy & Security > Accessibility."
        case .unsupportedKey(let key):
            return "Unsupported key '\(key)'. Use a named key or a single US keyboard character."
        case .invalidShortcut(let message):
            return message
        case .applicationNotFound(let bundleIdentifier):
            return "No installed application has bundle identifier '\(bundleIdentifier)'."
        case .applicationActivationFailed(let bundleIdentifier):
            return "Could not launch or activate application '\(bundleIdentifier)'."
        }
    }
}
