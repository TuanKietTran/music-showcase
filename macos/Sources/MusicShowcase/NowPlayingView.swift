import SwiftUI

struct NowPlayingView: View {
    @ObservedObject var player: PlayerModel
    var coverData: Data?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let track = player.currentTrack {
                HStack(spacing: 12) {
                    coverImage
                    VStack(alignment: .leading) {
                        Text(track.title).font(.headline).lineLimit(1)
                        Text(track.artist).font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
                        if !detailLine(track).isEmpty {
                            Text(detailLine(track)).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                        }
                    }
                    if player.isBuffering { ProgressView().controlSize(.small) }
                    Spacer()
                }
                HStack(spacing: 8) {
                    Text(formatDuration(player.position)).font(.caption).monospacedDigit()
                    Slider(value: Binding(get: { player.position }, set: { player.position = $0 }), in: 0...max(player.duration, track.durationSec, 1), onEditingChanged: { editing in if !editing { player.seek(to: player.position) } })
                    Text(formatDuration(max(player.duration, track.durationSec))).font(.caption).monospacedDigit()
                }
                HStack(spacing: 16) {
                    Spacer()
                    Button { player.previous() } label: { Image(systemName: "backward.fill") }.help("Previous")
                    Button { player.toggle() } label: { Image(systemName: player.isPlaying ? "pause.circle.fill" : "play.circle.fill").font(.largeTitle) }.help(player.isPlaying ? "Pause" : "Play")
                    Button { player.next() } label: { Image(systemName: "forward.fill") }.help("Next")
                    Spacer()
                    Image(systemName: "speaker.wave.2.fill").foregroundStyle(.secondary)
                    Slider(value: Binding(get: { Double(player.volume) }, set: { player.volume = Float($0) }), in: 0...1).frame(width: 90)
                }
            } else {
                Text("Select a track to play.").foregroundStyle(.secondary)
            }
        }
        .padding(12)
    }

    private func detailLine(_ track: Track) -> String {
        var parts: [String] = []
        if track.sampleRate > 0 { parts.append(String(format: "%.1f kHz / %d-bit", Double(track.sampleRate) / 1000, track.bitDepth)) }
        if let format = track.format, !format.isEmpty { parts.append(format.uppercased()) }
        if let license = track.license, !license.isEmpty { parts.append(license) }
        return parts.joined(separator: " · ")
    }

    @ViewBuilder private var coverImage: some View {
        if let coverData, let image = NSImage(data: coverData) {
            Image(nsImage: image).resizable().scaledToFill().frame(width: 48, height: 48).clipped()
        } else {
            Image(systemName: "music.note").frame(width: 48, height: 48).background(.quaternary)
        }
    }
}
