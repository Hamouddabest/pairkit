import SwiftUI

struct AccentChoice: Identifiable {
    let id: Int
    let name: String
    let color: Color
}

enum Theme {
    static let accents: [AccentChoice] = [
        AccentChoice(id: 0, name: "Aqua", color: Color(red: 0.20, green: 0.75, blue: 0.90)),
        AccentChoice(id: 1, name: "Violet", color: Color(red: 0.62, green: 0.45, blue: 0.98)),
        AccentChoice(id: 2, name: "Coral", color: Color(red: 1.00, green: 0.45, blue: 0.40)),
        AccentChoice(id: 3, name: "Lime", color: Color(red: 0.55, green: 0.90, blue: 0.35)),
        AccentChoice(id: 4, name: "Gold", color: Color(red: 1.00, green: 0.78, blue: 0.25))
    ]
}

struct Backdrop: View {
    var body: some View {
        ZStack {
            Color(red: 0.04, green: 0.05, blue: 0.09)
            RadialGradient(colors: [Color.accentColor.opacity(0.35), .clear], center: .topLeading, startRadius: 10, endRadius: 420)
            RadialGradient(colors: [Color.purple.opacity(0.22), .clear], center: .bottomTrailing, startRadius: 10, endRadius: 460)
        }
        .ignoresSafeArea()
    }
}

struct Card<Content: View>: View {
    let content: Content
    init(@ViewBuilder _ c: () -> Content) { content = c() }
    var body: some View {
        content
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 22, style: .continuous)
                    .fill(Color.white.opacity(0.07))
                    .overlay(RoundedRectangle(cornerRadius: 22, style: .continuous).stroke(Color.white.opacity(0.10), lineWidth: 1))
            )
    }
}

struct BigButton: View {
    let title: String
    var symbol: String? = nil
    var enabled = true
    let action: () -> Void
    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                if let s = symbol { Image(systemName: s) }
                Text(title).fontWeight(.semibold)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 15)
            .background(
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .fill(LinearGradient(colors: [Color.accentColor, Color.accentColor.opacity(0.65)], startPoint: .topLeading, endPoint: .bottomTrailing))
            )
            .foregroundColor(.black.opacity(0.85))
            .opacity(enabled ? 1 : 0.4)
        }
        .disabled(!enabled)
    }
}

struct SoftButton: View {
    let title: String
    var symbol: String? = nil
    let action: () -> Void
    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                if let s = symbol { Image(systemName: s) }
                Text(title).fontWeight(.medium)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 13)
            .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color.white.opacity(0.10)))
            .foregroundColor(.white)
        }
    }
}

struct ProgressRing: View {
    let value: Double
    var body: some View {
        ZStack {
            Circle().stroke(Color.white.opacity(0.10), lineWidth: 10)
            Circle()
                .trim(from: 0, to: max(0.02, min(1, value)))
                .stroke(AngularGradient(colors: [Color.accentColor.opacity(0.4), Color.accentColor], center: .center), style: StrokeStyle(lineWidth: 10, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .animation(.easeOut(duration: 0.4), value: value)
            Text("\(Int(value * 100))%").font(.system(size: 22, weight: .bold, design: .rounded))
        }
    }
}
