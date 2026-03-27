import Foundation
import RiftUICore

@MainActor
final class OverviewStore: ObservableObject {
    @Published private(set) var snapshot: RiftOverviewSnapshot = .empty
    @Published private(set) var isLoading = false
    @Published private(set) var errorMessage: String?

    private let provider: any RiftOverviewProviding
    private let fallbackProvider: (any RiftOverviewProviding)?
    private var hasLoaded = false

    init(
        provider: any RiftOverviewProviding = FallbackOverviewProvider(
            primary: RiftMachOverviewProvider(),
            fallback: RiftCLIOverviewProvider()
        ),
        fallbackProvider: (any RiftOverviewProviding)? = MockOverviewProvider()
    ) {
        self.provider = provider
        self.fallbackProvider = fallbackProvider
    }

    func loadIfNeeded() async {
        guard !hasLoaded else { return }
        hasLoaded = true
        await reload()
    }

    func reload() async {
        isLoading = true
        defer { isLoading = false }

        do {
            snapshot = try await provider.fetchOverview()
            errorMessage = nil
        } catch {
            if let fallbackProvider {
                snapshot = (try? await fallbackProvider.fetchOverview()) ?? .empty
                errorMessage = "Live Rift data unavailable; showing preview snapshot. \(error.localizedDescription)"
            } else {
                errorMessage = error.localizedDescription
            }
        }
    }
}
