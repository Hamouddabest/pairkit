import SwiftUI

struct SettingsView: View {
    @EnvironmentObject var engine: Engine
    @AppStorage("accent") private var accent = 0
    @AppStorage("defaultApp") private var defaultApp = "sidestore"
    @AppStorage("certMode") private var certMode = "ask"
    @AppStorage("extReuse") private var extReuse = true
    @AppStorage("rememberEmail") private var rememberEmail = true
    @AppStorage("keepAwakeAudio") private var keepAwakeAudio = true
    @AppStorage("keepScreenOn") private var keepScreenOn = true
    @State private var confirm: String?

    var body: some View {
        ZStack {
            Backdrop()
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text("Settings").font(.system(size: 38, weight: .bold, design: .rounded)).padding(.top, 8)

                    section("Appearance") {
                        HStack(spacing: 14) {
                            ForEach(Theme.accents) { a in
                                Circle().fill(a.color).frame(width: 38, height: 38)
                                    .overlay(Circle().stroke(Color.white, lineWidth: accent == a.id ? 3 : 0))
                                    .onTapGesture { accent = a.id }
                            }
                        }
                    }

                    section("Install") {
                        picker("Default app", selection: $defaultApp, options: AppItem.all.map { ($0.id, $0.name) })
                        divider
                        picker("Certificate", selection: $certMode, options: [("ask", "Ask me"), ("reuse", "Always reuse"), ("new", "Always make new")])
                        divider
                        Toggle("Extensions share the main app ID", isOn: $extReuse)
                        Text("Saves app IDs on free accounts. Turn off only if an app needs separate IDs.")
                            .font(.caption).foregroundColor(.white.opacity(0.55))
                        divider
                        Toggle("Remember my Apple ID email", isOn: $rememberEmail)
                        Text("The password and codes are never saved.").font(.caption).foregroundColor(.white.opacity(0.55))
                    }

                    section("Sign-in") {
                        NavigationLink {
                            AnisetteView()
                        } label: {
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text("Anisette servers").foregroundColor(.white)
                                    Text("Fix \"failed to login\": change or add servers")
                                        .font(.caption).foregroundColor(.white.opacity(0.55))
                                }
                                Spacer()
                                Image(systemName: "chevron.right").foregroundColor(.white.opacity(0.4))
                            }
                        }
                    }

                    section("Apple ID") {
                        NavigationLink {
                            AppIDsScreen()
                        } label: {
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text("App IDs").foregroundColor(.white)
                                    Text("See how many are left and delete old ones")
                                        .font(.caption).foregroundColor(.white.opacity(0.55))
                                }
                                Spacer()
                                Image(systemName: "chevron.right").foregroundColor(.white.opacity(0.4))
                            }
                        }
                    }

                    section("Keep running") {
                        Toggle("Silent audio while pairing", isOn: $keepAwakeAudio)
                        Text("Stops iOS from freezing PairKit while you are in Settings.").font(.caption).foregroundColor(.white.opacity(0.55))
                        divider
                        Toggle("Keep the screen on while installing", isOn: $keepScreenOn)
                    }

                    section("Files") {
                        row("Pairing file", engine.hasPairingFile ? "Saved" : "None")
                        divider
                        Button("Delete pairing file") { confirm = "pairing" }.foregroundColor(.red)
                        divider
                        Button("Forget saved certificate key") { confirm = "keys" }.foregroundColor(.orange)
                        divider
                        Button("Delete downloaded IPAs") { confirm = "ipas" }.foregroundColor(.orange)
                    }

                    section("About") {
                        row("PairKit", "1.0")
                        divider
                        Text("Needs LocalDevVPN on to reach this iPhone. Sign in with your own Apple ID; it is only sent to Apple.")
                            .font(.caption).foregroundColor(.white.opacity(0.55))
                    }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 30)
            }
        }
        .alert("Are you sure?", isPresented: Binding(get: { confirm != nil }, set: { if !$0 { confirm = nil } })) {
            Button("Cancel", role: .cancel) {}
            Button("Do it", role: .destructive) {
                switch confirm {
                case "pairing": engine.deletePairingFile()
                case "keys": engine.clearCertificateKeys()
                case "ipas": engine.clearDownloads()
                default: break
                }
                confirm = nil
            }
        }
    }

    private var divider: some View { Divider().background(Color.white.opacity(0.1)) }

    private func row(_ a: String, _ b: String) -> some View {
        HStack { Text(a); Spacer(); Text(b).foregroundColor(.white.opacity(0.6)) }
    }

    private func picker(_ title: String, selection: Binding<String>, options: [(String, String)]) -> some View {
        HStack {
            Text(title)
            Spacer()
            Picker(title, selection: selection) {
                ForEach(options, id: \.0) { o in Text(o.1).tag(o.0) }
            }
            .pickerStyle(.menu)
        }
    }

    private func section<C: View>(_ title: String, @ViewBuilder _ content: () -> C) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title.uppercased()).font(.caption).fontWeight(.semibold).foregroundColor(.white.opacity(0.5)).padding(.leading, 6)
            Card {
                VStack(alignment: .leading, spacing: 10) { content() }
            }
        }
    }
}
