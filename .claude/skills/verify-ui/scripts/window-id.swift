// Print the CGWindowID of the largest on-screen window owned by <pid>.
// PID-filtered because several prboard instances can run at once and
// CGWindowList lookups by app name are ambiguous.
import CoreGraphics
import Foundation

guard CommandLine.arguments.count > 1, let pid = Int32(CommandLine.arguments[1]) else {
    FileHandle.standardError.write("usage: window-id.swift <pid>\n".data(using: .utf8)!)
    exit(2)
}
let opts: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
let list = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] ?? []
let mine = list.filter { ($0[kCGWindowOwnerPID as String] as? Int32) == pid }

func area(_ w: [String: Any]) -> Double {
    guard let b = w[kCGWindowBounds as String] as? [String: Double] else { return 0 }
    return (b["Width"] ?? 0) * (b["Height"] ?? 0)
}

guard let best = mine.max(by: { area($0) < area($1) }),
      let num = best[kCGWindowNumber as String] as? Int else {
    FileHandle.standardError.write("no on-screen window for pid \(pid)\n".data(using: .utf8)!)
    exit(1)
}
print(num)
