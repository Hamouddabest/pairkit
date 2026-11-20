import SwiftUI

struct PairView: View {
    @EnvironmentObject var engine: Engine
    @State private var copied = false

    var body: some View {
        ZStack {
            Backdrop()
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Pair").font(.system(size: 38, weight: .bold, design: .rounded))
                        Text("Pair this iPhone with itself. No computer.").foregroundColor(.white.opacity(0.65))
                    }
                    .padding(.top, 8)

                    Card { status }

                    if engine.pairPhase == .idle || isFailed {
                        Card {
                            VStack(alignment: .leading, spacing: 10) {
                                Text("Before you start").font(.headline)
                                Label("Turn LocalDevVPN on", systemImage: "network")
                                Label("Allow Local Network when asked", systemImage: "wifi")
                                Label("Developer Mode must be on", systemImage: "hammer")
                            }
                            .font(.subheadline)
                            .foregroundColor(.white.opacity(0.8))
                        }
                    }

                    if case .waiting = engine.pairPhase {
                        Card { steps }
                    }
                    if case .code = engine.pairPhase {
                        Card { steps }
                    }

                    controls

                    if engine.hasPairingFile {
                        Card {
                            VStack(alignment: .leading, spacing: 10) {
                                Label("Pairing file", systemImage: "doc.badge.gearshape").font(.headline)
                                Text("Keep a copy so other apps (like SideStore) can use it.")
                                    .font(.footnote).foregroundColor(.white.opacity(0.6))
                                ShareLink(item: engine.pairingFileURL) {
                                    HStack { Image(systemName: "square.and.arrow.up"); Text("Export pairing file").fontWeight(.medium) }
                                        .frame(maxWidth: .infinity).padding(.vertical, 13)
                                        .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color.white.opacity(0.10)))
                                        .foregroundColor(.white)
                                }
                                SoftButton(title: copied ? "Copied!" : "Copy as text", symbol: "doc.on.doc") {
                                    if let t = try? String(contentsOf: engine.pairingFileURL) {
                                        UIPasteboard.general.string = t
                                        copied = true
                                    }
                                }
                            }
                        }
                    }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 30)
            }
        }
    }

    private var isFailed: Bool {
        if case .failed = engine.pairPhase { return true }
        return false
    }

    @ViewBuilder private var status: some View {
        switch engine.pairPhase {
        case .idle:
            HStack(spacing: 14) {
                Image(systemName: engine.hasPairingFile ? "checkmark.seal.fill" : "link.badge.plus")
                    .font(.system(size: 34)).foregroundColor(engine.hasPairingFile ? .green : .accentColor)
                VStack(alignment: .leading, spacing: 2) {
                    Text(engine.hasPairingFile ? "Paired" : "Not paired yet").font(.headline)
                    Text(engine.hasPairingFile ? "You can install apps now." : "Tap Start pairing below.")
                        .font(.subheadline).foregroundColor(.white.opacity(0.65))
                }
            }
        case .waiting:
            HStack(spacing: 14) {
                ProgressView().scaleEffect(1.3)
                VStack(alignment: .leading, spacing: 2) {
                    Text("Waiting for Settings").font(.headline)
                    Text("Follow the steps below.").font(.subheadline).foregroundColor(.white.opacity(0.65))
                }
            }
        case .code(let c):
            VStack(spacing: 8) {
                Text("Enter this code in Settings").font(.subheadline).foregroundColor(.white.opacity(0.7))
                Text(c).font(.system(size: 52, weight: .heavy, design: .monospaced)).foregroundColor(.accentColor)
            }
            .frame(maxWidth: .infinity)
        case .done:
            HStack(spacing: 14) {
                Image(systemName: "checkmark.circle.fill").font(.system(size: 34)).foregroundColor(.green)
                VStack(alignment: .leading, spacing: 2) {
                    Text("Paired!").font(.headline)
                    Text("Pairing file saved.").font(.subheadline).foregroundColor(.white.opacity(0.65))
                }
            }
        case .failed(let m):
            HStack(alignment: .top, spacing: 14) {
                Image(systemName: "exclamationmark.triangle.fill").font(.system(size: 30)).foregroundColor(.orange)
                VStack(alignment: .leading, spacing: 2) {
                    Text("Pairing failed").font(.headline)
                    Text(m).font(.footnote).foregroundColor(.white.opacity(0.7))
                }
            }
        }
    }

    private var steps: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("In the Settings app").font(.headline)
            Text("1. Open Settings > Privacy & Security > Developer Mode")
            Text("2. Tap \"Pair with pairkit\"")
            Text("3. Enter your passcode, then the 6-digit code shown here")
            Text("Come back to PairKit when it says Paired.").foregroundColor(.white.opacity(0.55))
        }
        .font(.subheadline)
        .foregroundColor(.white.opacity(0.85))
    }

    @ViewBuilder private var controls: some View {
        switch engine.pairPhase {
        case .waiting, .code:
            SoftButton(title: "Cancel", symbol: "xmark") { engine.stopPairing() }
        default:
            BigButton(title: engine.hasPairingFile ? "Pair again" : "Start pairing", symbol: "link") {
                copied = false
                engine.startPairing()
            }
        }
    }
}
