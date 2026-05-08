import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var store: SecureChatStore

    var body: some View {
        Group {
            if store.isReady {
                MessengerView()
            } else {
                LoginView()
            }
        }
        .overlay(alignment: .bottom) {
            if let errorMessage = store.errorMessage {
                ErrorBanner(message: errorMessage)
                    .padding()
            }
        }
    }
}

private struct ErrorBanner: View {
    let message: String

    var body: some View {
        Label(message, systemImage: "exclamationmark.triangle")
            .font(.callout)
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
            .shadow(radius: 8)
    }
}
