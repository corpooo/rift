import AppKit
import SwiftUI

final class RiftUIApplicationDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }
}

@main
struct RiftUIApp: App {
    @NSApplicationDelegateAdaptor(RiftUIApplicationDelegate.self)
    private var appDelegate

    @StateObject
    private var store = OverviewStore()

    var body: some Scene {
        Window("Rift Overview", id: "overview") {
            RiftUIRootView(store: store)
                .background(OverviewWindowAccessor())
        }
        .windowStyle(.hiddenTitleBar)
        .windowResizability(.contentSize)
        .defaultSize(width: 1440, height: 900)
        .commands {
            CommandGroup(replacing: .newItem) {}
        }
    }
}
