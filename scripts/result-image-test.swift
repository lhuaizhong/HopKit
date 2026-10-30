// Run against `npm run dev`:
// swift -module-cache-path .cache/swift-result-image scripts/result-image-test.swift
// Tests the same system WKWebView renderer used by the macOS application.
import Cocoa
import WebKit

final class ExportTest: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
    let output = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        .appendingPathComponent("test-output/result-image-webkit", isDirectory: true)
    var window: NSWindow!
    var view: WKWebView!

    func start() throws {
        try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
        let config = WKWebViewConfiguration()
        config.userContentController.add(self, name: "artifact")
        view = WKWebView(frame: NSRect(x: 0, y: 0, width: 1100, height: 800), configuration: config)
        view.navigationDelegate = self
        window = NSWindow(contentRect: view.frame, styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = view
        window.orderFront(nil)
        view.load(URLRequest(url: URL(string: "http://127.0.0.1:1420/scripts/result-image-fixture.html?autorun")!))
        DispatchQueue.main.asyncAfter(deadline: .now() + 45) {
            fputs("Timed out waiting for native image export\n", stderr)
            exit(1)
        }
    }

    func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        guard let body = message.body as? [String: String],
              let name = body["name"], ["fresh.png", "readable.png", "fresh.svg", "readable.svg", "report.json"].contains(name),
              let content = body["content"] else { return }
        do {
            if name.hasSuffix(".png") {
                guard let encoded = content.split(separator: ",").last,
                      let bytes = Data(base64Encoded: String(encoded)) else { exit(1) }
                try bytes.write(to: output.appendingPathComponent(name))
            } else if name.hasSuffix(".svg") {
                try content.write(to: output.appendingPathComponent(name), atomically: true, encoding: .utf8)
            } else {
                try content.write(to: output.appendingPathComponent(name), atomically: true, encoding: .utf8)
                let report = try JSONSerialization.jsonObject(with: Data(content.utf8)) as? [String: Any]
                print("WebKit result image regression: \(report?["status"] ?? "failed") (details: \(output.path)/report.json)")
                exit(report?["status"] as? String == "passed" ? 0 : 1)
            }
        } catch {
            fputs("\(error)\n", stderr)
            exit(1)
        }
    }

    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        fputs("\(error)\n", stderr)
        exit(1)
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let test = ExportTest()
try test.start()
app.run()
