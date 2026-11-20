import SwiftUI

@main
struct PairKitApp: App {
    @StateObject private var engine = Engine.shared
    @AppStorage("accent") private var accent = 0

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(engine)
                .tint(Theme.accents[min(accent, Theme.accents.count - 1)].color)
                .preferredColorScheme(.dark)
        }
    }
}

struct RootView: View {
    @EnvironmentObject var engine: Engine

    var body: some View {
        TabView {
            PairView()
                .tabItem { Label("Pair", systemImage: "link") }
            InstallView()
                .tabItem { Label("Install", systemImage: "arrow.down.app.fill") }
            LogView()
                .tabItem { Label("Log", systemImage: "text.alignleft") }
            SettingsView()
                .tabItem { Label("Settings", systemImage: "slider.horizontal.3") }
        }
        .sheet(item: $engine.prompt) { p in
            PromptSheet(req: p)
        }
    }
}
