import Foundation

public struct RiftCLIOverviewProvider: RiftOverviewProviding {
    public init() {}

    public func fetchOverview() async throws -> RiftOverviewSnapshot {
        try await Task.detached(priority: .userInitiated) {
            let command = Self.resolveCLICommand()
            let data = try Self.run(command: command, arguments: ["query", "overview"])
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            let payload = try decoder.decode(RustOverviewPayload.self, from: data)
            return payload.makeSnapshot()
        }
        .value
    }

    private static func run(command: CLICommand, arguments: [String]) throws -> Data {
        let process = Process()
        let stdout = Pipe()
        let stderr = Pipe()

        switch command {
        case .direct(let executableURL):
            process.executableURL = executableURL
            process.arguments = arguments
        case .env(let commandName):
            process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            process.arguments = [commandName] + arguments
        }

        process.standardOutput = stdout
        process.standardError = stderr

        do {
            try process.run()
        } catch {
            throw RiftCLIProviderError.launchFailed(error.localizedDescription)
        }

        process.waitUntilExit()

        let stdoutData = stdout.fileHandleForReading.readDataToEndOfFile()
        let stderrData = stderr.fileHandleForReading.readDataToEndOfFile()

        guard process.terminationStatus == 0 else {
            let details = String(data: stderrData, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            throw RiftCLIProviderError.commandFailed(details ?? "rift-cli exited with status \(process.terminationStatus)")
        }

        return stdoutData
    }

    private static func resolveCLICommand() -> CLICommand {
        if let explicitPath = ProcessInfo.processInfo.environment["RIFT_CLI_PATH"],
           FileManager.default.isExecutableFile(atPath: explicitPath) {
            return .direct(URL(fileURLWithPath: explicitPath))
        }

        for directory in candidateDirectories() {
            let candidate = directory.appending(path: "target/debug/rift-cli", directoryHint: .notDirectory)
            if FileManager.default.isExecutableFile(atPath: candidate.path()) {
                return .direct(candidate)
            }
        }

        for path in ["/opt/homebrew/bin/rift-cli", "/usr/local/bin/rift-cli"] {
            if FileManager.default.isExecutableFile(atPath: path) {
                return .direct(URL(fileURLWithPath: path))
            }
        }

        return .env("rift-cli")
    }

    private static func candidateDirectories() -> [URL] {
        var seen = Set<String>()
        var directories: [URL] = []

        func appendAncestors(of startingURL: URL) {
            var current = startingURL.standardizedFileURL
            let fileManager = FileManager.default
            if !fileManager.fileExists(atPath: current.path(), isDirectory: nil) {
                return
            }
            if current.hasDirectoryPath == false {
                current.deleteLastPathComponent()
            }

            while true {
                let path = current.path()
                if seen.insert(path).inserted {
                    directories.append(current)
                }
                let parent = current.deletingLastPathComponent()
                if parent.path() == current.path() {
                    break
                }
                current = parent
            }
        }

        appendAncestors(of: URL(fileURLWithPath: FileManager.default.currentDirectoryPath, isDirectory: true))
        appendAncestors(of: URL(fileURLWithPath: CommandLine.arguments[0]))
        appendAncestors(of: Bundle.main.bundleURL)

        return directories
    }
}

private enum CLICommand {
    case direct(URL)
    case env(String)
}

public enum RiftCLIProviderError: LocalizedError {
    case launchFailed(String)
    case commandFailed(String)

    public var errorDescription: String? {
        switch self {
        case .launchFailed(let details):
            "Failed to launch rift-cli. \(details)"
        case .commandFailed(let details):
            "rift-cli query overview failed. \(details)"
        }
    }
}

struct RustOverviewPayload: Decodable {
    var generatedAtMs: UInt64
    var currentDisplay: RustDisplayPayload?
    var workspaces: [RustWorkspacePayload]

    func makeSnapshot() -> RiftOverviewSnapshot {
        RiftOverviewSnapshot(
            generatedAt: Date(timeIntervalSince1970: Double(generatedAtMs) / 1000.0),
            currentDisplayName: currentDisplay?.displayName ?? "Rift",
            workspaces: workspaces.enumerated().map { offset, workspace in
                workspace.makeSnapshot(position: offset)
            }
        )
    }
}

struct RustDisplayPayload: Decodable {
    var uuid: String
    var name: String?
    var screenId: UInt32

