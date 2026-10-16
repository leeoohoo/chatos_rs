import AppKit
import Foundation

extension MarkdownLayoutTextView: NSTextViewDelegate {
    func textView(_ textView: NSTextView, clickedOnLink link: Any, at charIndex: Int) -> Bool {
        guard let url = (link as? URL) ?? (link as? String).flatMap(URL.init(string:)) else {
            return true
        }
        linkHandler?(url)
        return true // Never let AppKit launch a raw relative file URL.
    }
}
