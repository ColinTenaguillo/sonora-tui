//! The service's own sign-in page, in a native window: the cookie sign-ins
//! (Apple Music, Deezer, YouTube Music) without the devtools.
//!
//! sonora's `webview` opens a `WKWebView` over a throwaway data store, and
//! hands back the `Cookie` header once the service's proof cookies are in.
//! No browser profile holds that session, so nothing rotates the cookies
//! behind the app's back the way a shared browser session does.
//!
//! AppKit only works on the main thread, and on macOS that thread pumps the
//! run loop for the media keys while the app runs on another (see
//! `mediakeys_macos`). So the app asks for a window with [`open`], and the
//! main thread's [`Host`] opens it, pumps its events and polls it.
//! On Linux `webview` runs GTK on a thread of its own, so a plain thread
//! opens the window and polls it; without webkit2gtk it fails to open, and
//! the pasted header stays the way in.

/// How a sign-in window ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The `Cookie` header for the service's domain, with its proof in it.
    Cookies(String),
    /// The window was closed before the sign-in went through.
    Closed,
    /// No window could be opened.
    Failed(String),
}

/// What `webview` is asked to open for `sign_in`.
fn target(sign_in: &music::WebSignIn, service: &str) -> webview::Target {
    webview::Target {
        url: sign_in.url.to_owned(),
        landing: sign_in.landing.to_owned(),
        domain: sign_in.domain.to_owned(),
        proof: sign_in.proof.iter().map(|p| (*p).to_owned()).collect(),
        title: format!("Sign in to {service}"),
        agent: sign_in.agent.map(str::to_owned),
        script: None,
    }
}

/// How a poll leaves the window: still signing in, closed, or done.
fn settled(poll: webview::Poll) -> Option<Outcome> {
    match poll {
        webview::Poll::Pending => None,
        webview::Poll::Closed => Some(Outcome::Closed),
        webview::Poll::Cookies(header) => Some(Outcome::Cookies(header)),
    }
}

/// Ask the main thread for a window on `sign_in`. `None` where there is no
/// main-thread host to open one, as in a test.
#[cfg(target_os = "macos")]
pub fn open(
    sign_in: &music::WebSignIn,
    service: &str,
) -> Option<tokio::sync::oneshot::Receiver<Outcome>> {
    let target = target(sign_in, service);
    let (reply, outcome) = tokio::sync::oneshot::channel();
    mac::REQUESTS
        .get()?
        .send(mac::Request { target, reply })
        .ok()?;
    // The main thread may be asleep in the run loop for up to a second.
    apple_cf::cf::CFRunLoop::main().stop();
    Some(outcome)
}

/// Open a window on `sign_in`, driven by a thread of its own.
#[cfg(not(target_os = "macos"))]
pub fn open(
    sign_in: &music::WebSignIn,
    service: &str,
) -> Option<tokio::sync::oneshot::Receiver<Outcome>> {
    let target = target(sign_in, service);
    let (reply, outcome) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("sign-in window".into())
        .spawn(move || drive(target, reply))
        .ok()?;
    Some(outcome)
}

