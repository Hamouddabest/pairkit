import SwiftUI
import AVFoundation
import UniformTypeIdentifiers

struct PromptReq: Identifiable {
    let id = UUID()
    let label: String
    let secret: Bool
    let reply: (String) -> Void
}

enum PairPhase: Equatable {
    case idle
    case waiting
    case code(String)
    case done
    case failed(String)
}

struct AppItem: Identifiable, Equatable {
    let id: String
    let name: String
    let tagline: String
    let symbol: String
    let colors: [Color]
    static let all: [AppItem] = [
        AppItem(id: "sidestore", name: "SideStore", tagline: "Refreshes apps on-device", symbol: "arrow.triangle.2.circlepath.circle.fill", colors: [.purple, .indigo]),
        AppItem(id: "altstore", name: "AltStore", tagline: "The classic sideloading store", symbol: "diamond.fill", colors: [.teal, .cyan]),
        AppItem(id: "livecontainer", name: "LiveContainer", tagline: "Run apps inside one app", symbol: "square.stack.3d.up.fill", colors: [.orange, .pink]),
        AppItem(id: "custom", name: "Custom IPA", tagline: "Your own link or file", symbol: "link.circle.fill", colors: [.green, .mint])
    ]
}

// MARK: - C callbacks (no captures allowed, the Engine comes in through ctx)

private func engine(_ ctx: UnsafeMutableRawPointer?) -> Engine {
    Unmanaged<Engine>.fromOpaque(ctx!).takeUnretainedValue()
}

private let logCB: @convention(c) (UnsafePointer<CChar>?, UnsafeMutableRawPointer?) -> Void = { line, ctx in
    guard let line = line else { return }
    let s = String(cString: line)
    let e = engine(ctx)
    DispatchQueue.main.async { e.handleLog(s) }
}

private let pinCB: @convention(c) (UnsafePointer<CChar>?, UnsafeMutableRawPointer?) -> Void = { pin, ctx in
    guard let pin = pin else { return }
    let s = String(cString: pin)
    let e = engine(ctx)
    DispatchQueue.main.async { e.pairPhase = .code(s) }
}

private let doneCB: @convention(c) (Int32, UnsafePointer<CChar>?, UnsafeMutableRawPointer?) -> Void = { ok, msg, ctx in
    let m = msg.map { String(cString: $0) } ?? ""
    let e = engine(ctx)
    DispatchQueue.main.async { e.pairingFinished(ok: ok != 0, message: m) }
}

private final class AnswerBox { var value = "" }

