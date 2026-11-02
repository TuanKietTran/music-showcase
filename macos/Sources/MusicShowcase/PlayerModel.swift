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
                self.isBuffering = self.player.timeControlStatus == .waitingToPlayAtSpecifiedRate
            }
        }
    }

    func play(_ track: Track) { play(track: track) }

    func play(track: Track) {
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
        player.replaceCurrentItem(with: item)
        player.play()
        isPlaying = true
        isBuffering = true
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
