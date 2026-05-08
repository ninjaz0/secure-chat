import SwiftUI

struct RootView: View {
    @EnvironmentObject private var store: SecureChatStore

    var body: some View {
        Group {
            if store.isReady {
                ConversationListView()
            } else {
                LoginView()
            }
        }
        .overlay(alignment: .bottom) {
            if let message = store.errorMessage {
                Text(message)
                    .font(.footnote)
                    .foregroundStyle(.white)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .background(.red, in: Capsule())
                    .padding()
            }
        }
    }
}