private let askCB: @convention(c) (UnsafePointer<CChar>?, Int32, UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? = { label, secret, ctx in
    let l = label.map { String(cString: $0) } ?? ""
    let e = engine(ctx)
    let sem = DispatchSemaphore(value: 0)
    let box = AnswerBox()
    DispatchQueue.main.async {
        e.prompt = PromptReq(label: l, secret: secret != 0) { a in
            box.value = a
            sem.signal()
        }
    }
    sem.wait()
    return strdup(box.value)
}

// MARK: - Engine

final class Engine: NSObject, ObservableObject {
    static let shared = Engine()

    @Published var log: [String] = []
    @Published var pairPhase: PairPhase = .idle
    @Published var hasPairingFile = false
    @Published var prompt: PromptReq?
    @Published var busy = false
    @Published var stage = ""
    @Published var progress: Double = 0
    @Published var resultMessage: String?
    @Published var resultIsError = false

    private var service: NetService?
    private var player: AVAudioPlayer?
    let docs: URL

    override init() {
        docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        super.init()
        // The Rust side stores everything under $HOME/.pairkit
        setenv("HOME", docs.path, 1)
        refreshPairingFile()
    }

    var pairkitDir: URL { docs.appendingPathComponent(".pairkit") }
    var pairingFileURL: URL { pairkitDir.appendingPathComponent("rp_pairing_file.plist") }
    var ipaDir: URL { docs.appendingPathComponent("ipas") }
    var ctx: UnsafeMutableRawPointer { Unmanaged.passUnretained(self).toOpaque() }

    func refreshPairingFile() {
        hasPairingFile = FileManager.default.fileExists(atPath: pairingFileURL.path)
    }

    func handleLog(_ s: String) {
        log.append(s)
        if log.count > 500 { log.removeFirst(100) }
        if let r = s.range(of: #"(signing|installing) (\d+)%"#, options: .regularExpression) {
            let part = String(s[r])
            let pieces = part.split(separator: " ")
            if pieces.count == 2, let n = Double(pieces[1].dropLast()) {
                let isInstall = pieces[0] == "installing"
                stage = isInstall ? "Installing" : "Signing"
                progress = isInstall ? 0.6 + n / 100 * 0.4 : 0.35 + n / 100 * 0.25
            }
            return
        }
        if s.hasPrefix("== ") {
            stage = s.replacingOccurrences(of: "==", with: "").trimmingCharacters(in: .whitespaces)
        }
    }

    // MARK: Pairing

    func startPairing() {
        stopPairing()
        pairPhase = .waiting
        guard let p = pk_pair_start(logCB, pinCB, doneCB, ctx) else {
            pairPhase = .failed("Could not start pairing.")
            return
        }
        let json = String(cString: p)
        pk_free(p)
        guard let data = json.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let identifier = obj["identifier"] as? String,
              let port = obj["port"] as? Int,
              let txt = obj["txt"] as? [String: String] else {
            pairPhase = .failed("Unexpected response from the pairing engine.")
            return
        }
        let svc = NetService(domain: "", type: "_remotepairing-pairable-host._tcp.", name: identifier, port: Int32(port))
        var d: [String: Data] = [:]
        for (k, v) in txt { d[k] = Data(v.utf8) }
        svc.setTXTRecord(NetService.data(fromTXT: d))
        svc.publish()
        service = svc
        if UserDefaults.standard.object(forKey: "keepAwakeAudio") as? Bool ?? true { startAwakeAudio() }
    }

    func stopPairing() {
        pk_pair_stop()
        service?.stop()
        service = nil
        stopAwakeAudio()
        if pairPhase != .done { pairPhase = .idle }
    }

    func pairingFinished(ok: Bool, message: String) {
        service?.stop()
        service = nil
        stopAwakeAudio()
        refreshPairingFile()
        pairPhase = ok ? .done : .failed(message)
    }

    // A silent loop keeps iOS from suspending the app while you are in Settings.
    private func startAwakeAudio() {
        do {
            try AVAudioSession.sharedInstance().setCategory(.playback, options: [.mixWithOthers])
            try AVAudioSession.sharedInstance().setActive(true)
            let p = try AVAudioPlayer(data: Engine.silentWav(seconds: 5))
            p.numberOfLoops = -1
            p.volume = 0
            p.play()
            player = p
        } catch {
            handleLog("Could not start keep-awake audio: \(error.localizedDescription)")
        }
    }

    private func stopAwakeAudio() {
        player?.stop()
        player = nil
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    static func silentWav(seconds: Int) -> Data {
        let rate = 8000
        let n = rate * seconds
        var d = Data()
        func u32(_ v: Int) { var x = UInt32(v).littleEndian; d.append(Data(bytes: &x, count: 4)) }
        func u16(_ v: Int) { var x = UInt16(v).littleEndian; d.append(Data(bytes: &x, count: 2)) }
        d.append("RIFF".data(using: .ascii)!)
        u32(36 + n)
        d.append("WAVEfmt ".data(using: .ascii)!)
        u32(16); u16(1); u16(1); u32(rate); u32(rate); u16(1); u16(8)
        d.append("data".data(using: .ascii)!)
        u32(n)
        d.append(Data(repeating: 0x80, count: n))
        return d
    }

    // MARK: Install

    func install(item: AppItem, customLink: String, localFile: URL?) {
        guard !busy else { return }
        let defaults = UserDefaults.standard
        busy = true
        resultMessage = nil
        resultIsError = false
        progress = 0
        stage = "Starting"
        log.removeAll()
        if defaults.object(forKey: "keepScreenOn") as? Bool ?? true { UIApplication.shared.isIdleTimerDisabled = true }
        let certMode = defaults.string(forKey: "certMode") ?? "ask"
        let reuseExt = defaults.object(forKey: "extReuse") as? Bool ?? true

        Task {
            do {
                try FileManager.default.createDirectory(at: ipaDir, withIntermediateDirectories: true)
                let ipa: URL
                if item.id == "custom", let f = localFile {
                    ipa = f
                } else {
                    await MainActor.run { self.stage = "Finding the latest version" }
                    let url = try await Resolver.url(for: item, custom: customLink)
                    await MainActor.run { self.handleLog("Downloading \(url.absoluteString)"); self.stage = "Downloading" }
                    let dest = ipaDir.appendingPathComponent("\(item.id).ipa")
                    ipa = try await Downloader().download(url, to: dest) { p in
                        DispatchQueue.main.async { self.progress = p * 0.3 }
                    }
                }
                await MainActor.run { self.progress = 0.3; self.stage = "Preparing" }
                let opts = "{\"cert_mode\":\"\(certMode)\",\"reuse_main_ext\":\(reuseExt)}"
                let err: String? = await withCheckedContinuation { cont in
                    DispatchQueue.global(qos: .userInitiated).async {
                        let r = pk_install(ipa.path, opts, logCB, askCB, self.ctx)
                        if let r = r {
                            let s = String(cString: r)
                            pk_free(r)
                            cont.resume(returning: s)
                        } else {
                            cont.resume(returning: nil)
                        }
                    }
                }
                await MainActor.run {
                    self.busy = false
                    UIApplication.shared.isIdleTimerDisabled = false
                    if let err = err {
                        self.resultIsError = true
                        self.resultMessage = err
                    } else {
                        self.progress = 1
                        self.stage = "Done"
                        self.resultMessage = "\(item.name) is installed. If it won't open, trust your Apple ID in Settings > General > VPN & Device Management."
                    }
                }
            } catch {
                await MainActor.run {
                    self.busy = false
                    UIApplication.shared.isIdleTimerDisabled = false
                    self.resultIsError = true
                    self.resultMessage = error.localizedDescription
                }
            }
        }
    }

    // MARK: Housekeeping

    func deletePairingFile() {
        try? FileManager.default.removeItem(at: pairingFileURL)
        pairPhase = .idle
        refreshPairingFile()
    }

    func clearCertificateKeys() {
        try? FileManager.default.removeItem(at: pairkitDir.appendingPathComponent("keys"))
    }

    func clearDownloads() {
        try? FileManager.default.removeItem(at: ipaDir)
    }

    func submit(_ answer: String) {
        let p = prompt
        prompt = nil
        p?.reply(answer)
    }
}

// MARK: - Latest-version lookup

enum Resolver {
    struct Failure: LocalizedError { let errorDescription: String? }

    static func json(_ s: String) async -> Any? {
        guard let u = URL(string: s) else { return nil }
        var req = URLRequest(url: u)
        req.setValue("PairKit", forHTTPHeaderField: "User-Agent")
        guard let (d, _) = try? await URLSession.shared.data(for: req) else { return nil }
        return try? JSONSerialization.jsonObject(with: d)
    }

    static func github(_ repo: String, prefer: String) async -> URL? {
        if let obj = await json("https://api.github.com/repos/\(repo)/releases/latest") as? [String: Any],
           let assets = obj["assets"] as? [[String: Any]] {
            let pick = assets.first { ($0["name"] as? String) == prefer }
                ?? assets.first { ($0["name"] as? String)?.hasSuffix(".ipa") == true }
            if let s = pick?["browser_download_url"] as? String, let u = URL(string: s) { return u }
        }
        return URL(string: "https://github.com/\(repo)/releases/latest/download/\(prefer)")
    }

    static func altstore() async -> URL? {
        if let obj = await json("https://cdn.altstore.io/file/altstore/apps.json") as? [String: Any],
           let apps = obj["apps"] as? [[String: Any]],
           let app = apps.first(where: { ($0["bundleIdentifier"] as? String) == "com.rileytestut.AltStore" }) {
            if let versions = app["versions"] as? [[String: Any]],
               let s = versions.first?["downloadURL"] as? String, let u = URL(string: s) { return u }
            if let s = app["downloadURL"] as? String, let u = URL(string: s) { return u }
        }
        return URL(string: "https://cdn.altstore.io/file/altstore/altstore.ipa")
    }

    static func url(for item: AppItem, custom: String) async throws -> URL {
        let u: URL?
        switch item.id {
        case "sidestore": u = await github("SideStore/SideStore", prefer: "SideStore.ipa")
        case "altstore": u = await altstore()
        case "livecontainer": u = await github("LiveContainer/LiveContainer", prefer: "LiveContainer.ipa")
        default: u = URL(string: custom.trimmingCharacters(in: .whitespaces))
        }
        guard let url = u, url.scheme?.hasPrefix("http") == true else {
            throw Failure(errorDescription: "Couldn't find a download link. Check your connection or the link you pasted.")
        }
        return url
    }
}

// MARK: - Download with progress

final class Downloader: NSObject, URLSessionDownloadDelegate {
    private var cont: CheckedContinuation<URL, Error>?
    private var dest: URL!
    private var onProgress: ((Double) -> Void)?
    private var session: URLSession?

    func download(_ url: URL, to dest: URL, progress: @escaping (Double) -> Void) async throws -> URL {
        self.dest = dest
        self.onProgress = progress
        return try await withCheckedThrowingContinuation { c in
            self.cont = c
            let s = URLSession(configuration: .default, delegate: self, delegateQueue: nil)
            self.session = s
            s.downloadTask(with: url).resume()
        }
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64, totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        if totalBytesExpectedToWrite > 0 { onProgress?(Double(totalBytesWritten) / Double(totalBytesExpectedToWrite)) }
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        let code = (downloadTask.response as? HTTPURLResponse)?.statusCode ?? 200
        guard code < 400 else {
            finish(.failure(Resolver.Failure(errorDescription: "Download failed (HTTP \(code)).")))
            return
        }
        do {
            try? FileManager.default.removeItem(at: dest)
            try FileManager.default.moveItem(at: location, to: dest)
            finish(.success(dest))
        } catch {
            finish(.failure(error))
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        if let error = error { finish(.failure(error)) }
    }

    private func finish(_ r: Result<URL, Error>) {
        guard let c = cont else { return }
        cont = nil
        session?.finishTasksAndInvalidate()
        switch r {
        case .success(let u): c.resume(returning: u)
        case .failure(let e): c.resume(throwing: e)
        }
    }
}
