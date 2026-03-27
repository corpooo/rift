import SwiftUI
import RiftUICore

struct RiftUIRootView: View {
    @ObservedObject var store: OverviewStore

    var body: some View {
        ZStack {
            OverviewBackdrop()

            VStack(alignment: .leading, spacing: 28) {
                OverviewHeader(snapshot: store.snapshot, isLoading: store.isLoading)

                if let errorMessage = store.errorMessage {
                    Text(errorMessage)
                        .font(.custom("Menlo", size: 12))
                        .foregroundStyle(Color.white.opacity(0.7))
                        .padding(.horizontal, 16)
                        .padding(.vertical, 12)
                        .background(
                            RoundedRectangle(cornerRadius: 14, style: .continuous)
                                .fill(Color.white.opacity(0.05))
                        )
                }

                ScrollView(.vertical, showsIndicators: false) {
                    LazyVStack(spacing: 18) {
                        ForEach(store.snapshot.workspaces) { workspace in
                            WorkspaceOverviewRow(workspace: workspace)
                        }
                    }
                    .padding(.bottom, 24)
                }
            }
            .padding(.horizontal, 42)
            .padding(.vertical, 36)
        }
        .frame(minWidth: 1320, minHeight: 840)
        .task {
            await store.loadIfNeeded()
        }
    }
}

private struct OverviewHeader: View {
    let snapshot: RiftOverviewSnapshot
    let isLoading: Bool

    private var workspaceCountLabel: String {
        "\(snapshot.workspaces.count) workspaces"
    }

    private var windowCountLabel: String {
        let windowCount = snapshot.workspaces.reduce(0) { partial, workspace in
            partial + workspace.windows.count
        }
        return "\(windowCount) visible windows"
    }

    var body: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 8) {
                Text("RIFT OVERVIEW")
                    .font(.custom("Avenir Next Condensed", size: 54))
                    .fontWeight(.heavy)
                    .tracking(1.4)
                    .foregroundStyle(.white)

                Text("Mission-control-style workspace rows driven by a dedicated Swift client.")
                    .font(.custom("Avenir Next", size: 18))
                    .foregroundStyle(Color.white.opacity(0.7))

                HStack(spacing: 10) {
                    StatusChip(label: snapshot.currentDisplayName.uppercased(), accent: Color(hex: "6AE4FF"))
                    StatusChip(label: workspaceCountLabel.uppercased(), accent: Color(hex: "74F0A7"))
                    StatusChip(label: windowCountLabel.uppercased(), accent: Color(hex: "FFB86B"))
                }
            }

            Spacer(minLength: 24)

            VStack(alignment: .trailing, spacing: 12) {
                if isLoading {
                    ProgressView()
                        .progressViewStyle(.circular)
                        .tint(Color.white.opacity(0.8))
                }

                Text(snapshot.generatedAt.formatted(date: .omitted, time: .shortened))
                    .font(.custom("Menlo", size: 12))
                    .foregroundStyle(Color.white.opacity(0.65))

                Text("Esc to dismiss")
                    .font(.custom("Menlo", size: 12))
                    .foregroundStyle(Color.white.opacity(0.45))
            }
        }
    }
}

private struct StatusChip: View {
    let label: String
    let accent: Color

    var body: some View {
        Text(label)
            .font(.custom("Menlo", size: 11))
            .tracking(0.8)
            .foregroundStyle(.white.opacity(0.82))
            .padding(.horizontal, 12)
            .padding(.vertical, 7)
            .background(
                Capsule(style: .continuous)
                    .fill(.white.opacity(0.05))
                    .overlay(
                        Capsule(style: .continuous)
                            .stroke(accent.opacity(0.65), lineWidth: 1)
                    )
            )
    }
}

private struct WorkspaceOverviewRow: View {
    let workspace: RiftWorkspaceSnapshot

    private var rowAccent: Color {
        Color(hex: workspace.accentHex)
    }

