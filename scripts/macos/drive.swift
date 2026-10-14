// Input driver for the macOS in-game tests: posts CoreGraphics key and mouse events, the way a
// player would. Needs Accessibility permission for the app running it (System Settings > Privacy).
// Usage: drive <cmd>...  cmds: down:<keycode> up:<keycode> tap:<keycode> look:<dx>,<dy>,<steps>
//                              sleep:<ms> focus:<pid>   (keycodes are macOS virtual keys; W = 13)
import AppKit
import CoreGraphics
import Foundation

let src = CGEventSource(stateID: .hidSystemState)
func key(_ code: UInt16, _ down: Bool) {
    CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: down)?.post(tap: .cghidEventTap)
}
func look(_ dx: Int, _ dy: Int, _ steps: Int) {
    for _ in 0..<steps {
        let pos = CGEvent(source: nil)?.location ?? .zero
        let e = CGEvent(mouseEventSource: src, mouseType: .mouseMoved, mouseCursorPosition: pos, mouseButton: .left)!
        e.setIntegerValueField(.mouseEventDeltaX, value: Int64(dx))
        e.setIntegerValueField(.mouseEventDeltaY, value: Int64(dy))
        e.post(tap: .cghidEventTap)
        usleep(16_000)
    }
}
for arg in CommandLine.arguments.dropFirst() {
    let parts = arg.split(separator: ":", maxSplits: 1).map(String.init)
    let (cmd, val) = (parts[0], parts.count > 1 ? parts[1] : "")
    switch cmd {
    case "down": key(UInt16(val)!, true)
    case "up": key(UInt16(val)!, false)
    case "tap": key(UInt16(val)!, true); usleep(60_000); key(UInt16(val)!, false)
    case "look":
        let n = val.split(separator: ",").map { Int($0)! }
        look(n[0], n[1], n[2])
    case "sleep": usleep(UInt32(val)! * 1000)
    case "focus":
        NSRunningApplication(processIdentifier: pid_t(val)!)?.activate()
    default: FileHandle.standardError.write("unknown command \(arg)\n".data(using: .utf8)!)
    }
}
