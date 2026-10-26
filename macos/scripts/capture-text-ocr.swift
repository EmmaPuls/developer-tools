import AppKit
import Foundation
import Vision

// Prints the text found in an image, one line per observation, top to bottom.
// Timings go to stderr so stdout stays clean for pbcopy.
guard CommandLine.arguments.count == 2 else {
    FileHandle.standardError.write("usage: capture-text-ocr <image>\n".data(using: .utf8)!)
    exit(2)
}

let started = Date()

guard let image = NSImage(contentsOfFile: CommandLine.arguments[1]),
      let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
    FileHandle.standardError.write("could not read image\n".data(using: .utf8)!)
    exit(1)
}

let loaded = Date()

let request = VNRecognizeTextRequest()
request.recognitionLevel = .accurate
request.usesLanguageCorrection = true
try VNImageRequestHandler(cgImage: cgImage).perform([request])

let recognized = Date()

// Vision doesn't return observations in reading order, so sort by height.
let observations = (request.results ?? []).sorted { $0.boundingBox.minY > $1.boundingBox.minY }
print(observations.compactMap { $0.topCandidates(1).first?.string }.joined(separator: "\n"))

let timing = String(
    format: "ocr size=%dx%d load=%.0fms recognize=%.0fms lines=%d\n",
    cgImage.width, cgImage.height,
    loaded.timeIntervalSince(started) * 1000,
    recognized.timeIntervalSince(loaded) * 1000,
    observations.count
)
FileHandle.standardError.write(timing.data(using: .utf8)!)
