import Foundation
import ImageIO
import CoreGraphics

struct Pixels {
    let width: Int
    let height: Int
    let bytes: [UInt8]
    init(_ path: String) {
        let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil)!
        let image = CGImageSourceCreateImageAtIndex(source, 0, nil)!
        let width = image.width
        let height = image.height
        self.width = width
        self.height = height
        var data = [UInt8](repeating: 0, count: width * height * 4)
        data.withUnsafeMutableBytes { buffer in
            let context = CGContext(data: buffer.baseAddress, width: width, height: height,
                bitsPerComponent: 8, bytesPerRow: width * 4,
                space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue)!
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        }
        bytes = data
    }
    func red(_ index: Int) -> Bool {
        let offset = index * 4
        return bytes[offset] > 20 && Int(bytes[offset]) > Int(bytes[offset + 1]) * 2
            && Int(bytes[offset]) > Int(bytes[offset + 2]) * 2
    }
}

for directory in CommandLine.arguments.dropFirst() {
    let final = Pixels(directory + "/capture-2.png")
    let width = final.width
    let height = final.height
    let radius = 8
    var horizontal = [Bool](repeating: false, count: width * height)
    for y in 0..<height {
        var count = 0
        for x in 0..<min(width, radius + 1) { if final.red(y * width + x) { count += 1 } }
        for x in 0..<width {
            horizontal[y * width + x] = count > 0
            if x - radius >= 0 && final.red(y * width + x - radius) { count -= 1 }
            if x + radius + 1 < width && final.red(y * width + x + radius + 1) { count += 1 }
        }
    }
    var dilated = [Bool](repeating: false, count: width * height)
    for x in 0..<width {
        var count = 0
        for y in 0..<min(height, radius + 1) { if horizontal[y * width + x] { count += 1 } }
        for y in 0..<height {
            dilated[y * width + x] = count > 0
            if y - radius >= 0 && horizontal[(y - radius) * width + x] { count -= 1 }
            if y + radius + 1 < height && horizontal[(y + radius + 1) * width + x] { count += 1 }
        }
    }
    var counts: [Int] = []
    var outside: [Int] = []
    var energy: [Int] = []
    for frame in 0...2 {
        let pixels = Pixels(directory + "/capture-\(frame).png")
        precondition(pixels.width == width && pixels.height == height && height > 800)
        var red = 0
        var offBorder = 0
        var redEnergy = 0
        for y in 400..<(height - 400) {
            for x in 0..<width {
                let index = y * width + x
                if pixels.red(index) {
                    red += 1
                    redEnergy += Int(pixels.bytes[index * 4])
                    if !dilated[index] { offBorder += 1 }
                }
            }
        }
        counts.append(red)
        outside.append(offBorder)
        energy.append(redEnergy)
    }
    print("\(URL(fileURLWithPath: directory).lastPathComponent) red=\(counts) outsideFinalBorder8px=\(outside) redEnergy=\(energy)")
}