/// Opens the window and polls it until it settles, or until the sign-in is
/// cancelled in the app: dropping the page closes it.
#[cfg(not(target_os = "macos"))]
fn drive(target: webview::Target, reply: tokio::sync::oneshot::Sender<Outcome>) {
    let mut page = match webview::Page::open(target) {
        Ok(page) => page,
        Err(error) => {
            let _ = reply.send(Outcome::Failed(format!("{error:#}")));
            return;
        }
    };
    while !reply.is_closed() {
        if let Some(outcome) = settled(page.poll()) {
            let _ = reply.send(outcome);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(target_os = "macos")]
pub use mac::Host;

#[cfg(target_os = "macos")]
mod popups;

#[cfg(target_os = "macos")]
mod mac {
    use std::sync::mpsc;
    use std::sync::OnceLock;
    use std::time::Duration;

    use objc2::rc::Retained;
    use objc2::runtime::Sel;
    use objc2::{sel, MainThreadMarker, MainThreadOnly as _};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSEventMask, NSMenu, NSMenuItem,
    };
    use objc2_foundation::{NSDate, NSDefaultRunLoopMode, NSString};

    use super::{settled, Outcome};

    pub(super) struct Request {
        pub target: webview::Target,
        pub reply: tokio::sync::oneshot::Sender<Outcome>,
    }

    /// Where the app's requests go. Set once, by the main thread's host.
    pub(super) static REQUESTS: OnceLock<mpsc::Sender<Request>> = OnceLock::new();

    /// The window up, who waits on it, and the delegate opening its popups.
    struct Open {
        page: webview::Page,
        reply: tokio::sync::oneshot::Sender<Outcome>,
        popups: Option<Retained<super::popups::Popups>>,
    }

    /// The main thread's side: the window up, if any, and who waits on it.
    pub struct Host {
        requests: mpsc::Receiver<Request>,
        open: Option<Open>,
        mtm: MainThreadMarker,
    }

    impl Host {
        /// Start taking requests. `None` off the main thread, or when a
        /// host is already installed.
        pub fn install() -> Option<Self> {
            let mtm = MainThreadMarker::new()?;
            let (tx, requests) = mpsc::channel();
            REQUESTS.set(tx).ok()?;
            Some(Self {
                requests,
                open: None,
                mtm,
            })
        }

        /// Whether a window is up, and so wants its events pumped.
        pub fn busy(&self) -> bool {
            self.open.is_some()
        }

        /// Deliver the window's clicks and keys for up to `wait`. The run
        /// loop turns inside, so the media keys keep working meanwhile.
        pub fn pump(&self, wait: Duration) {
            let app = app(self.mtm);
            let until = NSDate::dateWithTimeIntervalSinceNow(wait.as_secs_f64());
            while let Some(event) = app.nextEventMatchingMask_untilDate_inMode_dequeue(
                NSEventMask::Any,
                Some(&until),
                unsafe { NSDefaultRunLoopMode },
                true,
            ) {
                app.sendEvent(&event);
            }
        }

        /// One turn of the main loop: the window's events while one is
        /// up, `idle` otherwise, then what was asked for and where the
        /// window is.
        pub fn turn(&mut self, idle: impl FnOnce()) {
            match self.busy() {
                true => self.pump(Duration::from_millis(100)),
                false => idle(),
            }
            self.take_request();
            if let Some(outcome) = self.check() {
                self.finish(outcome);
            }
        }

        fn take_request(&mut self) {
            if self.open.is_some() {
                return;
            }
            if let Ok(request) = self.requests.try_recv() {
                self.start(request);
            }
        }

        /// How the window up has ended, if it has. A sign-in cancelled in
        /// the app ends it too.
        fn check(&mut self) -> Option<Outcome> {
            let open = self.open.as_mut()?;
            match open.reply.is_closed() {
                true => Some(Outcome::Closed),
                false => settled(open.page.poll()),
            }
        }

        /// Hands the outcome over, closes the window, and gives the
        /// keyboard back to the terminal the user came from.
        fn finish(&mut self, outcome: Outcome) {
            if let Some(open) = self.open.take() {
                if let Some(popups) = &open.popups {
                    popups.close(None);
                }
                let _ = open.reply.send(outcome);
            }
            app(self.mtm).hide(None);
        }

        fn start(&mut self, request: Request) {
            // A terminal process is no app to AppKit until it says so; an
            // accessory one has no Dock icon, and still takes the keyboard.
            let app = app(self.mtm);
            app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
            if app.mainMenu().is_none() {
                app.setMainMenu(Some(&menu(self.mtm)));
            }
            let title = request.target.title.clone();
            match webview::Page::open(request.target) {
                Ok(page) => {
                    #[allow(deprecated)]
                    app.activateIgnoringOtherApps(true);
                    self.open = Some(Open {
                        page,
                        reply: request.reply,
                        popups: super::popups::Popups::attach(self.mtm, &title),
                    });
                }
                Err(error) => {
                    let _ = request.reply.send(Outcome::Failed(format!("{error:#}")));
                }
            }
        }
    }

    /// The menu bar AppKit reads key equivalents from. Without one, ⌘V
    /// pastes nothing into the page -- a password manager's paste included
    /// -- and ⌘W does not close the window.
    fn menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
        let item = |title: &str, action: Sel, key: &str| unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                Some(action),
                &NSString::from_str(key),
            )
        };
        let submenu = |title: &str, items: &[Retained<NSMenuItem>]| {
            let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
            for item in items {
                menu.addItem(item);
            }
            let holder = NSMenuItem::new(mtm);
            holder.setSubmenu(Some(&menu));
            holder
        };
        let bar = NSMenu::new(mtm);
        // The first menu is the application's, named by the system.
        bar.addItem(&submenu(
            "sonora-tui",
            &[item("Close Window", sel!(performClose:), "w")],
        ));
        bar.addItem(&submenu(
            "Edit",
            &[
                item("Cut", sel!(cut:), "x"),
                item("Copy", sel!(copy:), "c"),
                item("Paste", sel!(paste:), "v"),
                item("Select All", sel!(selectAll:), "a"),
            ],
        ));
        bar
    }

    fn app(mtm: MainThreadMarker) -> Retained<NSApplication> {
        NSApplication::sharedApplication(mtm)
    }
}

#[cfg(test)]
mod tests {
    use super::{settled, target, Outcome};

    #[test]
    fn the_window_opens_on_the_services_page_and_waits_for_its_proof() {
        let sign_in = music::WebSignIn {
            url: "https://www.deezer.com/login",
            landing: "www.deezer.com",
            domain: "deezer.com",
            proof: &["arl"],
            agent: None,
        };
        let target = target(&sign_in, "Deezer");
        assert_eq!(target.url, "https://www.deezer.com/login");
        assert_eq!(target.domain, "deezer.com");
        assert_eq!(target.proof, ["arl"]);
        assert_eq!(target.title, "Sign in to Deezer");
        assert_eq!(
            target.script, None,
            "a sign-in window is shown, never scripted"
        );
    }

    #[test]
    fn a_window_settles_on_its_cookies_or_on_being_closed() {
        assert_eq!(settled(webview::Poll::Pending), None, "still signing in");
        assert_eq!(settled(webview::Poll::Closed), Some(Outcome::Closed));
        assert_eq!(
            settled(webview::Poll::Cookies("arl=q".into())),
            Some(Outcome::Cookies("arl=q".into()))
        );
    }
}
