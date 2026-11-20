import SwiftUI

struct ContainersView: View {
    @EnvironmentObject var engine: Engine
    @State private var link = ""
    static let testURL = "https://github.com/Hamouddabest/pairkit/releases/download/app/HelloGuest.ipa"

    var body: some View {
        ZStack {
            Backdrop()
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text("Containers").font(.system(size: 38, weight: .bold, design: .rounded))
                            Text("TEST").font(.caption2).fontWeight(.bold).padding(.horizontal, 8).padding(.vertical, 3)
                                .background(Capsule().fill(Color.orange)).foregroundColor(.black)
                        }
                        Text("Run apps inside PairKit so they don't use app IDs of their own.")
                            .foregroundColor(.white.opacity(0.65))
                    }
                    .padding(.top, 8)

                    Card {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Experimental").font(.headline)
                            Text("This is a first test. Start with the Hello Guest app. If it opens, the container works on your iPhone. Launching restarts PairKit; open PairKit again to come back.")
                                .font(.footnote).foregroundColor(.white.opacity(0.7))
                        }
                    }

                    BigButton(title: engine.busy ? "Working..." : "Add the Hello Guest test app", symbol: "plus.app.fill", enabled: !engine.busy) {
                        engine.addGuest(link: Self.testURL)
                    }

                    Card {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("Add from a link").font(.headline)
                            TextField("https://example.com/app.ipa", text: $link)
                                .textInputAutocapitalization(.never)
                                .autocorrectionDisabled()
                                .keyboardType(.URL)
                                .padding(12)
                                .background(RoundedRectangle(cornerRadius: 12).fill(Color.black.opacity(0.35)))
                            SoftButton(title: "Paste link", symbol: "doc.on.clipboard") {
                                if let t = UIPasteboard.general.string { link = t.trimmingCharacters(in: .whitespacesAndNewlines) }
                            }
                            SoftButton(title: "Add app", symbol: "plus.circle") { engine.addGuest(link: link) }
                        }
                    }

                    if engine.busy || engine.resultMessage != nil {
                        Card {
                            if engine.busy {
                                HStack(spacing: 14) {
                                    ProgressRing(value: engine.progress).frame(width: 64, height: 64)
                                    Text(engine.stage.isEmpty ? "Working" : engine.stage).font(.headline)
                                }
                            } else if let m = engine.resultMessage {
                                HStack(alignment: .top, spacing: 12) {
                                    Image(systemName: engine.resultIsError ? "xmark.octagon.fill" : "checkmark.circle.fill")
                                        .foregroundColor(engine.resultIsError ? .red : .green).font(.title2)
                                    Text(m).font(.subheadline)
                                }
                            }
                        }
                    }

                    if !engine.guests.isEmpty {
                        Text("YOUR APPS").font(.caption).fontWeight(.semibold).foregroundColor(.white.opacity(0.5)).padding(.leading, 6)
                        ForEach(engine.guests) { g in
                            Card {
                                VStack(alignment: .leading, spacing: 10) {
                                    HStack {
                                        VStack(alignment: .leading, spacing: 2) {
                                            Text(g.name).font(.headline)
                                            Text(g.bundleID).font(.caption).foregroundColor(.white.opacity(0.55))
                                        }
                                        Spacer()
                                    }
                                    HStack(spacing: 10) {
                                        BigButton(title: "Launch", symbol: "play.fill") { engine.launch(g) }
                                        SoftButton(title: "Remove", symbol: "trash") { engine.deleteGuest(g) }
                                    }
                                }
                            }
                        }
                    }

                    if !engine.launchLog.isEmpty {
                        Card {
                            VStack(alignment: .leading, spacing: 6) {
                                Text("Last launch").font(.headline)
                                Text(engine.launchLog).font(.system(size: 12, design: .monospaced))
                                    .foregroundColor(engine.launchLog.contains("FAILED") ? .red : .white.opacity(0.8))
                                    .textSelection(.enabled)
                                Text("If this stops at \"calling main()\", the app started and then crashed or closed.")
                                    .font(.caption).foregroundColor(.white.opacity(0.5))
                            }
                        }
                    }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 30)
            }
        }
        .onAppear { engine.refreshGuests() }
    }
}
