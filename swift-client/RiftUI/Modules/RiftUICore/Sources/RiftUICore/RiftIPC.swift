import Foundation

public protocol RiftOverviewProviding: Sendable {
    func fetchOverview() async throws -> RiftOverviewSnapshot
}

public enum RiftRequest: Encodable, Equatable, Sendable {
    case getOverview
    case getWorkspaces(spaceID: UInt64?)
    case getWindows(spaceID: UInt64?)
    case getDisplays
    case getConfig

    public func encode(to encoder: Encoder) throws {
        switch self {
        case .getOverview:
            var container = encoder.singleValueContainer()
            try container.encode("get_overview")
        case .getDisplays:
            var container = encoder.singleValueContainer()
            try container.encode("get_displays")
        case .getConfig:
            var container = encoder.singleValueContainer()
            try container.encode("get_config")
        case .getWorkspaces(let spaceID):
            var container = encoder.container(keyedBy: DynamicCodingKey.self)
            let key = DynamicCodingKey("get_workspaces")
            var nested = container.nestedContainer(keyedBy: DynamicCodingKey.self, forKey: key)
            try nested.encodeIfPresent(spaceID, forKey: DynamicCodingKey("space_id"))
        case .getWindows(let spaceID):
            var container = encoder.container(keyedBy: DynamicCodingKey.self)
            let key = DynamicCodingKey("get_windows")
            var nested = container.nestedContainer(keyedBy: DynamicCodingKey.self, forKey: key)
            try nested.encodeIfPresent(spaceID, forKey: DynamicCodingKey("space_id"))
        }
    }
}

public struct FallbackOverviewProvider: RiftOverviewProviding {
    private let primary: any RiftOverviewProviding
    private let fallback: any RiftOverviewProviding

    public init(
        primary: any RiftOverviewProviding,
        fallback: any RiftOverviewProviding
    ) {
        self.primary = primary
        self.fallback = fallback
    }

    public func fetchOverview() async throws -> RiftOverviewSnapshot {
        do {
            return try await primary.fetchOverview()
        } catch {
            return try await fallback.fetchOverview()
        }
    }
}

public enum RiftIPCCodec {
    public static func encode(_ request: RiftRequest) throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(request)
    }
}

public struct DynamicCodingKey: CodingKey, Hashable {
    public var stringValue: String
    public var intValue: Int?

    public init(_ stringValue: String) {
        self.stringValue = stringValue
        self.intValue = nil
    }

    public init?(stringValue: String) {
        self.init(stringValue)
    }

    public init?(intValue: Int) {
        self.stringValue = String(intValue)
        self.intValue = intValue
    }
}
