import SwiftUI

@main
struct TranslatorVirtualMicApp: App {
    @StateObject private var viewModel = AppViewModel()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(viewModel)
                .frame(minWidth: 840, minHeight: 560)
        }
    }
}
