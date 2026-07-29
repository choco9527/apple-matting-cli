import Foundation
import Vision
import CoreImage
import CoreGraphics
import CoreVideo

// MARK: - Single Image Matting
//
// Exported via @_cdecl so Rust `extern "C"` can call it directly.
//
// Parameters:
//   inputPath  – absolute file path to the source image (UTF-8, null-terminated)
//   outputPath – absolute file path where the result PNG will be written
//
// Returns:
//   0  -> success
//  -1  -> could not load input image
//  -2  -> Vision request failed
//  -3  -> no foreground instance found
//  -4  -> mask generation failed
//  -5  -> blend filter produced no output
//  -6  -> failed to write output PNG
//  -7  -> failed to calculate crop bounds
//  -99 -> macOS version too old (requires 12.0+)

@available(macOS 14.0, *)
@_cdecl("matting_process_image")
public func mattingProcessImage(
    inputPath: UnsafePointer<CChar>,
    outputPath: UnsafePointer<CChar>,
    cropToSubject: Bool
) -> Int32 {
    guard #available(macOS 14.0, *) else { return -99 }

    let input  = String(cString: inputPath)
    let output = String(cString: outputPath)

    // Build a file URL from the raw path string
    let inputURL: URL
    if input.hasPrefix("file://") {
        guard let u = URL(string: input) else { return -1 }
        inputURL = u
    } else {
        inputURL = URL(fileURLWithPath: input)
    }

    guard let ciImage = CIImage(contentsOf: inputURL) else { return -1 }

    // --- Vision: foreground instance mask ---
    let request = VNGenerateForegroundInstanceMaskRequest()
    let handler = VNImageRequestHandler(ciImage: ciImage, options: [:])

    do {
        try handler.perform([request])
    } catch {
        return -2
    }

    guard let result = request.results?.first else { return -3 }

    let maskBuffer: CVPixelBuffer
    do {
        maskBuffer = try result.generateScaledMaskForImage(
            forInstances: result.allInstances,
            from: handler
        )
    } catch {
        return -4
    }

    // --- CIFilter: blend foreground over transparent background ---
    let maskImage = CIImage(cvPixelBuffer: maskBuffer)

    guard let blendFilter = CIFilter(name: "CIBlendWithMask") else { return -5 }
    blendFilter.setValue(ciImage,          forKey: kCIInputImageKey)
    blendFilter.setValue(maskImage,        forKey: kCIInputMaskImageKey)
    blendFilter.setValue(CIImage.empty(),  forKey: kCIInputBackgroundImageKey)

    guard let outputImage = blendFilter.outputImage else { return -5 }

    // --- Optional: crop to subject bounding box ---
    var finalImage = outputImage
    if cropToSubject {
        guard let cropRect = subjectCropRect(
            from: maskBuffer,
            imageExtent: outputImage.extent,
            paddingRatio: 0
        ) else {
            return -7
        }

        let cropped = finalImage.cropped(to: cropRect)
        finalImage = cropped.transformed(
            by: CGAffineTransform(
                translationX: -cropped.extent.origin.x,
                y: -cropped.extent.origin.y
            )
        )
    }

    // --- Write RGBA PNG ---
    let context = CIContext(options: [.workingColorSpace: CGColorSpaceCreateDeviceRGB()])
    let outputURL = URL(fileURLWithPath: output)

    do {
        try context.writePNGRepresentation(
            of: finalImage,
            to: outputURL,
            format: .RGBA8,
            colorSpace: CGColorSpaceCreateDeviceRGB()
        )
    } catch {
        return -6
    }

    return 0
}

