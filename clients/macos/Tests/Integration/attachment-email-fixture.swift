// Synthetic screenshot for manual native Main Chat image-routing QA.
// swift attachment-email-fixture.swift /absolute/temporary/email.png
import AppKit
import Foundation

guard CommandLine.arguments.count == 2 else { fatalError("Provide one PNG output path") }
let width = 1_200
let height = 560
guard let bitmap = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height,
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: width * 4, bitsPerPixel: 32
), let context = NSGraphicsContext(bitmapImageRep: bitmap) else { fatalError("Cannot create fixture") }
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = context
NSColor.white.setFill()
NSRect(x: 0, y: 0, width: width, height: height).fill()
let text = """
测试邮件 · 全部为虚构数据

发件人：测试同事 <colleague@example.invalid>
主题：蓝鲸-482 项目会议确认

小李，你好：
请确认是否方便参加周五下午 3 点的项目会议，预计 30 分钟。
会议将在 B203 会议室进行，请带上最新版的界面设计稿。
谢谢！
测试同事
"""
(text as NSString).draw(in: NSRect(x: 36, y: 30, width: 1_128, height: 500), withAttributes: [
    .font: NSFont.systemFont(ofSize: 28), .foregroundColor: NSColor.black,
])
NSGraphicsContext.restoreGraphicsState()
guard let bytes = bitmap.representation(using: .png, properties: [:]) else { fatalError("Cannot encode fixture") }
try bytes.write(to: URL(fileURLWithPath: CommandLine.arguments[1]), options: .atomic)
print("Synthetic email screenshot generated")
