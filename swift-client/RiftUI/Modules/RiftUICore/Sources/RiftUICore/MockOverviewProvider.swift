import Foundation

public struct MockOverviewProvider: RiftOverviewProviding {
    public init() {}

    public func fetchOverview() async throws -> RiftOverviewSnapshot {
        RiftOverviewSnapshot(
            generatedAt: .now,
            currentDisplayName: "Studio Display",
            workspaces: [
                WorkspaceFactory.make(
                    id: "workspace-01",
                    index: 1,
                    name: "Build",
                    subtitle: "Compiler output, logs, and project health",
                    isActive: false,
                    accentHex: "5BC0EB",
                    windows: [
                        WindowFactory.make(
                            id: "xcode",
                            app: "Xcode",
                            title: "rift-swift-client.xcodeproj",
                            subtitle: "Overlay app target",
                            role: "editor",
                            tintHex: "5BC0EB",
                            badge: "compile",
                            gradientStartHex: "1A3C7A",
                            gradientEndHex: "07142C",
                            aspectRatio: 1.48
                        ),
                        WindowFactory.make(
                            id: "terminal-build",
                            app: "Terminal",
                            title: "swift build --product RiftUI",
                            subtitle: "Build logs",
                            role: "terminal",
                            tintHex: "76E4F7",
                            badge: "shell",
                            gradientStartHex: "1D2431",
                            gradientEndHex: "06080D",
                            aspectRatio: 1.28
                        ),
                        WindowFactory.make(
                            id: "activity",
                            app: "Activity Monitor",
                            title: "Rift CPU / memory",
                            subtitle: "Performance trace",
                            role: "metrics",
                            tintHex: "A7F3D0",
                            badge: "perf",
                            gradientStartHex: "11312D",
                            gradientEndHex: "061310",
                            aspectRatio: 1.15
                        )
                    ]
                ),
                WorkspaceFactory.make(
                    id: "workspace-02",
                    index: 2,
                    name: "Code",
                    subtitle: "Primary workspace for the active development flow",
                    isActive: true,
                    accentHex: "74F0A7",
                    windows: [
                        WindowFactory.make(
                            id: "xcode-main",
                            app: "Xcode",
                            title: "Mission-control overlay prototype",
                            subtitle: "RiftUIRootView.swift",
                            role: "editor",
                            tintHex: "74F0A7",
                            badge: "active",
                            gradientStartHex: "0F2F27",
                            gradientEndHex: "08110E",
                            aspectRatio: 1.42
                        ),
                        WindowFactory.make(
                            id: "ghostty",
                            app: "Ghostty",
                            title: "cargo run --bin rift",
                            subtitle: "daemon console",
                            role: "terminal",
                            tintHex: "B7FFCB",
                            badge: "daemon",
                            gradientStartHex: "121821",
                            gradientEndHex: "05070A",
                            aspectRatio: 1.24
                        ),
                        WindowFactory.make(
                            id: "docs",
                            app: "Safari",
                            title: "Rift wiki / config reference",
                            subtitle: "tiling rules and commands",
                            role: "reference",
                            tintHex: "A6E3A1",
                            badge: "docs",
                            gradientStartHex: "1A3349",
                            gradientEndHex: "0B1320",
                            aspectRatio: 1.38
                        ),
                        WindowFactory.make(
                            id: "figma",
                            app: "Figma",
                            title: "Workspace overview explorations",
                            subtitle: "row-based composition",
                            role: "design",
                            tintHex: "F7B7FF",
                            badge: "layout",
                            gradientStartHex: "4A2050",
                            gradientEndHex: "190A1B",
                            aspectRatio: 1.10
                        )
                    ]
                ),
                WorkspaceFactory.make(
                    id: "workspace-03",
                    index: 3,
                    name: "Review",
                    subtitle: "Pull requests, issue triage, and release notes",
                    isActive: false,
                    accentHex: "FFB86B",
                    windows: [
                        WindowFactory.make(
                            id: "github",
                            app: "Arc",
                            title: "acsandmann/rift pull requests",
                            subtitle: "review queue",
                            role: "browser",
                            tintHex: "FFB86B",
                            badge: "review",
                            gradientStartHex: "5B3415",
                            gradientEndHex: "1A1008",
                            aspectRatio: 1.33
                        ),
                        WindowFactory.make(
                            id: "notion",
                            app: "Notion",
                            title: "Roadmap / release notes",
                            subtitle: "feature notes",
                            role: "notes",
                            tintHex: "FFD3A1",
                            badge: "notes",
                            gradientStartHex: "3B2A20",
                            gradientEndHex: "120B08",
                            aspectRatio: 1.20
                        )
                    ]
                ),
                WorkspaceFactory.make(
                    id: "workspace-04",
                    index: 4,
                    name: "Comms",
                    subtitle: "Chat, tickets, and ambient utilities",
                    isActive: false,
                    accentHex: "D8B4FE",
                    windows: [
                        WindowFactory.make(
                            id: "slack",
                            app: "Slack",
                            title: "#rift / design discussion",
                            subtitle: "Matrix bridge + team chat",
                            role: "chat",
                            tintHex: "D8B4FE",
                            badge: "chat",
                            gradientStartHex: "30143F",
                            gradientEndHex: "110817",
                            aspectRatio: 1.16
                        ),
                        WindowFactory.make(
                            id: "mail",
                            app: "Mail",
                            title: "Beta feedback",
                            subtitle: "user reports",
                            role: "inbox",
                            tintHex: "A5B4FC",
                            badge: "inbox",
                            gradientStartHex: "1E2D55",
                            gradientEndHex: "0B0F1A",
                            aspectRatio: 1.34
                        ),
                        WindowFactory.make(
                            id: "music",
                            app: "Music",
                            title: "Focus mix",
                            subtitle: "ambient playback",
                            role: "media",
                            tintHex: "F9A8D4",
                            badge: "now playing",
                            gradientStartHex: "4A1731",
                            gradientEndHex: "160812",
                            aspectRatio: 1.02
                        )
                    ]
                )
            ]
        )
    }
}

private enum WorkspaceFactory {
    static func make(
        id: String,
        index: Int,
        name: String,
        subtitle: String,
        isActive: Bool,
        accentHex: String,
        windows: [RiftWindowSnapshot]
    ) -> RiftWorkspaceSnapshot {
        RiftWorkspaceSnapshot(
            id: id,
            index: index,
            name: name,
            subtitle: subtitle,
            isActive: isActive,
            accentHex: accentHex,
            windows: windows
        )
    }
}

private enum WindowFactory {
    static func make(
        id: String,
        app: String,
        title: String,
        subtitle: String,
        role: String,
        tintHex: String,
        badge: String,
        gradientStartHex: String,
        gradientEndHex: String,
        aspectRatio: Double
    ) -> RiftWindowSnapshot {
        RiftWindowSnapshot(
            id: id,
            appName: app,
            title: title,
            subtitle: subtitle,
            role: role,
            tintHex: tintHex,
            preview: RiftWindowPreviewStyle(
                badge: badge,
                gradientStartHex: gradientStartHex,
                gradientEndHex: gradientEndHex,
                aspectRatio: aspectRatio
            )
        )
    }
}
