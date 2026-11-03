import Foundation

enum ErrorCode: String, CaseIterable {
    case serverUnreachable = "server-unreachable"
    case requestTimeout = "request-timeout"
    case unauthorized = "unauthorized"
    case tokenRejected = "token-rejected"
    case unauthorizedMidSession = "unauthorized-mid-session"
    case trackNotFound = "track-not-found"
    case badRange = "bad-range"
    case serverError = "server-error"
    case malformedResponse = "malformed-response"
    case audioDecodeFailed = "audio-decode-failed"
    case streamStalled = "stream-stalled"
    case connectionLostDuringPlayback = "connection-lost-during-playback"
    case coverLoadFailed = "cover-load-failed"
    case emptyLibrary = "empty-library"
    case unknown = "unknown"
}

struct CatalogEntry { let code: ErrorCode; let severity: String; let surface: String; let title: String; let message: String; let actions: [String]; let autoRetry: Bool; let maxAttempts: Int; let backoffMs: [Int]; let assertive: Bool; let silent: Bool }

extension ErrorCode {
    var entry: CatalogEntry {
        switch self {
        case .serverUnreachable: return CatalogEntry(code: .serverUnreachable, severity: "error", surface: "inline-banner", title: "Can't reach the server", message: "We couldn't connect to the music server. Check that it is running and that you are online, then try again.", actions: ["retry", "change-server", "dismiss"], autoRetry: true, maxAttempts: 3, backoffMs: [1000, 2000, 4000], assertive: true, silent: false)
        case .requestTimeout: return CatalogEntry(code: .requestTimeout, severity: "warning", surface: "inline-banner", title: "The server is taking too long", message: "The server didn't answer in time. Your library is still here. Try again in a moment.", actions: ["retry", "dismiss"], autoRetry: true, maxAttempts: 3, backoffMs: [1000, 2000, 4000], assertive: false, silent: false)
        case .unauthorized: return CatalogEntry(code: .unauthorized, severity: "blocking", surface: "blocking-dialog", title: "Access token needed", message: "This server needs an access token before it will share music. Enter the token to continue.", actions: ["enter-token", "change-server"], autoRetry: false, maxAttempts: 0, backoffMs: [], assertive: true, silent: false)
        case .tokenRejected: return CatalogEntry(code: .tokenRejected, severity: "warning", surface: "inline-banner", title: "That token wasn't accepted", message: "Check it and try again; it is the MUSIC_TOKEN the server was started with.", actions: ["retry"], autoRetry: false, maxAttempts: 0, backoffMs: [], assertive: true, silent: false)
        case .unauthorizedMidSession: return CatalogEntry(code: .unauthorizedMidSession, severity: "blocking", surface: "blocking-dialog", title: "Please sign in again", message: "The server no longer accepts your token. Enter it again to keep listening. Your queue and position are kept.", actions: ["enter-token", "dismiss"], autoRetry: false, maxAttempts: 0, backoffMs: [], assertive: true, silent: false)
        case .trackNotFound: return CatalogEntry(code: .trackNotFound, severity: "warning", surface: "toast", title: "Track unavailable", message: "This track is no longer on the server, so we can't play it. You can skip to the next one.", actions: ["skip-track", "dismiss"], autoRetry: false, maxAttempts: 0, backoffMs: [], assertive: false, silent: false)
        case .badRange: return CatalogEntry(code: .badRange, severity: "warning", surface: "toast", title: "Couldn't jump to that spot", message: "We couldn't seek to that point in the track. Playback restarts from where you were.", actions: ["retry", "dismiss"], autoRetry: true, maxAttempts: 1, backoffMs: [500], assertive: false, silent: false)
        case .serverError: return CatalogEntry(code: .serverError, severity: "error", surface: "inline-banner", title: "The server had a problem", message: "Something went wrong on the server's side. Nothing you did caused it. Try again shortly.", actions: ["retry", "dismiss"], autoRetry: true, maxAttempts: 3, backoffMs: [1000, 3000, 8000], assertive: false, silent: false)
        case .malformedResponse: return CatalogEntry(code: .malformedResponse, severity: "error", surface: "inline-banner", title: "Unexpected reply from the server", message: "The server sent something we couldn't read. Trying again may help; if not, the server may need an update.", actions: ["retry", "change-server", "dismiss"], autoRetry: false, maxAttempts: 0, backoffMs: [], assertive: false, silent: false)
        case .audioDecodeFailed: return CatalogEntry(code: .audioDecodeFailed, severity: "error", surface: "toast", title: "This track won't play", message: "We couldn't play this track's audio. Your queue is unchanged, so you can skip to the next track.", actions: ["skip-track", "dismiss"], autoRetry: false, maxAttempts: 0, backoffMs: [], assertive: false, silent: false)
        case .streamStalled: return CatalogEntry(code: .streamStalled, severity: "info", surface: "inline-banner", title: "Still loading the music", message: "The music is taking a while to load. We'll keep trying and resume from the same spot.", actions: ["retry", "skip-track"], autoRetry: true, maxAttempts: 3, backoffMs: [2000, 4000, 8000], assertive: false, silent: false)
        case .connectionLostDuringPlayback: return CatalogEntry(code: .connectionLostDuringPlayback, severity: "warning", surface: "inline-banner", title: "Connection lost", message: "Playback paused because the connection dropped. We'll resume from the same spot when you're back online.", actions: ["retry", "skip-track", "dismiss"], autoRetry: true, maxAttempts: 3, backoffMs: [1000, 2000, 4000], assertive: false, silent: false)
        case .coverLoadFailed: return CatalogEntry(code: .coverLoadFailed, severity: "info", surface: "toast", title: "Cover art unavailable", message: "Cover art couldn't be loaded. A placeholder is shown instead.", actions: [], autoRetry: false, maxAttempts: 0, backoffMs: [], assertive: false, silent: true)
        case .emptyLibrary: return CatalogEntry(code: .emptyLibrary, severity: "info", surface: "empty-state", title: "No music yet", message: "The library is empty. Add some tracks on the server and refresh.", actions: ["retry"], autoRetry: false, maxAttempts: 0, backoffMs: [], assertive: false, silent: false)
        case .unknown: return CatalogEntry(code: .unknown, severity: "error", surface: "inline-banner", title: "Something went wrong", message: "Something unexpected happened. Try again, and your library and queue will stay as they are.", actions: ["retry", "dismiss"], autoRetry: false, maxAttempts: 0, backoffMs: [], assertive: false, silent: false)
        }
    }
}

