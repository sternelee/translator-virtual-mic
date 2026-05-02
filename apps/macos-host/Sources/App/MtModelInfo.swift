import Foundation

enum MtModelFamily: String, CaseIterable {
    case opusMt = "OPUS-MT"
    case nllb200 = "NLLB-200"
}

struct MtModelFile: Identifiable {
    let id = UUID()
    let relativePath: String
    let url: String
    let sizeBytes: Int64
}

struct MtModelInfo: Identifiable {
    let id: String
    let family: MtModelFamily
    let srcLang: String
    let tgtLang: String
    let description: String
    let sizeDisplay: String
    let totalSizeBytes: Int64
    let files: [MtModelFile]
    let hfRepo: String
}