    var body: some View {
        HStack(alignment: .top, spacing: 22) {
            WorkspaceRail(workspace: workspace, accent: rowAccent)
                .frame(width: 196)

            VStack(alignment: .leading, spacing: 12) {
                Text(workspace.subtitle)
                    .font(.custom("Avenir Next", size: 14))
                    .foregroundStyle(.white.opacity(0.52))

                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(alignment: .top, spacing: 14) {
                        ForEach(workspace.windows) { window in
                            WindowThumbnailCard(window: window, accent: rowAccent)
                        }
                    }
                    .padding(.vertical, 2)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(18)
        .background(
            RoundedRectangle(cornerRadius: 28, style: .continuous)
                .fill(Color.white.opacity(workspace.isActive ? 0.08 : 0.04))
                .overlay(
                    RoundedRectangle(cornerRadius: 28, style: .continuous)
                        .stroke(rowAccent.opacity(workspace.isActive ? 0.95 : 0.35), lineWidth: 1)
                )
        )
        .shadow(color: rowAccent.opacity(workspace.isActive ? 0.18 : 0.08), radius: 24, x: 0, y: 14)
    }
}

private struct WorkspaceRail: View {
    let workspace: RiftWorkspaceSnapshot
    let accent: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(String(format: "%02d", workspace.index))
                .font(.custom("Avenir Next Condensed", size: 54))
                .fontWeight(.black)
                .foregroundStyle(.white)

            Text(workspace.name.uppercased())
                .font(.custom("Menlo", size: 13))
                .tracking(1.0)
                .foregroundStyle(accent.opacity(0.96))

            Text("\(workspace.windows.count) windows")
                .font(.custom("Avenir Next", size: 16))
                .foregroundStyle(.white.opacity(0.68))

            if workspace.isActive {
                Text("ACTIVE")
                    .font(.custom("Menlo", size: 11))
                    .tracking(0.8)
                    .foregroundStyle(Color.black.opacity(0.85))
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .background(
                        Capsule(style: .continuous)
                            .fill(accent)
                    )
            }
        }
        .padding(18)
        .frame(maxHeight: .infinity, alignment: .topLeading)
        .background(
            RoundedRectangle(cornerRadius: 24, style: .continuous)
                .fill(accent.opacity(0.14))
        )
    }
}

private struct WindowThumbnailCard: View {
    let window: RiftWindowSnapshot
    let accent: Color

    private var previewWidth: CGFloat {
        CGFloat(max(186.0, min(320.0, window.preview.aspectRatio * 168.0)))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                Circle().fill(Color(hex: "FF6B6B")).frame(width: 7, height: 7)
                Circle().fill(Color(hex: "FFCC66")).frame(width: 7, height: 7)
                Circle().fill(Color(hex: "7BE495")).frame(width: 7, height: 7)

                Spacer()

                Text(window.appName)
                    .font(.custom("Menlo", size: 10))
                    .foregroundStyle(.white.opacity(0.5))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 9)

            ZStack(alignment: .bottomLeading) {
                RoundedRectangle(cornerRadius: 20, style: .continuous)
                    .fill(
                        LinearGradient(
                            colors: [
                                Color(hex: window.preview.gradientStartHex),
                                Color(hex: window.preview.gradientEndHex)
                            ],
                            startPoint: .topLeading,
                            endPoint: .bottomTrailing
                        )
                    )

                RoundedRectangle(cornerRadius: 20, style: .continuous)
                    .fill(
                        LinearGradient(
                            colors: [Color.black.opacity(0.0), Color.black.opacity(0.42)],
                            startPoint: .top,
                            endPoint: .bottom
                        )
                    )

                VStack(alignment: .leading, spacing: 8) {
                    Text(window.preview.badge.uppercased())
                        .font(.custom("Menlo", size: 10))
                        .tracking(0.7)
                        .foregroundStyle(Color.white.opacity(0.7))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(
                            Capsule(style: .continuous)
                                .fill(Color.white.opacity(0.12))
                        )

                    Spacer(minLength: 0)

                    Text(window.title)
                        .font(.custom("Avenir Next", size: 16))
                        .fontWeight(.semibold)
                        .foregroundStyle(.white)
                        .lineLimit(2)

                    Text(window.subtitle)
                        .font(.custom("Avenir Next", size: 12))
                        .foregroundStyle(.white.opacity(0.72))
                        .lineLimit(1)
                }
                .padding(16)
            }
            .frame(width: previewWidth, height: 168)
            .overlay(
                RoundedRectangle(cornerRadius: 20, style: .continuous)
                    .stroke(accent.opacity(0.18), lineWidth: 1)
            )