@available(macOS 14.0, *)
private func subjectCropRect(
    from maskBuffer: CVPixelBuffer,
    imageExtent: CGRect,
    paddingRatio: CGFloat
) -> CGRect? {
    let width = CVPixelBufferGetWidth(maskBuffer)
    let height = CVPixelBufferGetHeight(maskBuffer)
    guard width > 0, height > 0, !imageExtent.isEmpty else { return nil }

    guard CVPixelBufferLockBaseAddress(maskBuffer, .readOnly) == kCVReturnSuccess else {
        return nil
    }
    defer { CVPixelBufferUnlockBaseAddress(maskBuffer, .readOnly) }

    guard let baseAddress = CVPixelBufferGetBaseAddress(maskBuffer) else { return nil }

    let pixelFormat = CVPixelBufferGetPixelFormatType(maskBuffer)
    let bytesPerRow = CVPixelBufferGetBytesPerRow(maskBuffer)
    let pixels = baseAddress.assumingMemoryBound(to: UInt8.self)

    var minX = width
    var minY = height
    var maxX = -1
    var maxY = -1

    switch pixelFormat {
    case kCVPixelFormatType_OneComponent8:
        for y in 0..<height {
            let row = pixels.advanced(by: y * bytesPerRow)
            for x in 0..<width {
                if row[x] > 0 {
                    updateBounds(x: x, y: y, minX: &minX, minY: &minY, maxX: &maxX, maxY: &maxY)
                }
            }
        }

    case kCVPixelFormatType_OneComponent16Half:
        guard bytesPerRow >= width * MemoryLayout<UInt16>.stride else { return nil }
        let values = baseAddress.assumingMemoryBound(to: UInt16.self)
        let valuesPerRow = bytesPerRow / MemoryLayout<UInt16>.stride
        for y in 0..<height {
            let row = values.advanced(by: y * valuesPerRow)
            for x in 0..<width {
                if row[x] != 0 {
                    updateBounds(x: x, y: y, minX: &minX, minY: &minY, maxX: &maxX, maxY: &maxY)
                }
            }
        }

    case kCVPixelFormatType_OneComponent32Float:
        guard bytesPerRow >= width * MemoryLayout<Float32>.stride else { return nil }
        let values = baseAddress.assumingMemoryBound(to: Float32.self)
        let valuesPerRow = bytesPerRow / MemoryLayout<Float32>.stride
        for y in 0..<height {
            let row = values.advanced(by: y * valuesPerRow)
            for x in 0..<width {
                if row[x] > 0 {
                    updateBounds(x: x, y: y, minX: &minX, minY: &minY, maxX: &maxX, maxY: &maxY)
                }
            }
        }

    case kCVPixelFormatType_32BGRA, kCVPixelFormatType_32RGBA:
        guard bytesPerRow >= width * 4 else { return nil }
        for y in 0..<height {
            let row = pixels.advanced(by: y * bytesPerRow)
            for x in 0..<width {
                let offset = x * 4
                if row[offset] > 0 || row[offset + 1] > 0 || row[offset + 2] > 0 {
                    updateBounds(x: x, y: y, minX: &minX, minY: &minY, maxX: &maxX, maxY: &maxY)
                }
            }
        }

    case kCVPixelFormatType_32ARGB:
        guard bytesPerRow >= width * 4 else { return nil }
        for y in 0..<height {
            let row = pixels.advanced(by: y * bytesPerRow)
            for x in 0..<width {
                let offset = x * 4
                if row[offset + 1] > 0 || row[offset + 2] > 0 || row[offset + 3] > 0 {
                    updateBounds(x: x, y: y, minX: &minX, minY: &minY, maxX: &maxX, maxY: &maxY)
                }
            }
        }

    default:
        return nil
    }

    guard maxX >= minX, maxY >= minY else { return nil }

    let scaleX = imageExtent.width / CGFloat(width)
    let scaleY = imageExtent.height / CGFloat(height)

    let subjectRect = CGRect(
        x: imageExtent.origin.x + CGFloat(minX) * scaleX,
        y: imageExtent.origin.y + CGFloat(height - maxY - 1) * scaleY,
        width: CGFloat(maxX - minX + 1) * scaleX,
        height: CGFloat(maxY - minY + 1) * scaleY
    )

    let paddingX = subjectRect.width * paddingRatio
    let paddingY = subjectRect.height * paddingRatio
    let paddedRect = subjectRect.insetBy(dx: -paddingX, dy: -paddingY)
    let clampedRect = paddedRect.intersection(imageExtent).integral

    return clampedRect.isEmpty ? nil : clampedRect
}

@available(macOS 14.0, *)
private func updateBounds(
    x: Int,
    y: Int,
    minX: inout Int,
    minY: inout Int,
    maxX: inout Int,
    maxY: inout Int
) {
    minX = min(minX, x)
    minY = min(minY, y)
    maxX = max(maxX, x)
    maxY = max(maxY, y)
}
