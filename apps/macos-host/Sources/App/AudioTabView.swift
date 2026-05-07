import SwiftUI
import CoreAudio

struct AudioTabView: View {
    @EnvironmentObject private var viewModel: AppViewModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                // MARK: - Device
                GroupBox("Device") {
                    VStack(alignment: .leading, spacing: 12) {
                        Picker("Input Device", selection: $viewModel.selectedDeviceID) {
                            ForEach(viewModel.devices) { device in
                                Text(device.name).tag(device.id as AudioDeviceID?)
                            }
                        }
                        .onChange(of: viewModel.selectedDeviceID) { _, newValue in
                            if let newValue,
                               let device = viewModel.devices.first(where: { $0.id == newValue }) {
                                viewModel.selectDevice(device)
                            }
                        }

                        Button("Refresh Devices") {
                            viewModel.refreshDevices()
                        }
                    }
                    .padding(8)
                }

                // MARK: - Levels
                GroupBox("Levels") {
                    VStack(alignment: .leading, spacing: 8) {
                        HStack {
                            Text("Input Level")
                            Spacer()
                            Text(String(format: "%.1f%%", Double(viewModel.inputLevel) * 100))
                                .foregroundStyle(.secondary)
                                .monospacedDigit()
                        }
                        GeometryReader { geo in
                            ZStack(alignment: .leading) {
                                RoundedRectangle(cornerRadius: 4)
                                    .fill(Color.secondary.opacity(0.2))
                                RoundedRectangle(cornerRadius: 4)
                                    .fill(levelColor)
                                    .frame(width: geo.size.width * CGFloat(viewModel.inputLevel))
                            }
                        }
                        .frame(height: 12)
                    }
                    .padding(8)
                }

                // MARK: - Gain Controls
                GroupBox("Gain Controls") {
                    VStack(alignment: .leading, spacing: 16) {
                        VStack(alignment: .leading, spacing: 4) {
                            HStack {
                                Text("Input Gain")
                                Spacer()
                                Text(String(format: "%.1f dB", viewModel.inputGainDB))
                                    .foregroundStyle(.secondary)
                                    .monospacedDigit()
                            }
                            Slider(value: $viewModel.inputGainDB, in: -6...18, step: 0.5)
                        }

                        VStack(alignment: .leading, spacing: 4) {
                            HStack {
                                Text("Limiter Threshold")
                                Spacer()
                                Text(String(format: "%.1f dB", viewModel.limiterThresholdDB))
                                    .foregroundStyle(.secondary)
                                    .monospacedDigit()
                            }
                            Slider(value: $viewModel.limiterThresholdDB, in: -18 ... -1, step: 0.5)
                        }
                    }
                    .padding(8)
                }

                // MARK: - Status
                GroupBox("Status") {
                    VStack(alignment: .leading, spacing: 8) {
                        HStack {
                            Text("Engine:")
                            Text(viewModel.statusText)
                                .foregroundStyle(statusColor)
                        }
                        if !viewModel.sharedOutputPath.isEmpty {
                            Text(viewModel.sharedOutputPath)
                                .font(.system(.footnote, design: .monospaced))
                                .textSelection(.enabled)
                                .lineLimit(1)
                        }
                        Text(viewModel.sharedBufferStatusText)
                            .font(.system(.footnote, design: .monospaced))
                            .textSelection(.enabled)
                            .lineLimit(1)
                    }
                    .padding(8)
                }

                Spacer(minLength: 20)
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var levelColor: Color {
        let level = Double(viewModel.inputLevel)
        if level > 0.9 { return .red }
        if level > 0.7 { return .yellow }
        return .green
    }

    private var statusColor: Color {
        switch viewModel.statusText {
        case "Listening": return .green
        case "Failed", "Degraded": return .red
        default: return .secondary
        }
    }
}
