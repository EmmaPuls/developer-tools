import AppKit
import Foundation
import Vision

// Prints the text found in an image, one line per observation, top to bottom.
guard CommandLine.arguments.count == 2 else {
    FileHandle.standardError.write("usage: capture-text-ocr <image>\n".data(using: .utf8)!)
    exit(2)
}

guard let image = NSImage(contentsOfFile: CommandLine.arguments[1]),
      let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
    FileHandle.standardError.write("could not read image\n".data(using: .utf8)!)
    exit(1)
}

let request = VNRecognizeTextRequest()
request.recognitionLevel = .accurate
request.usesLanguageCorrection = true
try VNImageRequestHandler(cgImage: cgImage).perform([request])

// Vision doesn't return observations in reading order, so sort by height.
let observations = (request.results ?? []).sorted { $0.boundingBox.minY > $1.boundingBox.minY }
print(observations.compactMap { $0.topCandidates(1).first?.string }.joined(separator: "\n"))
