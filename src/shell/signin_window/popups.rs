//! The windows a sign-in page opens on its own.
//!
//! "Continue with Google" (or Apple, or Facebook) on a service's sign-in page
//! is a `window.open`: the account's page runs in a popup and posts the
//! answer back to its opener. A `WKWebView` with no UI delegate drops every
//! `window.open`, so the button did nothing. [`Popups`] is that delegate: it
//! opens each popup in a window of its own, on the configuration WebKit hands
//! over -- the same throwaway data store, with `window.opener` wired up --
//! and closes it when the page calls `window.close()`.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, DefinedClass as _, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSBackingStoreType, NSWindow, NSWindowStyleMask};
use objc2_foundation::{NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};
use objc2_web_kit::{
    WKNavigationAction, WKUIDelegate, WKWebView, WKWebViewConfiguration, WKWindowFeatures,
};

/// The size Google's own popup asks for.
const WIDTH: f64 = 590.;
const HEIGHT: f64 = 830.;

#[derive(Default)]
pub struct Ivars {
    windows: RefCell<Vec<Retained<NSWindow>>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and Popups has no Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    pub struct Popups;

    unsafe impl NSObjectProtocol for Popups {}

    unsafe impl WKUIDelegate for Popups {
        #[unsafe(method_id(webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:))]
        fn create(
            &self,
            opener: &WKWebView,
            configuration: &WKWebViewConfiguration,
            _action: &WKNavigationAction,
            _features: &WKWindowFeatures,
        ) -> Option<Retained<WKWebView>> {
            Some(self.open(opener, configuration))
        }

        #[unsafe(method(webViewDidClose:))]
        fn did_close(&self, view: &WKWebView) {
            self.close(Some(view));
        }
    }
);

impl Popups {
    /// A delegate for the sign-in window titled `title`, set on its web
    /// view. The view only holds it weakly: the caller keeps it alive.
    pub fn attach(mtm: MainThreadMarker, title: &str) -> Option<Retained<Self>> {
        let view = sign_in_view(mtm, title)?;
        let popups: Retained<Self> =
            unsafe { msg_send![super(Self::alloc(mtm).set_ivars(Ivars::default())), init] };
        unsafe { view.setUIDelegate(Some(ProtocolObject::from_ref(&*popups))) };
        Some(popups)
    }

    /// Closes the popup showing `view`, or every popup when `None`.
    pub fn close(&self, view: Option<&WKWebView>) {
        self.ivars().windows.borrow_mut().retain(|window| {
            let shows = |view: &WKWebView| {
                window
                    .contentView()
                    .is_some_and(|content| std::ptr::eq(&*content, view.as_ref()))
            };
            let closing = view.is_none_or(shows);
            if closing {
                window.close();
            }
            !closing
        });
    }

    fn open(
        &self,
        opener: &WKWebView,
        configuration: &WKWebViewConfiguration,
    ) -> Retained<WKWebView> {
        let mtm = self.mtm();
        let frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(WIDTH, HEIGHT));
        // WebKit loads the request into the view returned, which must be
        // built on the configuration it handed over.
        let view = unsafe {
            WKWebView::initWithFrame_configuration(WKWebView::alloc(mtm), frame, configuration)
        };
        unsafe {
            view.setCustomUserAgent(opener.customUserAgent().as_deref());
            // A popup may open one of its own, and close itself.
            view.setUIDelegate(Some(ProtocolObject::from_ref(self)));
        }
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable;
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                frame,
                style,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // The list holds it; closing must not free it under the list.
        unsafe { window.setReleasedWhenClosed(false) };
        if let Some(title) = opener.window().map(|w| w.title()) {
            window.setTitle(&title);
        }
        window.setContentView(Some(&view));
        window.center();
        window.makeKeyAndOrderFront(None);
        self.ivars().windows.borrow_mut().push(window);
        view
    }
}

/// The web view of the window titled `title`: `webview` keeps its own to
/// itself, so it is found through AppKit.
fn sign_in_view(mtm: MainThreadMarker, title: &str) -> Option<Retained<WKWebView>> {
    let title = NSString::from_str(title);
    NSApplication::sharedApplication(mtm)
        .windows()
        .iter()
        .find(|window| window.title().isEqualToString(&title))?
        .contentView()?
        .downcast::<WKWebView>()
        .ok()
}
