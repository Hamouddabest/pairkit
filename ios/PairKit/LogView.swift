import SwiftUI

struct LogView: View {
    @EnvironmentObject var engine: Engine

    var body: some View {
        ZStack {
            Backdrop()
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    Text("Log").font(.system(size: 38, weight: .bold, design: .rounded))
                    Spacer()
                    Button {
                        UIPasteboard.general.string = engine.log.joined(separator: "\n")
                    } label: { Image(systemName: "doc.on.doc") }
                    Button { engine.log.removeAll() } label: { Image(systemName: "trash") }
                }
                .font(.title3)
                .padding(.top, 8)
                ScrollView {
                    Text(engine.log.isEmpty ? "Nothing yet." : engine.log.joined(separator: "\n"))
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundColor(.white.opacity(0.85))
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .textSelection(.enabled)
                        .padding(14)
                }
                .background(RoundedRectangle(cornerRadius: 18).fill(Color.black.opacity(0.35)))
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 12)
        }
    }
}