    var displayName: String {
        name ?? "Display \(screenId)"
    }
}

struct RustWorkspacePayload: Decodable {
    var id: String
    var index: Int
    var name: String
    var layoutMode: String
    var isActive: Bool
    var windowCount: Int
    var windows: [RustWindowPayload]

    func makeSnapshot(position: Int) -> RiftWorkspaceSnapshot {
        let accent = AppearancePalette.accent(for: index, seed: name)
        let title = name.isEmpty ? "Workspace \(index + 1)" : name
        let subtitle = "\(layoutMode.replacingOccurrences(of: "_", with: " ").capitalized) layout • \(windowCount) windows"

        return RiftWorkspaceSnapshot(
            id: id,
            index: index + 1,
            name: title,
            subtitle: subtitle,
            isActive: isActive,
            accentHex: accent,
            windows: windows.enumerated().map { offset, window in
                window.makeSnapshot(accentHex: accent, position: position + offset)
            }
        )
    }
}

struct RustWindowPayload: Decodable {
    var id: RustWindowIdentifier
    var title: String
    var frame: RustRect
    var isFloating: Bool
    var isFocused: Bool
    var bundleId: String?
    var appName: String?

    func makeSnapshot(accentHex: String, position: Int) -> RiftWindowSnapshot {
        let resolvedAppName = appName ?? bundleId?.split(separator: ".").last.map(String.init) ?? "Window"
        let role = semanticRole(for: resolvedAppName, title: title)
        let gradients = AppearancePalette.gradient(for: resolvedAppName + title, offset: position)
        let badge = isFocused ? "active" : (isFloating ? "floating" : role)
        let aspectRatio = max(1.0, min(1.8, frame.size.width / max(frame.size.height, 1.0)))
        let subtitle = "\(Int(frame.size.width))×\(Int(frame.size.height))"

        return RiftWindowSnapshot(
            id: "\(id.pid)-\(id.idx)",
            appName: resolvedAppName,
            title: title.isEmpty ? resolvedAppName : title,
            subtitle: subtitle,
            role: role,
            tintHex: accentHex,
            preview: RiftWindowPreviewStyle(
                badge: badge,
                gradientStartHex: gradients.0,
                gradientEndHex: gradients.1,
                aspectRatio: aspectRatio
            )
        )
    }

    private func semanticRole(for appName: String, title: String) -> String {
        let normalized = "\(appName) \(title)".lowercased()

        if normalized.contains("xcode") || normalized.contains("code") {
            return "editor"
        }
        if normalized.contains("terminal") || normalized.contains("ghostty") || normalized.contains("warp") || normalized.contains("iterm") {
            return "terminal"
        }
        if normalized.contains("safari") || normalized.contains("arc") || normalized.contains("chrome") || normalized.contains("firefox") {
            return "browser"
        }
        if normalized.contains("slack") || normalized.contains("discord") || normalized.contains("messages") || normalized.contains("matrix") {
            return "chat"
        }
        if normalized.contains("figma") || normalized.contains("preview") || normalized.contains("photoshop") {
            return "design"
        }
        if isFloating {
            return "floating"
        }
        return "window"
    }
}

struct RustWindowIdentifier: Decodable {
    var pid: Int
    var idx: Int
}

struct RustRect: Decodable {
    var origin: RustPoint
    var size: RustSize
}

struct RustPoint: Decodable {
    var x: Double
    var y: Double
}

struct RustSize: Decodable {
    var width: Double
    var height: Double
}

private enum AppearancePalette {
    static let accents = [
        "74F0A7",
        "6AE4FF",
        "FFB86B",
        "D8B4FE",
        "FF8E72",
        "B4F55C"
    ]

    static let gradients = [
        ("132F45", "07131D"),
        ("132F27", "08110D"),
        ("4A2714", "190D07"),
        ("38184F", "130818"),
        ("47211E", "170908"),
        ("2A3355", "0D101B")
    ]

    static func accent(for index: Int, seed: String) -> String {
        accents[(stableHash(seed) + index) % accents.count]
    }

    static func gradient(for seed: String, offset: Int) -> (String, String) {
        gradients[(stableHash(seed) + offset) % gradients.count]
    }

    private static func stableHash(_ value: String) -> Int {
        value.utf8.reduce(5381) { partial, byte in
            ((partial << 5) &+ partial) &+ Int(byte)
        }
    }
}
