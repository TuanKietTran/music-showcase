import SwiftUI
import AppKit

struct ErrorBanner: View {
    let code: ErrorCode
    var onAction: (String) -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    private var entry: CatalogEntry { code.entry }
    private var icon: String {
        switch entry.severity {
        case "info": return "info.circle"
        case "warning": return "exclamationmark.triangle"
        case "blocking": return "lock.circle"
        default: return "xmark.octagon"
        }
    }
    private func label(for action: String) -> String? {
        switch action {
        case "retry": return "Try again"
        case "skip-track": return "Skip track"
        case "dismiss": return "Dismiss"
        default: return nil
        }
    }
    var body: some View {
        Group {
            if entry.silent {
                EmptyView()
            } else {
                HStack(alignment: .top, spacing: 12) {
                    Image(systemName: icon)
                    Text(entry.severity)
                        .font(.caption)
                    VStack(alignment: .leading, spacing: 4) {
                        Text(entry.title)
                            .font(.headline)
                        Text(entry.message)
                            .font(.subheadline)
                    }
                    ForEach(entry.actions, id: \.self) { action in
                        if let text = label(for: action) {
                            Button(text) { onAction(action) }
                        }
                    }
                }
                .padding()
                .background(.quaternary, in: RoundedRectangle(cornerRadius: 8))
                .transition(reduceMotion ? .identity : .opacity)
                .accessibilityElement(children: .contain)
                .accessibilityLabel("\(entry.title). \(entry.message)")
                .onAppear {
                    NSAccessibility.post(element: NSApp as Any, notification: .announcementRequested, userInfo: [.announcement: entry.message, .priority: entry.assertive ? NSAccessibilityPriorityLevel.high.rawValue : NSAccessibilityPriorityLevel.medium.rawValue])
                }
            }
        }
    }
}
