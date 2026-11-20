import SwiftUI

struct AppIDInfo: Identifiable, Hashable {
    let id: String
    let identifier: String
    let name: String
    let expires: String?
}

struct AppIDPicker: View {
    @EnvironmentObject var engine: Engine
    let req: PromptReq
    @State private var items: [AppIDInfo] = []
    @State private var maxCount: Int?
    @State private var available: Int?
    @State private var chosen = Set<String>()
    @State private var confirm = false

    var body: some View {
        ZStack {
            Backdrop()
            VStack(alignment: .leading, spacing: 14) {
                Text("App IDs").font(.system(size: 28, weight: .bold, design: .rounded))
                Card {
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(available.map { "\($0) left" } ?? "Unknown").font(.title3).fontWeight(.bold)
                            Text(maxCount.map { "of \($0) per week" } ?? "free accounts get about 10 per week")
                                .font(.caption).foregroundColor(.white.opacity(0.6))
                        }
                        Spacer()
                        Image(systemName: "number.square.fill").font(.system(size: 30)).foregroundColor(.accentColor)
                    }
                }
                Text("Tick the ones to delete. An app that uses a deleted ID stops opening until you reinstall or refresh it.")
                    .font(.footnote).foregroundColor(.white.opacity(0.65))
                ScrollView {
                    VStack(spacing: 8) {
                        if items.isEmpty { Text("No app IDs on this account.").foregroundColor(.white.opacity(0.6)).padding() }
                        ForEach(items) { it in
                            Button {
                                if chosen.contains(it.id) { chosen.remove(it.id) } else { chosen.insert(it.id) }
                            } label: {
                                HStack(spacing: 12) {
                                    Image(systemName: chosen.contains(it.id) ? "checkmark.circle.fill" : "circle")
                                        .foregroundColor(chosen.contains(it.id) ? .red : .white.opacity(0.4))
                                        .font(.title3)
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(it.name).font(.subheadline).fontWeight(.semibold).foregroundColor(.white)
                                        Text(it.identifier).font(.caption).foregroundColor(.white.opacity(0.55)).lineLimit(1).truncationMode(.middle)
                                        if let e = it.expires { Text("Expires " + e.prefix(10)).font(.caption2).foregroundColor(.white.opacity(0.45)) }
                                    }
                                    Spacer()
                                }
                                .padding(12)
                                .background(RoundedRectangle(cornerRadius: 14).fill(Color.white.opacity(chosen.contains(it.id) ? 0.14 : 0.06)))
                            }
                        }
                    }
                }
                Button {
                    confirm = true
                } label: {
                    Text(chosen.isEmpty ? "Delete selected" : "Delete \(chosen.count) selected").fontWeight(.semibold)
                        .frame(maxWidth: .infinity).padding(.vertical, 14)
                        .background(RoundedRectangle(cornerRadius: 16).fill(Color.red.opacity(chosen.isEmpty ? 0.25 : 0.85)))
                        .foregroundColor(.white)
                }
                .disabled(chosen.isEmpty)
                SoftButton(title: "Done", symbol: "checkmark") { engine.submit("") }
            }
            .padding(20)
        }
        .interactiveDismissDisabled()
        .onAppear { parse() }
        .alert("Delete \(chosen.count) app ID(s)?", isPresented: $confirm) {
            Button("Cancel", role: .cancel) {}
            Button("Delete", role: .destructive) { engine.submit(chosen.joined(separator: ",")) }
        } message: {
            Text("Apps that use them will stop opening until reinstalled.")
        }
    }

    private func parse() {
        let json = String(req.label.dropFirst("APPIDS:".count))
        guard let d = json.data(using: .utf8),
              let o = try? JSONSerialization.jsonObject(with: d) as? [String: Any] else { return }
        maxCount = (o["max"] as? NSNumber)?.intValue
        available = (o["available"] as? NSNumber)?.intValue
        let arr = o["ids"] as? [[String: Any]] ?? []
        items = arr.map {
            AppIDInfo(id: $0["id"] as? String ?? "", identifier: $0["identifier"] as? String ?? "",
                      name: $0["name"] as? String ?? "", expires: $0["expires"] as? String)
        }
    }
}

struct AppIDsScreen: View {
    @EnvironmentObject var engine: Engine

    var body: some View {
        ZStack {
            Backdrop()
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Card {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("Free up app IDs").font(.headline)
                            Text("A free Apple ID only allows about 10 app IDs a week. If installs say there are none left, delete ones you don't need here.")
                                .font(.subheadline).foregroundColor(.white.opacity(0.7))
                        }
                    }
                    BigButton(title: engine.busy ? "Working..." : "Sign in and show my app IDs", symbol: "list.bullet.rectangle", enabled: !engine.busy) {
                        engine.manageAppIDs()
                    }
                    if engine.busy {
                        Card {
                            HStack(spacing: 12) {
                                ProgressView()
                                Text(engine.stage.isEmpty ? "Working" : engine.stage).font(.subheadline)
                            }
                        }
                    }
                    if let m = engine.resultMessage {
                        Card {
                            Text(m).font(.subheadline).foregroundColor(engine.resultIsError ? .red : .white.opacity(0.85))
                        }
                    }
                    if !engine.log.isEmpty {
                        Card {
                            Text(engine.log.suffix(12).joined(separator: "\n"))
                                .font(.system(size: 12, design: .monospaced)).foregroundColor(.white.opacity(0.75))
                        }
                    }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 30)
            }
        }
        .navigationTitle("App IDs")
        .navigationBarTitleDisplayMode(.inline)
    }
}
