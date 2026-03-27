import Foundation
import Testing
@testable import RiftUICore

@Test func requestEncodingMatchesRustShape() throws {
    let data = try RiftIPCCodec.encode(.getWorkspaces(spaceID: 7))
    let json = try #require(String(data: data, encoding: .utf8))
    #expect(json == #"{"get_workspaces":{"space_id":7}}"#)
}

@Test func overviewRequestEncodingMatchesRustShape() throws {
    let data = try RiftIPCCodec.encode(.getOverview)
    let json = try #require(String(data: data, encoding: .utf8))
    #expect(json == #""get_overview""#)
}

@Test func mockOverviewContainsActiveWorkspace() async throws {
    let snapshot = try await MockOverviewProvider().fetchOverview()
    #expect(snapshot.workspaces.count == 4)
    #expect(snapshot.workspaces.contains { $0.isActive })
    #expect(snapshot.workspaces.allSatisfy { !$0.windows.isEmpty })
}
