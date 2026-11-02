import AVFoundation
import Combine
import Foundation

@MainActor
final class PlayerModel: ObservableObject {
    @Published var tracks: [Track] = []
    @Published var queue: [Track] = []
    @Published var currentIndex: Int = -1
    @Published var isPlaying: Bool = false
    @Published var isBuffering: Bool = false
    @Published var playbackError: ErrorCode?
    @Published var position: Double = 0
    @Published var duration: Double = 0
    @Published var volume: Float = 1.0 {
        didSet { player.volume = volume }
    }

    let player: AVPlayer
    var token: String
    /// Base for resolving a track's relative streamUrl, in case a Track
    /// reaches the player without going through APIClient's resolving.
    var baseURL: URL = URL(string: "http://localhost:8080")!
    private var timeObserver: Any?
    private var retryAttempt = 0
    private var retryTask: Task<Void, Never>?
    private var resumeAt: Double = 0
    private var statusObserver: NSKeyValueObservation?
    private var noteObservers: [NSObjectProtocol] = []

    var currentTrack: Track? {
        guard queue.indices.contains(currentIndex) else { return nil }
        return queue[currentIndex]
    }

    init(token: String = "") {
        self.token = token
        self.player = AVPlayer()
        self.player.automaticallyWaitsToMinimizeStalling = true
        self.player.volume = volume
        addTimeObserver()
    }

    deinit {
        if let timeObserver { player.removeTimeObserver(timeObserver) }
    }

    private func addTimeObserver() {
        let interval = CMTime(seconds: 0.5, preferredTimescale: 600)
        timeObserver = player.addPeriodicTimeObserver(forInterval: interval, queue: .main) { [weak self] time in
            guard let self else { return }
            Task { @MainActor in
                self.position = time.seconds.isFinite ? time.seconds : 0
                if let item = self.player.currentItem {
                    let d = item.duration.seconds
                    self.duration = d.isFinite ? d : 0
                }
                self.isPlaying = self.player.timeControlStatus == .playing
                if self.isPlaying && self.playbackError == .streamStalled { self.playbackError = nil; self.retryAttempt = 0 }
                self.isBuffering = self.player.timeControlStatus == .waitingToPlayAtSpecifiedRate
            }
        }
    }

    func play(_ track: Track) { play(track: track) }

    func play(track: Track) {
        playbackError = nil
        retryAttempt = 0
        retryTask?.cancel()
        if queue.isEmpty { queue = tracks }
        if let index = queue.firstIndex(where: { $0.id == track.id }) {
            currentIndex = index
        } else {
            queue.append(track)
            currentIndex = queue.count - 1
        }
        guard let url = URL(string: Track.resolve(baseURL: baseURL, path: track.streamUrl)) else { return }
        var options: [String: Any] = [:]
        if !token.trimmingCharacters(in: .whitespaces).isEmpty {
            options["AVURLAssetHTTPHeaderFieldsKey"] = ["Authorization": "Bearer \(token)"]
        }
        let asset = AVURLAsset(url: url, options: options)
        let item = AVPlayerItem(asset: asset)
        observeItem(item)
        player.replaceCurrentItem(with: item)
        player.play()
        isPlaying = true
        isBuffering = true
    }

    private func observeItem(_ item: AVPlayerItem) {
        statusObserver?.invalidate()
        noteObservers.forEach { NotificationCenter.default.removeObserver($0) }
        noteObservers = []
        statusObserver = item.observe(\.status, options: [.new]) { [weak self] item, _ in
            guard let self else { return }
            Task { @MainActor in
                switch item.status {
                case .readyToPlay:
                    self.playbackError = nil
                    self.retryAttempt = 0
                case .failed:
                    self.handleFailure(item.error)
                default:
                    break
                }
            }
        }
        noteObservers.append(NotificationCenter.default.addObserver(forName: .AVPlayerItemPlaybackStalled, object: item, queue: .main) { [weak self] _ in
            guard let self else { return }
            Task { @MainActor in
                self.resumeAt = self.position
                self.playbackError = .streamStalled
                self.isPlaying = false
                self.isBuffering = false
                self.scheduleAutoRetry(for: .streamStalled)
            }
        })
        noteObservers.append(NotificationCenter.default.addObserver(forName: .AVPlayerItemFailedToPlayToEndTime, object: item, queue: .main) { [weak self] notification in
            guard let self else { return }
            let error = notification.userInfo?[AVPlayerItemFailedToPlayToEndTimeErrorKey] as? Error
            Task { @MainActor in
                self.handleFailure(error)
            }
        })
    }

    private func scheduleAutoRetry(for code: ErrorCode) {
        guard code != .audioDecodeFailed, code != .trackNotFound else { return }
        if code.entry.autoRetry && retryAttempt < code.entry.maxAttempts {
            let backoffMs = code.entry.backoffMs.isEmpty ? 0 : code.entry.backoffMs[min(retryAttempt, code.entry.backoffMs.count - 1)]
            retryTask?.cancel()
            retryTask = Task { [weak self] in
                try? await Task.sleep(nanoseconds: UInt64(backoffMs) * 1_000_000)
                guard let self else { return }
                await MainActor.run {
                    guard !Task.isCancelled else { return }
                    self.retryAttempt += 1
                    self.reload()
                }
            }
        }
    }

    func handleFailure(_ error: Error?) {
        resumeAt = position
        let code = ErrorCode.classifyPlayback(error)
        playbackError = code
        isPlaying = false
        isBuffering = false
        scheduleAutoRetry(for: code)
    }

    private func reload() {
        guard let track = currentTrack else { return }
        guard let url = URL(string: Track.resolve(baseURL: baseURL, path: track.streamUrl)) else { return }
        var options: [String: Any] = [:]
        if !token.trimmingCharacters(in: .whitespaces).isEmpty {
            options["AVURLAssetHTTPHeaderFieldsKey"] = ["Authorization": "Bearer \(token)"]
        }
        let asset = AVURLAsset(url: url, options: options)
        let item = AVPlayerItem(asset: asset)
        observeItem(item)
        player.replaceCurrentItem(with: item)
        player.seek(to: CMTime(seconds: resumeAt, preferredTimescale: 600))
        player.play()
        isPlaying = true
        isBuffering = true
    }

    func retryPlayback() {
        retryAttempt = 0
        reload()
    }

    func dismissPlaybackError() {
        retryTask?.cancel()
        playbackError = nil
    }

    func skipAfterError() {
        retryTask?.cancel()
        next()
    }

    func toggle() {
        if player.timeControlStatus == .playing || isPlaying {
            player.pause()
            isPlaying = false
        } else {
            player.play()
            isPlaying = true
        }
    }

    func next() {
        let nextIndex = currentIndex + 1
        guard queue.indices.contains(nextIndex) else { return }
        play(track: queue[nextIndex])
    }

    func previous() {
        let previousIndex = currentIndex - 1
        guard queue.indices.contains(previousIndex) else { return }
        play(track: queue[previousIndex])
    }

    func seek(to seconds: Double) {
        let time = CMTime(seconds: seconds, preferredTimescale: 600)
        player.seek(to: time)
        position = seconds
    }
}
