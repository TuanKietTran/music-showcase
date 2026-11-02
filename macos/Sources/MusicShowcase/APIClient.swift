import Foundation

struct Track: Codable, Identifiable {
    let id: String
    let title: String
    let artist: String
    let durationSec: Double
    let sampleRate: Int
    let bitDepth: Int
    let bytes: Int
    let coverUrl: String?
    let streamUrl: String

    /// Server returns root-relative paths (`/api/tracks/{id}/stream|cover`,
    /// docs/api.md); resolve them against the client base URL, like
    /// Android's `Track.resolve`. Already-absolute URLs pass through.
    static func resolve(baseURL: URL, path: String) -> String {
        if path.hasPrefix("http://") || path.hasPrefix("https://") { return path }
        var base = baseURL.absoluteString
        while base.hasSuffix("/") { base.removeLast() }
        return path.hasPrefix("/") ? base + path : base + "/" + path
    }

    func streamAbsolute(_ baseURL: URL) -> String { Self.resolve(baseURL: baseURL, path: streamUrl) }
    func coverAbsolute(_ baseURL: URL) -> String { Self.resolve(baseURL: baseURL, path: coverUrl ?? "/api/tracks/\(id)/cover") }

    /// Copy with cover/stream made absolute; blank URLs fall back to the
    /// canonical routes for this id (mirrors Android `Track.fromJson`).
    func resolving(against baseURL: URL) -> Track {
        let streamPath = streamUrl.trimmingCharacters(in: .whitespaces).isEmpty ? "/api/tracks/\(id)/stream" : streamUrl
        let coverRaw = coverUrl?.trimmingCharacters(in: .whitespaces) ?? ""
        let coverPath = coverRaw.isEmpty ? "/api/tracks/\(id)/cover" : coverRaw
        return Track(id: id, title: title, artist: artist, durationSec: durationSec, sampleRate: sampleRate, bitDepth: bitDepth, bytes: bytes, coverUrl: Self.resolve(baseURL: baseURL, path: coverPath), streamUrl: Self.resolve(baseURL: baseURL, path: streamPath))
    }
}

enum APIClientError: Error {
    case unauthorized
    case http(Int)
}

final class APIClient {
    let baseURL: URL
    let token: String
    private let session: URLSession

    init(baseURL: URL, token: String, session: URLSession = .shared) {
        self.baseURL = baseURL
        self.token = token
        self.session = session
    }

    convenience init(baseURL: String, token: String) {
        self.init(baseURL: URL(string: baseURL) ?? URL(string: "http://localhost:8080")!, token: token)
    }

    func tracks() async throws -> [Track] {
        // docs/api.md: the route is /api/tracks, not /tracks.
        guard let url = URL(string: Track.resolve(baseURL: baseURL, path: "/api/tracks")) else { throw URLError(.badURL) }
        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        if !token.trimmingCharacters(in: .whitespaces).isEmpty {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        let (data, response) = try await session.data(for: request)
        if let http = response as? HTTPURLResponse, !(200...299).contains(http.statusCode) {
            throw http.statusCode == 401 ? APIClientError.unauthorized : APIClientError.http(http.statusCode)
        }
        let decoded = try JSONDecoder().decode([Track].self, from: data)
        return decoded.map { $0.resolving(against: baseURL) }
    }

    /// Cover art needs the Bearer header too (web fetches a blob, Android adds
    /// the header in Coil), so fetch the bytes instead of using AsyncImage.
    func coverData(for track: Track) async throws -> Data {
        guard let url = URL(string: track.coverAbsolute(baseURL)) else { throw URLError(.badURL) }
        var request = URLRequest(url: url)
        if !token.trimmingCharacters(in: .whitespaces).isEmpty {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        let (data, response) = try await session.data(for: request)
        if let http = response as? HTTPURLResponse, !(200...299).contains(http.statusCode) {
            throw http.statusCode == 401 ? APIClientError.unauthorized : APIClientError.http(http.statusCode)
        }
        return data
    }
}
