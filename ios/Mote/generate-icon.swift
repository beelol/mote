import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let size = 1024
guard CommandLine.arguments.count == 2,
      let context = CGContext(
        data: nil,
        width: size,
        height: size,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue
      ) else {
    fatalError("Pass one output PNG path")
}

let background = CGColor(red: 11 / 255, green: 11 / 255, blue: 12 / 255, alpha: 1)
let dimRed = CGColor(red: 104 / 255, green: 29 / 255, blue: 36 / 255, alpha: 1)
let red = CGColor(red: 240 / 255, green: 68 / 255, blue: 76 / 255, alpha: 1)

context.setFillColor(background)
context.fill(CGRect(x: 0, y: 0, width: size, height: size))

context.setStrokeColor(dimRed)
context.setLineWidth(12)
context.stroke(CGRect(x: 104, y: 104, width: 816, height: 816))

context.setStrokeColor(red)
context.setLineWidth(42)
context.setLineCap(.square)
context.setLineJoin(.miter)
context.move(to: CGPoint(x: 264, y: 288))
context.addLine(to: CGPoint(x: 264, y: 736))
context.addLine(to: CGPoint(x: 512, y: 456))
context.addLine(to: CGPoint(x: 760, y: 736))
context.addLine(to: CGPoint(x: 760, y: 288))
context.strokePath()

let pixels: [(Int, Int, Int)] = [
  (120, 792, 24), (160, 824, 16), (208, 852, 12), (144, 728, 12),
  (776, 152, 24), (824, 200, 16), (856, 248, 12), (728, 144, 12),
  (136, 184, 16), (184, 136, 12), (840, 776, 16), (792, 840, 12),
]
context.setFillColor(red)
for (x, y, side) in pixels {
  context.fill(CGRect(x: x, y: y, width: side, height: side))
}

guard let image = context.makeImage(),
      let destination = CGImageDestinationCreateWithURL(
        URL(fileURLWithPath: CommandLine.arguments[1]) as CFURL,
        UTType.png.identifier as CFString,
        1,
        nil
      ) else {
    fatalError("Cannot create icon PNG")
}
CGImageDestinationAddImage(destination, image, nil)
guard CGImageDestinationFinalize(destination) else {
  fatalError("Cannot write icon PNG")
}
