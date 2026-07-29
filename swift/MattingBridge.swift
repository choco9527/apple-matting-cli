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
//   background – normalized RGBA components; alpha 0 preserves transparency
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
//  -99 -> macOS version too old (requires 14.0+)

private let outputColorSpace = CGColorSpaceCreateDeviceRGB()
// CIContext is expensive to construct and safe to reuse for concurrent renders.
private let sharedRenderContext = CIContext(options: [
    .workingColorSpace: outputColorSpace,
    .cacheIntermediates: false,
])
private let recommendedMaxPixels: CGFloat = 32_000_000

private enum MattingFailure: Int32, Error {
    case loadFailed = -1
    case visionRequestFailed = -2
    case noForeground = -3
    case maskFailed = -4
    case blendFailed = -5
    case writeFailed = -6
    case cropBoundsFailed = -7
}

@available(macOS 14.0, *)
@_cdecl("matting_process_image")
public func mattingProcessImage(
    inputPath: UnsafePointer<CChar>,
    outputPath: UnsafePointer<CChar>,
    cropToSubject: Bool,
    backgroundRed: Float,
    backgroundGreen: Float,
    backgroundBlue: Float,
    backgroundAlpha: Float
) -> Int32 {
    guard #available(macOS 14.0, *) else { return -99 }

    return autoreleasepool {
        do {
            try processImage(
                input: String(cString: inputPath),
                output: String(cString: outputPath),
                cropToSubject: cropToSubject,
                backgroundColor: backgroundColor(
                    red: backgroundRed,
                    green: backgroundGreen,
                    blue: backgroundBlue,
                    alpha: backgroundAlpha
                )
            )
            return 0
        } catch let failure as MattingFailure {
            return failure.rawValue
        } catch {
            return MattingFailure.visionRequestFailed.rawValue
        }
    }
}

@available(macOS 14.0, *)
private func processImage(
    input: String,
    output: String,
    cropToSubject: Bool,
    backgroundColor: CIColor?
) throws {
    let inputURL = try resolveInputURL(input)
    guard let inputImage = CIImage(contentsOf: inputURL) else {
        throw MattingFailure.loadFailed
    }
    warnForLargeImage(inputImage, path: input)

    let maskBuffer = try generateForegroundMask(inputImage)
    let outputImage = try blendForeground(
        inputImage,
        maskBuffer: maskBuffer,
        backgroundColor: backgroundColor
    )
    let finalImage = try cropIfNeeded(outputImage, maskBuffer: maskBuffer, enabled: cropToSubject)
    try writePNG(finalImage, output: output)
}

private func resolveInputURL(_ input: String) throws -> URL {
    if input.hasPrefix("file://") {
        guard let url = URL(string: input) else { throw MattingFailure.loadFailed }
        return url
    }
    return URL(fileURLWithPath: input)
}

@available(macOS 14.0, *)
private func generateForegroundMask(_ inputImage: CIImage) throws -> CVPixelBuffer {
    let request = VNGenerateForegroundInstanceMaskRequest()
    let handler = VNImageRequestHandler(ciImage: inputImage, options: [:])

    do {
        try handler.perform([request])
    } catch {
        throw MattingFailure.visionRequestFailed
    }

    guard let result = request.results?.first else { throw MattingFailure.noForeground }
    do {
        return try result.generateScaledMaskForImage(
            forInstances: result.allInstances,
            from: handler
        )
    } catch {
        throw MattingFailure.maskFailed
    }
}

private func backgroundColor(red: Float, green: Float, blue: Float, alpha: Float) -> CIColor? {
    guard alpha > 0 else { return nil }
    return CIColor(
        red: CGFloat(red),
        green: CGFloat(green),
        blue: CGFloat(blue),
        alpha: CGFloat(alpha)
    )
}

private func blendForeground(
    _ inputImage: CIImage,
    maskBuffer: CVPixelBuffer,
    backgroundColor: CIColor?
) throws -> CIImage {
    let maskImage = CIImage(cvPixelBuffer: maskBuffer)
    guard let blendFilter = CIFilter(name: "CIBlendWithMask") else {
        throw MattingFailure.blendFailed
    }
    let backgroundImage = backgroundColor.map {
        CIImage(color: $0).cropped(to: inputImage.extent)
    } ?? CIImage.empty()
    blendFilter.setValue(inputImage,       forKey: kCIInputImageKey)
    blendFilter.setValue(maskImage,        forKey: kCIInputMaskImageKey)
    blendFilter.setValue(backgroundImage,  forKey: kCIInputBackgroundImageKey)
    guard let outputImage = blendFilter.outputImage else {
        throw MattingFailure.blendFailed
    }
    return outputImage
}

@available(macOS 14.0, *)
private func cropIfNeeded(
    _ image: CIImage,
    maskBuffer: CVPixelBuffer,
    enabled: Bool
) throws -> CIImage {
    guard enabled else { return image }
    guard let cropRect = subjectCropRect(
        from: maskBuffer,
        imageExtent: image.extent,
        paddingRatio: 0
    ) else {
        throw MattingFailure.cropBoundsFailed
    }
    let cropped = image.cropped(to: cropRect)
    return cropped.transformed(
        by: CGAffineTransform(
            translationX: -cropped.extent.origin.x,
            y: -cropped.extent.origin.y
        )
    )
}

private func writePNG(_ image: CIImage, output: String) throws {
    do {
        try sharedRenderContext.writePNGRepresentation(
            of: image,
            to: URL(fileURLWithPath: output),
            format: .RGBA8,
            colorSpace: outputColorSpace
        )
    } catch {
        throw MattingFailure.writeFailed
    }
}

private func warnForLargeImage(_ image: CIImage, path: String) {
    let extent = image.extent
    guard extent.width.isFinite, extent.height.isFinite else { return }
    let pixelCount = extent.width * extent.height
    guard pixelCount > recommendedMaxPixels else { return }

    let megapixels = Double(pixelCount / 1_000_000)
    let message = String(
        format: "Warning: %@ is %.1f MP; images above 32 MP may cause high memory pressure.\n",
        path,
        megapixels
    )
    FileHandle.standardError.write(Data(message.utf8))
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
