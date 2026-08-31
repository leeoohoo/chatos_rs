import AppKit
import SwiftUI

enum AppPalette {
    static let ai = Color(red: 0.43, green: 0.34, blue: 0.84)
    static let aiSoft = adaptive(
        light: NSColor(srgbRed: 0.94, green: 0.92, blue: 1.0, alpha: 1),
        dark: NSColor(srgbRed: 0.19, green: 0.16, blue: 0.29, alpha: 1)
    )
    static let selection = Color.accentColor.opacity(0.12)
    static let canvas = adaptive(
        light: NSColor(srgbRed: 0.985, green: 0.990, blue: 1.0, alpha: 1),
        dark: NSColor(srgbRed: 0.075, green: 0.080, blue: 0.095, alpha: 1)
    )
    static let surface = adaptive(
        light: .white,
        dark: NSColor(srgbRed: 0.105, green: 0.110, blue: 0.130, alpha: 1)
    )
    static let surfaceSubtle = adaptive(
        light: NSColor(srgbRed: 0.955, green: 0.975, blue: 1.0, alpha: 1),
        dark: NSColor(srgbRed: 0.135, green: 0.145, blue: 0.175, alpha: 1)
    )
    static let inputSurface = adaptive(
        light: .white,
        dark: NSColor(srgbRed: 0.125, green: 0.130, blue: 0.150, alpha: 1)
    )
    static let border = adaptive(
        light: NSColor(srgbRed: 0.78, green: 0.84, blue: 0.93, alpha: 1),
        dark: NSColor(srgbRed: 0.275, green: 0.295, blue: 0.355, alpha: 1)
    )
    static let idleControl = Color(red: 0.34, green: 0.47, blue: 0.68)
    static let terminalGreen = Color(red: 0.10, green: 0.62, blue: 0.28)
    static let terminalOrange = Color(red: 0.84, green: 0.39, blue: 0.08)

    private static func adaptive(light: NSColor, dark: NSColor) -> Color {
        Color(nsColor: NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? dark : light
        })
    }
}

struct StatusCapsule: View {
    let title: String
    let color: Color

    var body: some View {
        Text(LocalizedStringKey(title))
            .appFont(.caption.weight(.medium))
            .foregroundStyle(color)
            .padding(.horizontal, 9)
            .padding(.vertical, 4)
            .background(color.opacity(0.11), in: Capsule())
    }
}

struct SectionLabel: View {
    let title: String

    var body: some View {
        Text(LocalizedStringKey(title))
            .appFont(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
            .textCase(.uppercase)
    }
}

extension View {
    /// Keeps a top-level workspace stable while its loading, empty, error and
    /// populated states have very different intrinsic sizes.
    func workspaceFill(alignment: Alignment = .center) -> some View {
        frame(maxWidth: .infinity, maxHeight: .infinity, alignment: alignment)
    }

    func workspaceBackground() -> some View {
        background(AppPalette.canvas)
    }
}
