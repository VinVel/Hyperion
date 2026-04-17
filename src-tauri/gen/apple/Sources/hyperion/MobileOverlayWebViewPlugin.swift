import Foundation
import Tauri
import UIKit
import WebKit

private let defaultDesktopUserAgent =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 " +
  "(KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36 Hyperion/0.1"

private struct OpenOverlayWebViewArgs: Decodable {
  let url: String
  let title: String?
  let userAgent: String?
}

private final class OverlayWebViewController: UIViewController {
  private let pageTitle: String
  private let pageUrl: URL
  private let userAgent: String?
  private var webView: WKWebView?

  init(title: String, url: URL, userAgent: String?) {
    self.pageTitle = title
    self.pageUrl = url
    self.userAgent = userAgent
    super.init(nibName: nil, bundle: nil)
    modalPresentationStyle = .fullScreen
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    nil
  }

  override func viewDidLoad() {
    super.viewDidLoad()

    view.backgroundColor = UIColor(red: 0.07, green: 0.08, blue: 0.11, alpha: 1.0)

    let configuration = WKWebViewConfiguration()
    configuration.defaultWebpagePreferences.preferredContentMode = .desktop

    let webView = WKWebView(frame: .zero, configuration: configuration)
    webView.translatesAutoresizingMaskIntoConstraints = false
    webView.customUserAgent = (userAgent?.isEmpty == false ? userAgent : nil) ?? defaultDesktopUserAgent
    webView.allowsBackForwardNavigationGestures = true
    self.webView = webView

    let toolbar = UIView()
    toolbar.translatesAutoresizingMaskIntoConstraints = false
    toolbar.backgroundColor = UIColor(red: 0.09, green: 0.11, blue: 0.14, alpha: 1.0)

    let titleLabel = UILabel()
    titleLabel.translatesAutoresizingMaskIntoConstraints = false
    titleLabel.text = pageTitle
    titleLabel.textColor = .white
    titleLabel.font = UIFont.systemFont(ofSize: 16, weight: .semibold)
    titleLabel.numberOfLines = 1

    let closeButton = UIButton(type: .close)
    closeButton.translatesAutoresizingMaskIntoConstraints = false
    closeButton.addTarget(self, action: #selector(closeOverlay), for: .touchUpInside)

    toolbar.addSubview(titleLabel)
    toolbar.addSubview(closeButton)
    view.addSubview(toolbar)
    view.addSubview(webView)

    let guide = view.safeAreaLayoutGuide
    NSLayoutConstraint.activate([
      toolbar.topAnchor.constraint(equalTo: guide.topAnchor),
      toolbar.leadingAnchor.constraint(equalTo: view.leadingAnchor),
      toolbar.trailingAnchor.constraint(equalTo: view.trailingAnchor),
      toolbar.heightAnchor.constraint(equalToConstant: 56),

      closeButton.trailingAnchor.constraint(equalTo: toolbar.trailingAnchor, constant: -12),
      closeButton.centerYAnchor.constraint(equalTo: toolbar.centerYAnchor),

      titleLabel.leadingAnchor.constraint(equalTo: toolbar.leadingAnchor, constant: 16),
      titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: closeButton.leadingAnchor, constant: -12),
      titleLabel.centerYAnchor.constraint(equalTo: toolbar.centerYAnchor),

      webView.topAnchor.constraint(equalTo: toolbar.bottomAnchor),
      webView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
      webView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
      webView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
    ])

    webView.load(URLRequest(url: pageUrl))
  }

  @objc
  private func closeOverlay() {
    dismiss(animated: true)
  }
}

class MobileOverlayWebViewPlugin: Plugin {
  @objc public func open(_ invoke: Invoke) throws {
    do {
      let args = try invoke.parseArgs(OpenOverlayWebViewArgs.self)
      guard let url = URL(string: args.url) else {
        invoke.reject("Failed to open mobile overlay webview: invalid URL")
        return
      }

      let title = args.title?.isEmpty == false ? args.title! : (url.host ?? args.url)
      let controller = OverlayWebViewController(title: title, url: url, userAgent: args.userAgent)

      DispatchQueue.main.async {
        guard let viewController = self.manager.viewController else {
          invoke.reject("Failed to open mobile overlay webview: missing view controller")
          return
        }

        if let existingOverlay = viewController.presentedViewController as? OverlayWebViewController {
          existingOverlay.dismiss(animated: false) {
            viewController.present(controller, animated: true)
            invoke.resolve()
          }
          return
        }

        viewController.present(controller, animated: true)
        invoke.resolve()
      }
    } catch {
      invoke.reject("Failed to open mobile overlay webview: \(error.localizedDescription)")
    }
  }
}

@_cdecl("init_plugin_mobile_overlay_webview")
func initPluginMobileOverlayWebView() -> Plugin {
  MobileOverlayWebViewPlugin()
}
