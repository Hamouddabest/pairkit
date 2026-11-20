import SwiftUI
import UniformTypeIdentifiers

struct InstallView: View {
    @EnvironmentObject var engine: Engine
    @AppStorage("defaultApp") private var defaultApp = "sidestore"
    @State private var selected: AppItem = AppItem.all[0]
    @State private var customLink = ""
    @State private var pickedFile: URL?
    @State private var showPicker = false
    @State private var didInit = false

    var body: some View {
        ZStack {
            Backdrop()
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Install").font(.system(size: 38, weight: .bold, design: .rounded))
                        Text("Pick an app. The latest version is downloaded and signed with your Apple ID.")
                            .foregroundColor(.white.opacity(0.65))
                    }
                    .padding(.top, 8)

                    if !engine.hasPairingFile {
                        Card {
                            Label("Pair first (Pair tab).", systemImage: "exclamationmark.circle.fill")
                                .foregroundColor(.orange)
                        }
                    }

                    LazyVGrid(columns: [GridItem(.flexible(), spacing: 12), GridItem(.flexible(), spacing: 12)], spacing: 12) {
                        ForEach(AppItem.all) { item in
                            tile(item)
                        }
                    }

                    if selected.id == "custom" {
                        Card {
                            VStack(alignment: .leading, spacing: 10) {
                                Text("Custom IPA").font(.headline)
                                TextField("https://example.com/app.ipa", text: $customLink)
                                    .textInputAutocapitalization(.never)
                                    .autocorrectionDisabled()
                                    .keyboardType(.URL)
                                    .padding(12)
                                    .background(RoundedRectangle(cornerRadius: 12).fill(Color.black.opacity(0.35)))
                                SoftButton(title: pickedFile == nil ? "Or choose an IPA file" : "File: \(pickedFile!.lastPathComponent)", symbol: "doc") {
                                    showPicker = true
                                }
                            }
                        }
                    }

                    if engine.busy || engine.resultMessage != nil {
                        Card { progressCard }
                    }

                    BigButton(title: engine.busy ? "Working..." : "Install \(selected.name)", symbol: "arrow.down.circle.fill",
                              enabled: canInstall) {
                        engine.install(item: selected, customLink: customLink, localFile: pickedFile)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 30)
            }
        }
        .onAppear {
            if !didInit {
                didInit = true
                if let d = AppItem.all.first(where: { $0.id == defaultApp }) { selected = d }
            }
        }
        .fileImporter(isPresented: $showPicker, allowedContentTypes: [UTType(filenameExtension: "ipa") ?? .data, .data]) { result in
            if case .success(let url) = result {
                let ok = url.startAccessingSecurityScopedResource()
                defer { if ok { url.stopAccessingSecurityScopedResource() } }
                let dest = engine.ipaDir.appendingPathComponent("custom.ipa")
                try? FileManager.default.createDirectory(at: engine.ipaDir, withIntermediateDirectories: true)
                try? FileManager.default.removeItem(at: dest)
                if (try? FileManager.default.copyItem(at: url, to: dest)) != nil {
                    pickedFile = dest
                }
            }
        }
    }

    private var canInstall: Bool {
        if engine.busy || !engine.hasPairingFile { return false }
        if selected.id == "custom" { return pickedFile != nil || !customLink.trimmingCharacters(in: .whitespaces).isEmpty }
        return true
    }

    private func tile(_ item: AppItem) -> some View {
        let on = item == selected
        return Button {
            selected = item
        } label: {
            VStack(alignment: .leading, spacing: 10) {
                ZStack {
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .fill(LinearGradient(colors: item.colors, startPoint: .topLeading, endPoint: .bottomTrailing))
                        .frame(width: 48, height: 48)
                    Image(systemName: item.symbol).font(.system(size: 24)).foregroundColor(.white)
                }
                Text(item.name).font(.headline).foregroundColor(.white)
                Text(item.tagline).font(.caption).foregroundColor(.white.opacity(0.6)).multilineTextAlignment(.leading)
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 20, style: .continuous)
                    .fill(Color.white.opacity(on ? 0.14 : 0.06))
                    .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).stroke(on ? Color.accentColor : Color.white.opacity(0.08), lineWidth: on ? 2 : 1))
            )
        }
        .disabled(engine.busy)
    }

    @ViewBuilder private var progressCard: some View {
        if engine.busy {
            HStack(spacing: 16) {
                ProgressRing(value: engine.progress).frame(width: 78, height: 78)
                VStack(alignment: .leading, spacing: 4) {
                    Text(engine.stage.isEmpty ? "Working" : engine.stage).font(.headline)
                    Text("Keep PairKit open and LocalDevVPN on.").font(.footnote).foregroundColor(.white.opacity(0.6))
                }
            }
        } else if let m = engine.resultMessage {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: engine.resultIsError ? "xmark.octagon.fill" : "checkmark.circle.fill")
                    .font(.system(size: 28)).foregroundColor(engine.resultIsError ? .red : .green)
                Text(m).font(.subheadline).foregroundColor(.white.opacity(0.9))
            }
        }
    }
}
