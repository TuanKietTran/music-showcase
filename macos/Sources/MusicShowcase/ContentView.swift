import SwiftUI

/// "m:ss" for a duration in seconds (same format as Android's formatDuration).
func formatDuration(_ durationSec: Double) -> String {
    let total = Int(durationSec)
    return String(format: "%d:%02d", total / 60, total % 60)
}

struct ContentView: View {
    @StateObject private var player = PlayerModel()
    // Token and base URL live in memory only (docs/api.md) — @State, never
    // @AppStorage/UserDefaults.
    @State private var baseURLString = "http://localhost:8080"
    @State private var token = ""
    @State private var isLoading = false
    @State private var libraryError: ErrorCode?
    @State private var needsToken = false

    var body: some View {
        VStack(spacing: 0) {
            Form {
                TextField("Server", text: $baseURLString)
                SecureField("Token", text: $token)
                HStack {
                    Button(isLoading ? "Loading\u{2026}" : "Load tracks") { Task { await load() } }.disabled(isLoading)
                    if isLoading { ProgressView().controlSize(.small) }
                }
            }
            .formStyle(.grouped)
            .frame(height: 132)
            if needsToken {
                let entry = libraryError?.entry ?? ErrorCode.unauthorized.entry
                VStack(alignment: .leading, spacing: 4) {
                    Text(entry.title).bold()
                    Text(entry.message)
                }.padding(.horizontal)
            } else if let libraryError {
                ErrorBanner(code: libraryError) { action in
                    if action == "retry" { Task { await load() } } else { self.libraryError = nil }
                }
            }
            if player.tracks.isEmpty && libraryError == nil && !isLoading {
                VStack(spacing: 8) {
                    Text(ErrorCode.emptyLibrary.entry.title).font(.headline)
                    Text(ErrorCode.emptyLibrary.entry.message).foregroundStyle(.secondary).multilineTextAlignment(.center)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .padding()
            } else {
            List(player.tracks) { track in
                HStack {
                    VStack(alignment: .leading) {
                        Text(track.title).lineLimit(1)
                        Text(track.artist).font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
                    }
                    Spacer()
                    if player.currentTrack?.id == track.id && player.isBuffering { ProgressView().controlSize(.small) }
                    Text(formatDuration(track.durationSec)).font(.caption).foregroundStyle(.secondary)
                }
                .contentShape(Rectangle())
                .onTapGesture { start(track) }
            }
            }
            Divider()
            if let e = player.playbackError {
                ErrorBanner(code: e) { action in
                    switch action {
                    case "retry": player.retryPlayback()
                    case "skip-track": player.skipAfterError()
                    default: player.dismissPlaybackError()
                    }
                }
            }
            bottomBar
        }
        .frame(minWidth: 640, minHeight: 480)
        .task { await load() }
    }

    private var bottomBar: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let track = player.currentTrack {
                Text("\(track.title) \u{2014} \(track.artist)").font(.headline).lineLimit(1)
            } else {
                Text("Select a track to play.").foregroundStyle(.secondary)
            }
            HStack(spacing: 8) {
                Text(formatDuration(player.position)).font(.caption).monospacedDigit()
                Slider(value: Binding(get: { player.position }, set: { player.position = $0 }), in: 0...max(player.duration, player.currentTrack?.durationSec ?? 0, 1), onEditingChanged: { editing in if !editing { player.seek(to: player.position) } })
                Text(formatDuration(max(player.duration, player.currentTrack?.durationSec ?? 0))).font(.caption).monospacedDigit()
            }
            HStack(spacing: 16) {
                Spacer()
                Button { player.previous() } label: { Image(systemName: "backward.fill") }.help("Previous")
                Button { player.toggle() } label: { Image(systemName: player.isPlaying ? "pause.circle.fill" : "play.circle.fill").font(.largeTitle) }.help(player.isPlaying ? "Pause" : "Play")
                Button { player.next() } label: { Image(systemName: "forward.fill") }.help("Next")
                if player.isBuffering { ProgressView().controlSize(.small) }
                Spacer()
                Image(systemName: "speaker.wave.2.fill").foregroundStyle(.secondary)
                Slider(value: Binding(get: { Double(player.volume) }, set: { player.volume = Float($0) }), in: 0...1).frame(width: 90)
            }
        }
        .padding(12)
    }

    private func start(_ track: Track) {
        player.queue = player.tracks
        player.play(track: track)
    }

    private func load() async {
        guard let baseURL = URL(string: baseURLString) else { libraryError = .serverUnreachable; return }
        isLoading = true
        defer { isLoading = false }
        let client = APIClient(baseURL: baseURL, token: token)
        player.token = token
        player.baseURL = baseURL
        var attempt = 0
        while true {
            do {
                player.tracks = try await client.tracks()
                needsToken = false
                libraryError = nil
                return
            } catch {
                let code = ErrorCode.classify(error, loaded: !player.tracks.isEmpty)
                libraryError = code
                needsToken = code == .unauthorized || code == .unauthorizedMidSession
                let entry = code.entry
                guard entry.autoRetry, code != .unauthorized, code != .unauthorizedMidSession, code != .malformedResponse, attempt < entry.maxAttempts, !entry.backoffMs.isEmpty else { return }
                let delayMs = entry.backoffMs[min(attempt, entry.backoffMs.count - 1)]
                attempt += 1
                try? await Task.sleep(nanoseconds: UInt64(delayMs) * 1_000_000)
            }
        }
    }
}
