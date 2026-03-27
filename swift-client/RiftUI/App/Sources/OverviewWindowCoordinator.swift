import AppKit
import SwiftUI

struct OverviewWindowAccessor: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView(frame: .zero)
        DispatchQueue.main.async {
            if let window = view.window {
                OverviewWindowCoordinator.shared.configure(window: window)
            }
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        DispatchQueue.main.async {
            if let window = nsView.window {
                OverviewWindowCoordinator.shared.configure(window: window)
            }
        }
    }
}

@MainActor
final class OverviewWindowCoordinator {
    static let shared = OverviewWindowCoordinator()

    private var configuredWindowNumbers: Set<Int> = []
    private var localMonitor: Any?

    private init() {}

    func configure(window: NSWindow) {
        let windowNumber = window.windowNumber
        if !configuredWindowNumbers.contains(windowNumber) {
            configuredWindowNumbers.insert(windowNumber)

            window.titleVisibility = .hidden
            window.titlebarAppearsTransparent = true
            window.isOpaque = false
            window.backgroundColor = .clear
            window.isReleasedWhenClosed = false
            window.level = .statusBar
            window.collectionBehavior = [
                .canJoinAllSpaces,
                .fullScreenAuxiliary,
                .stationary,
                .ignoresCycle
            ]
            window.hasShadow = false
            window.isMovable = false
            window.isMovableByWindowBackground = false
            window.standardWindowButton(.closeButton)?.isHidden = true
            window.standardWindowButton(.miniaturizeButton)?.isHidden = true
            window.standardWindowButton(.zoomButton)?.isHidden = true
        }

        if let screen = bestScreen(for: window) {
            window.setFrame(screen.frame, display: true)
        }

        if localMonitor == nil {
            localMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                if event.keyCode == 53 {
                    NSApp.terminate(nil)
                    return nil
                }
                return event
            }
        }
    }

    private func bestScreen(for window: NSWindow) -> NSScreen? {
        let mouseLocation = NSEvent.mouseLocation
        return NSScreen.screens.first(where: { $0.frame.contains(mouseLocation) }) ?? window.screen ?? NSScreen.main
    }
}
