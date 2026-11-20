import SwiftUI

/// Anisette servers are what let Apple accept the sign-in from a device like this one.
/// If one is down, signing in fails, so PairKit keeps a list and can try several.
enum Anisette {
    static let builtin = ["https://ani.iloader.app", "https://ani.sidestore.io"]
    static let listURL = "https://servers.sidestore.io/servers.json"

    static func official() -> [String] {
        let saved = UserDefaults.standard.stringArray(forKey: "anisetteOfficial") ?? []
        return merge(builtin, saved)
    }

    static func custom() -> [String] {
        UserDefaults.standard.stringArray(forKey: "anisetteCustom") ?? []
    }

    static func all() -> [String] { merge(official(), custom()) }

    static func merge(_ a: [String], _ b: [String]) -> [String] {
        var out: [String] = []
        for s in a + b {
            let t = clean(s)
            if !t.isEmpty && !out.contains(t) { out.append(t) }
        }
        return out
    }

    static func clean(_ s: String) -> String {
        var t = s.trimmingCharacters(in: .whitespacesAndNewlines)
        while t.hasSuffix("/") { t.removeLast() }
        if !t.isEmpty && !t.contains("://") { t = "https://" + t }
        return t
    }

    /// Order to try during sign-in: the chosen server first, then (if auto) all the others.
    static func tryOrder() -> [String] {
        let d = UserDefaults.standard
        let selected = d.string(forKey: "anisetteSelected") ?? "auto"
        let auto = d.object(forKey: "anisetteAuto") as? Bool ?? true
        let everything = all()
        if selected == "auto" { return everything }
        let first = clean(selected)
        return auto ? merge([first], everything) : [first]
    }

    /// Downloads SideStore's maintained list of public servers.
    static func refreshOfficial() async -> Bool {
        guard let u = URL(string: listURL),
              let (data, _) = try? await URLSession.shared.data(from: u),
              let obj = try? JSONSerialization.jsonObject(with: data) else { return false }
        var found: [String] = []
        func walk(_ x: Any) {
            if let d = x as? [String: Any] {
                for (k, v) in d {
                    if let s = v as? String, ["address", "url", "server"].contains(k.lowercased()), s.hasPrefix("http") {
                        found.append(s)
                    } else {
                        walk(v)
                    }
                }
            } else if let a = x as? [Any] {
                for i in a { walk(i) }
            }
        }
        walk(obj)
        let cleaned = merge([], found)
        guard !cleaned.isEmpty else { return false }
        UserDefaults.standard.set(cleaned, forKey: "anisetteOfficial")
        return true
    }
}

struct AnisetteView: View {
    @AppStorage("anisetteSelected") private var selected = "auto"
    @AppStorage("anisetteAuto") private var autoTry = true
    @State private var servers: [String] = Anisette.all()
    @State private var newURL = ""
    @State private var refreshing = false
    @State private var note = ""

    var body: some View {
        ZStack {
            Backdrop()
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Card {
                        VStack(alignment: .leading, spacing: 10) {
                            Toggle("Switch servers automatically", isOn: $autoTry)
                            Text("If sign-in fails, PairKit tries the next server on its own.")
                                .font(.caption).foregroundColor(.white.opacity(0.55))
                        }
                    }
                    Card {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("Server to use first").font(.headline)
                            row("Automatic", value: "auto")
                            ForEach(servers, id: \.self) { s in
                                Divider().background(Color.white.opacity(0.1))
                                row(s, value: s)
                            }
                        }
                    }
                    Card {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("Add your own").font(.headline)
                            TextField("https://my-anisette.example.com", text: $newURL)
                                .textInputAutocapitalization(.never)
                                .autocorrectionDisabled()
                                .keyboardType(.URL)
                                .padding(12)
                                .background(RoundedRectangle(cornerRadius: 12).fill(Color.black.opacity(0.35)))
                            SoftButton(title: "Add server", symbol: "plus.circle") {
                                let c = Anisette.clean(newURL)
                                guard !c.isEmpty else { return }
                                var cs = Anisette.custom()
                                if !cs.contains(c) { cs.append(c) }
                                UserDefaults.standard.set(cs, forKey: "anisetteCustom")
                                selected = c
                                newURL = ""
                                servers = Anisette.all()
                            }
                            if !Anisette.custom().isEmpty {
                                SoftButton(title: "Remove my added servers", symbol: "trash") {
                                    UserDefaults.standard.removeObject(forKey: "anisetteCustom")
                                    if !Anisette.official().contains(selected) { selected = "auto" }
                                    servers = Anisette.all()
                                }
                            }
                        }
                    }
                    Card {
                        VStack(alignment: .leading, spacing: 10) {
                            SoftButton(title: refreshing ? "Updating..." : "Update the public server list", symbol: "arrow.clockwise") {
                                refreshing = true
                                Task {
                                    let ok = await Anisette.refreshOfficial()
                                    await MainActor.run {
                                        refreshing = false
                                        servers = Anisette.all()
                                        note = ok ? "List updated." : "Could not reach the list. The built-in servers are still used."
                                    }
                                }
                            }
                            if !note.isEmpty { Text(note).font(.caption).foregroundColor(.white.opacity(0.6)) }
                        }
                    }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 30)
            }
        }
        .navigationTitle("Anisette servers")
        .navigationBarTitleDisplayMode(.inline)
    }

    private func row(_ title: String, value: String) -> some View {
        Button { selected = value } label: {
            HStack {
                Text(title).font(.subheadline).foregroundColor(.white).lineLimit(1).truncationMode(.middle)
                Spacer()
                if selected == value { Image(systemName: "checkmark.circle.fill").foregroundColor(.accentColor) }
            }
            .padding(.vertical, 6)
        }
    }
}
