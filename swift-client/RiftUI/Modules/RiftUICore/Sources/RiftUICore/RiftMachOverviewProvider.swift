import CRiftMachBridge
import Foundation

public struct RiftMachOverviewProvider: RiftOverviewProviding {
    public init() {}

    public func fetchOverview() async throws -> RiftOverviewSnapshot {
        try await Task.detached(priority: .userInitiated) {
            let requestData = try RiftIPCCodec.encode(.getOverview)
            let responseData = try Self.send(requestData)
            return try Self.decode(responseData)
        }
        .value
    }

    private static func send(_ requestData: Data) throws -> Data {
        var responsePointer: UnsafeMutablePointer<CChar>?
        var responseLength: UInt32 = 0
        var errorPointer: UnsafeMutablePointer<CChar>?

        let succeeded = requestData.withUnsafeBytes { rawBuffer in
            guard let baseAddress = rawBuffer.bindMemory(to: CChar.self).baseAddress else {
                return false
            }

            return rift_mach_send_request(
                nil,
                baseAddress,
                UInt32(requestData.count),
                &responsePointer,
                &responseLength,
                &errorPointer
            )
        }

        defer {
            if let responsePointer {
                rift_mach_free_buffer(responsePointer)
            }
            if let errorPointer {
                rift_mach_free_buffer(errorPointer)
            }
        }

        guard succeeded else {
            let details = errorPointer.map { String(cString: $0) } ?? "Unknown Mach transport error"
            throw RiftMachProviderError.transport(details)
        }

        guard let responsePointer else {
            throw RiftMachProviderError.transport("Mach transport returned an empty response buffer")
        }

        return Data(bytes: responsePointer, count: Int(responseLength))
    }

    private static func decode(_ responseData: Data) throws -> RiftOverviewSnapshot {
        let jsonObject = try JSONSerialization.jsonObject(with: responseData)
        guard let dictionary = jsonObject as? [String: Any] else {
            throw RiftMachProviderError.invalidResponse("Mach response is not a JSON object")
        }

        if let payload = dictionary["data"] {
            let data = try JSONSerialization.data(withJSONObject: payload)
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            let overview = try decoder.decode(RustOverviewPayload.self, from: data)
            return overview.makeSnapshot()
        }

        if let errorPayload = dictionary["error"] {
            let data = try JSONSerialization.data(withJSONObject: errorPayload, options: [.sortedKeys])
            let message = String(data: data, encoding: .utf8) ?? "Unknown Rift error"
            throw RiftMachProviderError.server(message)
        }

        throw RiftMachProviderError.invalidResponse("Mach response did not contain a data or error payload")
    }
}

public enum RiftMachProviderError: LocalizedError {
    case transport(String)
    case server(String)
    case invalidResponse(String)

    public var errorDescription: String? {
        switch self {
        case .transport(let details):
            "Direct Mach request failed. \(details)"
        case .server(let details):
            "Rift returned an error payload over Mach. \(details)"
        case .invalidResponse(let details):
            "Rift returned an invalid Mach response. \(details)"
        }
    }
}
