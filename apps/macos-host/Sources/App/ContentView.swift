import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var viewModel: AppViewModel
    @State private var selectedTab: Int = 0

    var body: some View {
        NavigationStack {
            TabView(selection: $selectedTab) {
                AudioTabView()
                    .tabItem {
                        Label("Audio", systemImage: "mic")
                    }
                    .tag(0)

                ProviderTabView()
                    .tabItem {
                        Label("Provider", systemImage: "network")
                    }
                    .tag(1)

                ModelsTabView()
                    .tabItem {
                        Label("Models", systemImage: "cpu")
                    }
                    .tag(2)

                TtsTabView()
                    .tabItem {
                        Label("TTS", systemImage: "speaker.wave.2")
                    }
                    .tag(3)

                DebugTabView()
                    .tabItem {
                        Label("Debug", systemImage: "ant")
                    }
                    .tag(4)
            }
            .navigationTitle("Translator Virtual Mic")
            .toolbar {
                ToolbarItem(placement: .automatic) {
                    HStack(spacing: 12) {
                        // Compact input level meter
                        HStack(spacing: 4) {
                            Image(systemName: "mic")
                                .font(.caption)
                            GeometryReader { geo in
                                ZStack(alignment: .leading) {
                                    RoundedRectangle(cornerRadius: 1)
                                        .fill(Color.secondary.opacity(0.2))
                                    RoundedRectangle(cornerRadius: 1)
                                        .fill(inputLevelColor)
                                        .frame(width: max(0, geo.size.width * CGFloat(viewModel.inputLevel)))
                                }
                            }
                            .frame(width: 60, height: 8)
                        }

                        Divider()
                            .frame(height: 20)

                        // Start / Stop
                        Button("Start") {
                            viewModel.startEngine()
                        }
                        .buttonStyle(.borderedProminent)

                        Button("Stop") {
                            viewModel.stopEngine()
                        }
                        .buttonStyle(.bordered)

                        Divider()
                            .frame(height: 20)

                        // Plugin status
                        HStack(spacing: 8) {
                            Circle()
                                .fill(viewModel.pluginInstalled ? Color.green : Color.red)
                                .frame(width: 8, height: 8)
                            Text(viewModel.pluginInstalled ? "Driver Ready" : "Driver Missing")
                                .font(.caption)

                            if viewModel.pluginInstallInProgress {
                                ProgressView()
                                    .controlSize(.small)
                            } else if viewModel.pluginInstalled {
                                Button("Uninstall") {
                                    viewModel.uninstallPlugin()
                                }
                                .buttonStyle(.bordered)
                                .tint(.red)
                                .controlSize(.small)
                            } else {
                                Button("Install Driver") {
                                    viewModel.installPlugin()
                                }
                                .buttonStyle(.borderedProminent)
                                .controlSize(.small)
                            }
                        }

                        if !viewModel.pluginInstallError.isEmpty {
                            Text(viewModel.pluginInstallError)
                                .font(.caption)
                                .foregroundStyle(.red)
                                .lineLimit(1)
                        }
                    }
                    .padding(.horizontal, 12)
                }
            }
        }
    }

    private var inputLevelColor: Color {
        let level = Double(viewModel.inputLevel)
        if level > 0.9 { return .red }
        if level > 0.7 { return .yellow }
        return .green
    }
}