extension ErrorCode {
    static func classify(_ error: Error, loaded: Bool = false, playing: Bool = false, submitted: Bool = false) -> ErrorCode {
        if let e = error as? APIClientError {
            switch e {
            case .unauthorized: return submitted ? .tokenRejected : (loaded ? .unauthorizedMidSession : .unauthorized)
            case .http(let status): if status == 404 { return .trackNotFound }; if status == 416 { return .badRange }; if status >= 500 { return .serverError }; return .unknown
            }
        }
        if let u = error as? URLError {
            if u.code == .timedOut { return .requestTimeout }
            switch u.code {
            case .notConnectedToInternet, .cannotConnectToHost, .cannotFindHost, .networkConnectionLost: return playing ? .connectionLostDuringPlayback : .serverUnreachable
            default: break
            }
        }
        if error is DecodingError { return .malformedResponse }
        return .unknown
    }
    static func classifyPlayback(_ error: Error?) -> ErrorCode {
        // AVFoundation wraps the network error as an underlying NSError; walk the chain.
        var current: NSError? = error as NSError?
        while let e = current {
            if e.domain == NSURLErrorDomain {
                switch e.code {
                case NSURLErrorNotConnectedToInternet, NSURLErrorCannotConnectToHost, NSURLErrorCannotFindHost, NSURLErrorNetworkConnectionLost, NSURLErrorTimedOut: return .connectionLostDuringPlayback
                case NSURLErrorBadServerResponse: return .serverError
                default: break
                }
            }
            current = e.userInfo[NSUnderlyingErrorKey] as? NSError
        }
        return .audioDecodeFailed
    }
}
