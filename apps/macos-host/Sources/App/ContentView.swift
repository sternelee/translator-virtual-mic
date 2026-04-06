import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var viewModel: AppViewModel

    var body: some View {
        NavigationSplitView {
            List {
                Section("Control") {
                    Text("Status: \(viewModel.statusText)")
                    HStack {
                        Text("Input Level")
                        ProgressView(value: Double(viewModel.inputLevel), total: 1.0)
                    }
                    Picker("Target Language", selection: $viewModel.targetLanguage) {
                        Text("English").tag("en")
                        Text("Chinese").tag("zh")
                        Text("Japanese").tag("ja")
                    }
                    Button("Refresh Devices") {
                        viewModel.refreshDevices()
                    }
                    HStack {
                        Button("Start") { viewModel.startEngine() }
                        Button("Stop") { viewModel.stopEngine() }
                    }
                }

                Section("Input Devices") {
                    ForEach(viewModel.devices) { device in
                        HStack {
                            Text(device.name)
                            Spacer()
                            if viewModel.selectedDeviceID == device.id {
                                Text("Selected")
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .contentShape(Rectangle())
                        .onTapGesture {
                            viewModel.selectDevice(device)
                        }
                    }
                }
            }
            .navigationTitle("Translator Virtual Mic")
        } detail: {
            VStack(alignment: .leading, spacing: 12) {
                Text(viewModel.microphonePermissionGranted ? "Microphone Access Ready" : "Microphone Access Pending")
                    .font(.headline)
                Text("Logs")
                    .font(.title3)
                if !viewModel.sharedOutputPath.isEmpty {
                    Text(viewModel.sharedOutputPath)
                        .font(.system(.footnote, design: .monospaced))
                        .textSelection(.enabled)
                }
                Text(viewModel.sharedBufferStatusText)
                    .font(.system(.footnote, design: .monospaced))
                    .textSelection(.enabled)
                Text(viewModel.metricsJSON)
                    .font(.system(.footnote, design: .monospaced))
                    .textSelection(.enabled)
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 8) {
                        ForEach(Array(viewModel.logLines.enumerated()), id: \.offset) { _, line in
                            Text(line)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                }
            }
            .padding()
        }
    }
}
