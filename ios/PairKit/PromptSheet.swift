import SwiftUI

struct PromptSheet: View {
    @EnvironmentObject var engine: Engine
    let req: PromptReq
    @State private var text = ""
    @State private var reveal = false
    @AppStorage("rememberEmail") private var rememberEmail = true
    @AppStorage("savedEmail") private var savedEmail = ""

    private var isCertChoice: Bool { req.label.contains("Type 1 to USE") }
    private var isEmail: Bool { req.label.hasPrefix("Apple ID email") }

    var body: some View {
        ZStack {
            Backdrop()
            VStack(alignment: .leading, spacing: 16) {
                Text(title).font(.system(size: 28, weight: .bold, design: .rounded))
                if isCertChoice {
                    Text("Your Apple ID already has a certificate.").foregroundColor(.white.opacity(0.7))
                    BigButton(title: "Use the saved certificate", symbol: "checkmark.shield") { engine.submit("1") }
                    SoftButton(title: "Revoke and make a new one", symbol: "arrow.triangle.2.circlepath") { engine.submit("2") }
                } else {
                    Text(req.label).foregroundColor(.white.opacity(0.7))
                    HStack {
                        Group {
                            if req.secret && !reveal {
                                SecureField("", text: $text)
                            } else {
                                TextField("", text: $text)
                            }
                        }
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(isEmail ? .emailAddress : .default)
                        .submitLabel(.send)
                        .onSubmit { send() }
                        if req.secret {
                            Button { reveal.toggle() } label: { Image(systemName: reveal ? "eye.slash" : "eye") }
                        }
                    }
                    .padding(14)
                    .background(RoundedRectangle(cornerRadius: 14).fill(Color.black.opacity(0.35)))
                    BigButton(title: "Continue", symbol: "arrow.right.circle.fill") { send() }
                    SoftButton(title: "Cancel", symbol: "xmark") { engine.submit("") }
                }
                Spacer()
            }
            .padding(20)
        }
        .interactiveDismissDisabled()
        .onAppear {
            if isEmail && rememberEmail { text = savedEmail }
        }
    }

    private var title: String {
        if isCertChoice { return "Certificate" }
        if isEmail { return "Apple ID" }
        if req.secret { return "Password" }
        if req.label.lowercased().contains("verification") { return "2-step code" }
        return "PairKit"
    }

    private func send() {
        if isEmail && rememberEmail { savedEmail = text }
        engine.submit(text)
    }
}
