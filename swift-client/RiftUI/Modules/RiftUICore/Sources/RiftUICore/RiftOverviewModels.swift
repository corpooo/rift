import Foundation

public struct RiftOverviewSnapshot: Codable, Equatable, Sendable {
    public var generatedAt: Date
    public var currentDisplayName: String
    public var workspaces: [RiftWorkspaceSnapshot]

    public init(
        generatedAt: Date,
        currentDisplayName: String,
        workspaces: [RiftWorkspaceSnapshot]
    ) {
        self.generatedAt = generatedAt
        self.currentDisplayName = currentDisplayName
        self.workspaces = workspaces
    }

    public static let empty = RiftOverviewSnapshot(
        generatedAt: .now,
        currentDisplayName: "MacBook Pro",
        workspaces: []
    )
}

public struct RiftWorkspaceSnapshot: Identifiable, Codable, Equatable, Sendable {
    public var id: String
    public var index: Int
    public var name: String
    public var subtitle: String
    public var isActive: Bool
    public var accentHex: String
    public var windows: [RiftWindowSnapshot]

    public init(
        id: String,
        index: Int,
        name: String,
        subtitle: String,
        isActive: Bool,
        accentHex: String,
        windows: [RiftWindowSnapshot]
    ) {
        self.id = id
        self.index = index
        self.name = name
        self.subtitle = subtitle
        self.isActive = isActive
        self.accentHex = accentHex
        self.windows = windows
    }
}

public struct RiftWindowSnapshot: Identifiable, Codable, Equatable, Sendable {
    public var id: String
    public var appName: String
    public var title: String
    public var subtitle: String
    public var role: String
    public var tintHex: String
    public var preview: RiftWindowPreviewStyle

    public init(
        id: String,
        appName: String,
        title: String,
        subtitle: String,
        role: String,
        tintHex: String,
        preview: RiftWindowPreviewStyle
    ) {
        self.id = id
        self.appName = appName
        self.title = title
        self.subtitle = subtitle
        self.role = role
        self.tintHex = tintHex
        self.preview = preview
    }
}

public struct RiftWindowPreviewStyle: Codable, Equatable, Sendable {
    public var badge: String
    public var gradientStartHex: String
    public var gradientEndHex: String
    public var aspectRatio: Double

    public init(
        badge: String,
        gradientStartHex: String,
        gradientEndHex: String,
        aspectRatio: Double
    ) {
        self.badge = badge
        self.gradientStartHex = gradientStartHex
        self.gradientEndHex = gradientEndHex
        self.aspectRatio = aspectRatio
    }
}
