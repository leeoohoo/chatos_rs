import SwiftUI

private struct InterfaceFontScaleKey: EnvironmentKey {
    static let defaultValue: CGFloat = 1
}

extension EnvironmentValues {
    var interfaceFontScale: CGFloat {
        get { self[InterfaceFontScaleKey.self] }
        set { self[InterfaceFontScaleKey.self] = newValue }
    }
}

struct AppFontSpec {
    fileprivate enum Base {
        case semantic(Font.TextStyle)
        case fixed(CGFloat)
    }

    fileprivate let base: Base
    fileprivate var fontWeight: Font.Weight?
    fileprivate var fontDesign: Font.Design
    fileprivate var usesMonospacedDigits: Bool

    static let largeTitle = semantic(.largeTitle)
    static let title = semantic(.title)
    static let title2 = semantic(.title2)
    static let title3 = semantic(.title3)
    static let headline = semantic(.headline, weight: .medium)
    static let subheadline = semantic(.subheadline)
    static let body = semantic(.body)
    static let callout = semantic(.callout)
    static let footnote = semantic(.footnote)
    static let caption = semantic(.caption)
    static let caption2 = semantic(.caption2)

    static func system(
        size: CGFloat,
        weight: Font.Weight = .regular,
        design: Font.Design = .default
    ) -> AppFontSpec {
        AppFontSpec(
            base: .fixed(size),
            fontWeight: weight,
            fontDesign: design,
            usesMonospacedDigits: false
        )
    }

    static func system(
        _ style: Font.TextStyle,
        design: Font.Design = .default
    ) -> AppFontSpec {
        semantic(style, design: design)
    }

    func weight(_ weight: Font.Weight) -> AppFontSpec {
        var copy = self
        copy.fontWeight = weight
        return copy
    }

    func monospaced() -> AppFontSpec {
        var copy = self
        copy.fontDesign = .monospaced
        return copy
    }

    func monospacedDigit() -> AppFontSpec {
        var copy = self
        copy.usesMonospacedDigits = true
        return copy
    }

    fileprivate func resolved(scale: CGFloat) -> Font {
        let baseSize: CGFloat
        switch base {
        case let .semantic(style):
            baseSize = Self.baseSize(for: style)
        case let .fixed(size):
            baseSize = size
        }

        var font = Font.system(
            size: max(8, baseSize * scale),
            weight: fontWeight ?? .regular,
            design: fontDesign
        )
        if usesMonospacedDigits {
            font = font.monospacedDigit()
        }
        return font
    }

    private static func semantic(
        _ style: Font.TextStyle,
        weight: Font.Weight? = nil,
        design: Font.Design = .default
    ) -> AppFontSpec {
        AppFontSpec(
            base: .semantic(style),
            fontWeight: weight,
            fontDesign: design,
            usesMonospacedDigits: false
        )
    }

    private static func baseSize(for style: Font.TextStyle) -> CGFloat {
        switch style {
        case .largeTitle: 26
        case .title: 22
        case .title2: 20
        case .title3: 17
        case .headline: 14
        case .subheadline: 13
        case .body: 14
        case .callout: 13
        case .footnote: 12
        case .caption: 12
        case .caption2: 11
        @unknown default: 14
        }
    }
}

private struct AppFontModifier: ViewModifier {
    @Environment(\.interfaceFontScale) private var scale
    let specification: AppFontSpec

    func body(content: Content) -> some View {
        content.font(specification.resolved(scale: scale))
    }
}

extension View {
    func appFont(_ specification: AppFontSpec) -> some View {
        modifier(AppFontModifier(specification: specification))
    }
}

extension AppModel {
    var interfaceFontScale: CGFloat {
        CGFloat(interfaceFontSize / 14)
    }
}
