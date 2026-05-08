import SwiftUI

struct SidebarView: View {
    @Binding var selection: SidebarSelection?

    var body: some View {
        List(selection: $selection) {
            Section("SecureChat") {
                ForEach(SidebarSelection.allCases) { item in
                    HStack(spacing: 10) {
                        Image(systemName: item.systemImage)
                            .foregroundStyle(.secondary)
                            .frame(width: 18)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(item.title)
                                .lineLimit(1)
                            Text(item.detail)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                    .tag(item)
                }
            }
        }
        .listStyle(.sidebar)
        .navigationTitle("SecureChat")
    }
}

