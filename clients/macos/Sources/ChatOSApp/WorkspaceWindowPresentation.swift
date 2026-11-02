import AppKit

/// Reopening a workspace must preserve its Space and its active modal chain.
/// Floating utility panels are not eligible workspace presentation targets.
@MainActor
enum WorkspaceWindowPresentation {
    static func primaryBehavior(isFullScreen: Bool) -> NSWindow.CollectionBehavior {
        isFullScreen ? [.fullScreenPrimary] : [.fullScreenPrimary, .moveToActiveSpace]
    }

    static func focusTarget(for window: NSWindow) -> NSWindow {
        var target = window
        while let sheet = target.attachedSheet { target = sheet }
        return target
    }

    static func show(_ window: NSWindow) {
        let isFullScreen = window.styleMask.contains(.fullScreen)
        window.collectionBehavior.subtract([.moveToActiveSpace, .canJoinAllSpaces, .fullScreenAuxiliary])
        window.collectionBehavior.formUnion(primaryBehavior(isFullScreen: isFullScreen))
        if window.isMiniaturized { window.deminiaturize(nil) }
        NSApp.activate(ignoringOtherApps: true)
        // Ordering the parent key while a sheet is active can select a different
        // window set. Let AppKit keep the sheet attached and focus that sheet.
        if window.attachedSheet == nil { window.makeKeyAndOrderFront(nil) }
        else { focusTarget(for: window).makeKeyAndOrderFront(nil) }
    }
}
