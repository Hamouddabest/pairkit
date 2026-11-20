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
    @State private var importMsg: String?
    @State private var pickedName = ""

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
                                SoftButton(title: pickedFile == nil ? "Or choose an IPA file" : "File: \(pickedName)", symbol: pickedFile == nil ? "doc" : "checkmark.circle.fill") {
                                    showPicker = true
                                }
                                if let m = importMsg {
                                    Text(m).font(.footnote).foregroundColor(.orange)
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
        .fileImporter(isPresented: $showPicker, allowedContentTypes: [.item, .data]) { result in
            switch result {
            case .success(let url): importFile(url)
            case .failure(let e): importMsg = "Couldn't open that file: \(e.localizedDescription)"
            }
        }
    }

    // Copies the chosen file into the app (downloading it first if it only lives in iCloud).
    private func importFile(_ url: URL) {
        importMsg = "Importing \(url.lastPathComponent)..."
        let dest = engine.ipaDir.appendingPathComponent("custom.ipa")
        let dir = engine.ipaDir
        DispatchQueue.global(qos: .userInitiated).async {
            let ok = url.startAccessingSecurityScopedResource()
            defer { if ok { url.stopAccessingSecurityScopedResource() } }
            var coordErr: NSError?
            var copyErr: Error?
            NSFileCoordinator().coordinate(readingItemAt: url, options: [], error: &coordErr) { readURL in
                do {
                    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
                    try? FileManager.default.removeItem(at: dest)
                    try FileManager.default.copyItem(at: readURL, to: dest)
                } catch {
                    copyErr = error
                }
            }
            DispatchQueue.main.async {
                if let e = copyErr ?? coordErr {
                    importMsg = "Couldn't import: \(e.localizedDescription)"
                } else {
                    pickedFile = dest
                    pickedName = url.lastPathComponent
                    importMsg = nil
                }
            }
        }
    }

    private var folderIPAs: [URL] {
        let fm = FileManager.default
        let items = (try? fm.contentsOfDirectory(at: engine.ipaDir, includingPropertiesForKeys: nil)) ?? []
        return items.filter { $0.pathExtension.lowercased() == "ipa" && !["sidestore.ipa", "altstore.ipa", "livecontainer.ipa"].contains($0.lastPathComponent.lowercased()) }
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