            HStack(alignment: .center) {
                Text(window.role.uppercased())
                    .font(.custom("Menlo", size: 10))
                    .tracking(0.6)
                    .foregroundStyle(Color.white.opacity(0.55))

                Spacer()

                Circle()
                    .fill(Color(hex: window.tintHex))
                    .frame(width: 10, height: 10)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 12)
        }
        .frame(width: previewWidth)
        .background(
            RoundedRectangle(cornerRadius: 24, style: .continuous)
                .fill(Color.white.opacity(0.045))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 24, style: .continuous)
                .stroke(Color.white.opacity(0.07), lineWidth: 1)
        )
        .shadow(color: Color.black.opacity(0.24), radius: 18, x: 0, y: 10)
    }
}

private struct OverviewBackdrop: View {
    var body: some View {
        ZStack {
            LinearGradient(
                colors: [
                    Color(hex: "06070B"),
                    Color(hex: "0A0D14"),
                    Color(hex: "040507")
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
            .ignoresSafeArea()

            Rectangle()
                .fill(
                    LinearGradient(
                        colors: [
                            Color(hex: "21D3EE").opacity(0.12),
                            Color.clear
                        ],
                        startPoint: .top,
                        endPoint: .bottom
                    )
                )
                .frame(width: 2)
                .blur(radius: 0.2)
                .ignoresSafeArea()

            VStack(spacing: 36) {
                ForEach(0..<6, id: \.self) { _ in
                    Capsule(style: .continuous)
                        .fill(Color.white.opacity(0.018))
                        .frame(height: 1)
                }
            }
            .padding(.horizontal, 32)
            .ignoresSafeArea()

            RadialGradient(
                colors: [
                    Color(hex: "6AE4FF").opacity(0.18),
                    Color.clear
                ],
                center: .topLeading,
                startRadius: 40,
                endRadius: 520
            )
            .ignoresSafeArea()

            RadialGradient(
                colors: [
                    Color(hex: "74F0A7").opacity(0.09),
                    Color.clear
                ],
                center: .bottomTrailing,
                startRadius: 40,
                endRadius: 480
            )
            .ignoresSafeArea()
        }
    }
}

private extension Color {
    init(hex: String) {
        let sanitized = hex.trimmingCharacters(in: CharacterSet.alphanumerics.inverted)
        var value: UInt64 = 0
        Scanner(string: sanitized).scanHexInt64(&value)

        let red: Double
        let green: Double
        let blue: Double
        let alpha: Double

        switch sanitized.count {
        case 8:
            red = Double((value >> 24) & 0xFF) / 255.0
            green = Double((value >> 16) & 0xFF) / 255.0
            blue = Double((value >> 8) & 0xFF) / 255.0
            alpha = Double(value & 0xFF) / 255.0
        default:
            red = Double((value >> 16) & 0xFF) / 255.0
            green = Double((value >> 8) & 0xFF) / 255.0
            blue = Double(value & 0xFF) / 255.0
            alpha = 1.0
        }

        self.init(.displayP3, red: red, green: green, blue: blue, opacity: alpha)
    }
}
