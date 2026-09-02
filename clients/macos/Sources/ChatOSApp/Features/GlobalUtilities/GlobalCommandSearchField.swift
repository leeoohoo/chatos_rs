import AppKit
import SwiftUI

@MainActor
struct GlobalCommandSearchField: NSViewRepresentable {
    static let focusIdentifier = NSUserInterfaceItemIdentifier("ChatOS.GlobalCommandSearchField")

    @Binding var text: String
    let placeholder: String
    let fontSize: CGFloat
    let onMove: (MoveCommandDirection) -> Void
    let onSubmit: () -> Void
    let onCancel: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField()
        field.identifier = Self.focusIdentifier
        field.delegate = context.coordinator
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.usesSingleLineMode = true
        field.lineBreakMode = .byTruncatingTail
        field.font = .systemFont(ofSize: fontSize, weight: .medium)
        field.placeholderString = placeholder
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        Task { @MainActor [weak field] in
            field?.window?.makeFirstResponder(field)
        }
        return field
    }

    func updateNSView(_ field: NSTextField, context: Context) {
        context.coordinator.parent = self
        field.placeholderString = placeholder
        field.font = .systemFont(ofSize: fontSize, weight: .medium)
        if field.stringValue != text {
            field.stringValue = text
        }
        if field.window?.firstResponder == nil {
            field.window?.makeFirstResponder(field)
        }
    }

    @MainActor
    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: GlobalCommandSearchField

        init(parent: GlobalCommandSearchField) {
            self.parent = parent
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField else { return }
            parent.text = field.stringValue
        }

        func control(
            _ control: NSControl,
            textView: NSTextView,
            doCommandBy commandSelector: Selector
        ) -> Bool {
            switch commandSelector {
            case #selector(NSResponder.moveUp(_:)):
                parent.onMove(.up)
            case #selector(NSResponder.moveDown(_:)):
                parent.onMove(.down)
            case #selector(NSResponder.insertNewline(_:)),
                 #selector(NSResponder.insertNewlineIgnoringFieldEditor(_:)):
                parent.onSubmit()
            case #selector(NSResponder.cancelOperation(_:)):
                parent.onCancel()
            default:
                return false
            }
            return true
        }
    }
}
