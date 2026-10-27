//! Signing in to YouTube, Spotify or Apple Music: the service's own sign-in page, in a window of WestPlay's on
//! the PC's desktop (a real keyboard, out of the game), on the WebView2 profile the service's page in WestPlay
//! uses, so the sign-in is its too. Nothing of the account passes through WestPlay: the page is the service's,
//! and what it keeps (its cookies) is WebView2's, as a browser's would be. WestPlay only sees where the window
//! is: still signing in, back on the service (signed in), or closed.
//!
//! Apple Music's sign-in is a dialog in its own web player (music.apple.com), whose address doesn't change as
//! it signs in: a script in the window's page opens that dialog and says when the page is signed in (only
//! that).
//!
//! The WebView is hosted as YouTube's is, drawn into a visual (WebView2 doesn't mix that with a windowed
//! WebView on one profile), here the window's own: the window passes its mouse to it and follows its cursor;
//! the keyboard reaches it as it has the focus.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MOUSE_EVENT_KIND, COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS, COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC,
    ICoreWebView2, ICoreWebView2CompositionController, ICoreWebView2Controller, ICoreWebView2Environment,
    ICoreWebView2Environment3,
};
use webview2_com::{
    AddScriptToExecuteOnDocumentCreatedCompletedHandler, CoTaskMemPWSTR,
    CreateCoreWebView2CompositionControllerCompletedHandler, CursorChangedEventHandler,
    NavigationCompletedEventHandler, WebMessageReceivedEventHandler, WindowCloseRequestedEventHandler,
};
use windows::UI::Composition::{Compositor, ContainerVisual, Desktop::DesktopWindowTarget};
use windows::Win32::Foundation::{E_POINTER, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetSystemMetrics, HCURSOR, HTCLIENT,
    IDC_ARROW, IsWindow, LoadCursorW, RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW, SetCursor,
    SetForegroundWindow, ShowWindow, WINDOW_EX_STYLE, WM_ACTIVATE, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
    WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SETFOCUS,
    WM_SIZE, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};
use windows::core::{HSTRING, Interface, PWSTR, w};
use windows_numerics::Vector2;

use crate::webview;

/// A service to sign in to: its sign-in page, the window's title, and its own pages, where the sign-in comes
/// back to once done (but for `not_yet`, a step of its own on the way, if any). A service whose sign-in stays
/// on one page has a script in it instead (`page_says`), which posts `{"signedIn":true}` once it is.
pub struct Site {
    pub sign_in: &'static str,
    pub title: &'static str,
    pub home: &'static str,
    pub not_yet: Option<&'static str>,
    pub page_says: Option<&'static str>,
}

/// Google's sign-in for YouTube, coming back to YouTube once signed in (through YouTube's own `/signin`, which
/// sets its cookies on the way).
pub const YOUTUBE: Site = Site {
    sign_in: "https://accounts.google.com/ServiceLogin?service=youtube&continue=https%3A%2F%2Fwww.youtube.com%2F",
    title: "Sign in to YouTube - WestPlay",
    home: "https://www.youtube.com/",
    not_yet: Some("signin"),
    page_says: None,
};

/// Google's sign-in for YouTube Music: YouTube's own (YouTube Music shares YouTube's sign-in: its cookies are
/// YouTube's), coming back to YouTube, which sets them.
pub const YOUTUBE_MUSIC: Site = Site { title: "Sign in to YouTube Music - WestPlay", ..YOUTUBE };

/// Spotify's sign-in, coming back to its web player.
pub const SPOTIFY: Site = Site {
    sign_in: "https://accounts.spotify.com/login?continue=https%3A%2F%2Fopen.spotify.com%2F",
    title: "Sign in to Spotify - WestPlay",
    home: "https://open.spotify.com/",
    not_yet: None,
    page_says: None,
};

/// Spotify's sign-up, coming back to its web player signed in to the new account.
pub const SPOTIFY_SIGN_UP: Site = Site {
    sign_in: "https://www.spotify.com/signup?forward_url=https%3A%2F%2Fopen.spotify.com%2F",
    title: "Sign up for Spotify - WestPlay",
    home: "https://open.spotify.com/",
    not_yet: None,
    page_says: None,
};

/// Apple Music's sign-in: the dialog of its own web player, opened at once (`apple_music_sign_in.js`).
pub const APPLE_MUSIC: Site = Site {
    sign_in: "https://music.apple.com/",
    title: "Sign in to Apple Music - WestPlay",
    home: "https://music.apple.com/",
    not_yet: None,
    page_says: Some(include_str!("apple_music_sign_in.js")),
};
const CLASS: windows::core::PCWSTR = w!("WestPlaySignIn");
const SIZE: (i32, i32) = (520, 760);
/// Windows' message for the mouse leaving a window that asked to be told (its value, from WinUser.h).
const WM_MOUSELEAVE: u32 = 0x02A3;

/// An open window's WebView, for its window procedure: to size it, pass it the mouse and show its cursor.
struct Hosted {
    window: isize,
    controller: ICoreWebView2Controller,
    composition: ICoreWebView2CompositionController,
    root: ContainerVisual,
    cursor: Cell<HCURSOR>,
    /// The mouse is over the window (Windows tells when it leaves only if asked each time it comes).
    tracking: Cell<bool>,
}

thread_local! {
    static HOSTED: RefCell<Vec<Rc<Hosted>>> = const { RefCell::new(Vec::new()) };
}

fn hosted(window: HWND) -> Option<Rc<Hosted>> {
    HOSTED.with(|hosted| hosted.borrow().iter().find(|h| h.window == window.0 as isize).cloned())
}

/// Where the sign-in is at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    /// Still on Google's pages.
    Signing,
    /// Back on the service: signed in.
    SignedIn,
    /// The player closed the window.
    Closed,
}

pub struct SignInWindow {
    window: HWND,
    hosted: Rc<Hosted>,
    webview: ICoreWebView2,
    /// Keeps the window's visual tree on screen.
    _target: DesktopWindowTarget,
    /// The address of the last page that finished loading.
    loaded: Rc<RefCell<String>>,
    site: &'static Site,
    /// Its page closed it (`window.close()`).
    closed: Rc<Cell<bool>>,
    /// Its page said it's signed in (a site that says so: `page_says`).
    said: Rc<Cell<bool>>,
}

impl SignInWindow {
    /// Opens `site`'s sign-in in a window on the desktop, in front, in `environment` (that of the service's page
    /// in WestPlay: its profile, so the sign-in is its).
    pub fn open(environment: &ICoreWebView2Environment, site: &'static Site) -> Result<Self, webview::Error> {
        // SAFETY: no name: this process's own module, loaded as long as it runs.
        let instance = unsafe { GetModuleHandleW(None)? };
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: CLASS,
            // SAFETY: a system cursor, by its id: Windows owns it, nothing to free.
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
            ..Default::default()
        };
        // Registered once (again: it fails harmlessly).
        // SAFETY: `class` is complete and its name is a static string.
        unsafe { RegisterClassW(&class) };
        // In the middle of the main screen.
        // SAFETY: plain queries, no pointers.
        let (screen_width, screen_height) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
        let (x, y) = if screen_width > SIZE.0 && screen_height > SIZE.1 {
            ((screen_width - SIZE.0) / 2, (screen_height - SIZE.1) / 2)
        } else {
            (CW_USEDEFAULT, CW_USEDEFAULT)
        };
        // SAFETY: the class is registered above, and the title lives through the call; no parent or menu.
        let window = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                CLASS,
                &HSTRING::from(site.title),
                WS_OVERLAPPEDWINDOW,
                x,
                y,
                SIZE.0,
                SIZE.1,
                None,
                None,
                Some(instance.into()),
                None,
            )?
        };
        match Self::host(environment, window, site) {
            Ok(this) => Ok(this),
            Err(e) => {
                // SAFETY: `window` was created above, on this thread, and nothing else holds it.
                unsafe {
                    let _ = DestroyWindow(window);
                }
                Err(e)
            }
        }
    }

    /// The WebView in `window`: drawn into its visual tree, on the service's sign-in.
    fn host(environment: &ICoreWebView2Environment, window: HWND, site: &'static Site) -> Result<Self, webview::Error> {
        // The window's own visual tree (the thread has the DispatcherQueue composition needs: the Stage's).
        let compositor = Compositor::new()?;
        let target =
            // SAFETY: `window` is ours and alive, on this thread, which has the dispatcher queue composition needs.
            unsafe { compositor.cast::<ICompositorDesktopInterop>()?.CreateDesktopWindowTarget(window, false)? };
        let root = compositor.CreateContainerVisual()?;
        target.SetRoot(&root)?;

        let environment: ICoreWebView2Environment3 = environment.cast()?;
        let (tx, rx) = mpsc::channel();
        CreateCoreWebView2CompositionControllerCompletedHandler::wait_for_async_operation(
            // SAFETY: `window` is alive; the handler is kept alive by `wait_for_async_operation` until WebView2 calls
            // it.
            Box::new(move |handler| unsafe {
                environment
                    .CreateCoreWebView2CompositionController(window, &handler)
                    .map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(move |error, composition| {
                error?;
                tx.send(composition.ok_or_else(|| windows::core::Error::from(E_POINTER))).expect("send controller");
                Ok(())
            }),
        )?;
        let composition: ICoreWebView2CompositionController = rx.recv()??;
        let controller: ICoreWebView2Controller = composition.cast()?;
        // SAFETY: `root` is the window's visual, kept by `Hosted` as long as the WebView.
        unsafe { composition.SetRootVisualTarget(&root)? };
        // SAFETY: the controller was just created and isn't closed.
        let webview = unsafe { controller.CoreWebView2()? };
        crate::webview::note_browser(&webview);

        let hosted = Rc::new(Hosted {
            window: window.0 as isize,
            controller,
            composition: composition.clone(),
            root,
            // SAFETY: a system cursor, by its id: Windows owns it, nothing to free.
            cursor: Cell::new(unsafe { LoadCursorW(None, IDC_ARROW)? }),
            tracking: Cell::new(false),
        });
        HOSTED.with(|all| all.borrow_mut().push(hosted.clone()));

        // The WebView's cursor (a hand over links, a bar over text) is the window's.
        let shown = hosted.clone();
        let handler = CursorChangedEventHandler::create(Box::new(move |sender, _| {
            if let Some(sender) = sender {
                let mut cursor = HCURSOR::default();
                // SAFETY: `sender` is the live controller WebView2 calls back with; `cursor` is a local it writes.
                if unsafe { sender.Cursor(&mut cursor) }.is_ok() {
                    shown.cursor.set(cursor);
                    // SAFETY: a cursor handle WebView2 just gave.
                    unsafe { SetCursor(Some(cursor)) };
                }
            }
            Ok(())
        }));
        let mut token = 0;
        // SAFETY: the handler and the token are locals for the call; WebView2 keeps its own reference to the handler.
        unsafe { composition.add_CursorChanged(&handler, &mut token)? };

        // Each page that finishes loading (Google's last step sets YouTube's cookies with a page of its own).
        let loaded = Rc::new(RefCell::new(String::new()));
        let noted = loaded.clone();
        let handler = NavigationCompletedEventHandler::create(Box::new(move |webview, _args| {
            if let Some(webview) = webview {
                let mut source = PWSTR::null();
                // SAFETY: WebView2 writes a CoTaskMem string into `source`, freed by `CoTaskMemPWSTR`.
                if unsafe { webview.Source(&mut source) }.is_ok() {
                    *noted.borrow_mut() = webview2_com::CoTaskMemPWSTR::from(source).to_string();
                }
            }
            Ok(())
        }));
        let mut token = 0;
        // SAFETY: the handler and the token are locals for the call; WebView2 keeps its own reference to the handler.
        unsafe { webview.add_NavigationCompleted(&handler, &mut token)? };

        // A site whose page says it's signed in: its script, and what it says.
        let said = Rc::new(Cell::new(false));
        if let Some(script) = site.page_says {
            let target = webview.clone();
            let script = HSTRING::from(script);
            AddScriptToExecuteOnDocumentCreatedCompletedHandler::wait_for_async_operation(
                // SAFETY: `script` lives through the call; the handler is kept alive until WebView2 calls it.
                Box::new(move |handler| unsafe {
                    target
                        .AddScriptToExecuteOnDocumentCreated(&script, &handler)
                        .map_err(webview2_com::Error::WindowsError)
                }),
                Box::new(|error, _id| error),
            )?;
            let heard = said.clone();
            let handler = WebMessageReceivedEventHandler::create(Box::new(move |_webview, args| {
                if let Some(args) = args {
                    let mut message = PWSTR::null();
                    // SAFETY: WebView2 writes a CoTaskMem string into `message`, freed by `CoTaskMemPWSTR`.
                    if unsafe { args.WebMessageAsJson(&mut message) }.is_ok()
                        && says_signed_in(&CoTaskMemPWSTR::from(message).to_string())
                    {
                        heard.set(true);
                    }
                }
                Ok(())
            }));
            let mut token = 0;
            // SAFETY: the handler and the token are locals for the call; WebView2 keeps its own reference to the
            // handler.
            unsafe { webview.add_WebMessageReceived(&handler, &mut token)? };
        }

        // Its page closing it.
        let closed = Rc::new(Cell::new(false));
        let noted = closed.clone();
        let handler = WindowCloseRequestedEventHandler::create(Box::new(move |_webview, _args| {
            noted.set(true);
            Ok(())
        }));
        let mut token = 0;
        // SAFETY: the handler and the token are locals for the call; WebView2 keeps its own reference to the handler.
        unsafe { webview.add_WindowCloseRequested(&handler, &mut token)? };

        let this = Self { window, hosted, webview, _target: target, loaded, site, closed, said };
        // SAFETY: the window and its WebView were made above, on this thread, and are alive; the address lives through
        // the call.
        unsafe {
            fit(window, &this.hosted);
            this.hosted.controller.SetIsVisible(true)?;
            this.webview.Navigate(&HSTRING::from(site.sign_in))?;
            let _ = ShowWindow(window, SW_SHOW);
        }
        this.bring_forward();
        Ok(this)
    }

    /// The window in front, its page taking the keyboard (also when it's already open: the sign-in asked for
    /// again).
    pub fn bring_forward(&self) {
        // SAFETY: the window and the controller live as long as `self`.
        unsafe {
            let _ = SetForegroundWindow(self.window);
            let _ = self.hosted.controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
        }
    }

    pub fn progress(&self) -> Progress {
        // SAFETY: `IsWindow` takes any handle, a destroyed one included.
        if self.closed.get() || !unsafe { IsWindow(Some(self.window)) }.as_bool() {
            return Progress::Closed;
        }
        if self.said.get() {
            return Progress::SignedIn;
        }
        if self.site.page_says.is_some() {
            return Progress::Signing;
        }
        progress_at(self.site, &self.loaded.borrow())
    }
}

/// Where a sign-in is at, by the address of the last page its window finished loading: a page of the
/// service's own is signed in, but for its own step on the way (YouTube's `/signin`, which sets its cookies).
fn progress_at(site: &Site, loaded: &str) -> Progress {
    let home = loaded.strip_prefix(site.home);
    let done = home.is_some_and(|path| site.not_yet.is_none_or(|step| !path.starts_with(step)));
    if done { Progress::SignedIn } else { Progress::Signing }
}

/// Whether a message of a sign-in window's page says it's signed in (`{"signedIn":true}`).
fn says_signed_in(message: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(message)
        .is_ok_and(|value| value.get("signedIn").and_then(serde_json::Value::as_bool) == Some(true))
}

/// The WebView over the whole window.
unsafe fn fit(window: HWND, hosted: &Hosted) {
    let mut bounds = RECT::default();
    // SAFETY: the caller passes a live window and its WebView (see `fit`); `bounds` is a local.
    unsafe {
        if GetClientRect(window, &mut bounds).is_ok() {
            let size = Vector2 { X: (bounds.right - bounds.left) as f32, Y: (bounds.bottom - bounds.top) as f32 };
            let _ = hosted.root.SetSize(size);
            let _ = hosted.controller.SetBounds(bounds);
        }
    }
}

impl Drop for SignInWindow {
    fn drop(&mut self) {
        let window = self.window.0 as isize;
        HOSTED.with(|hosted| hosted.borrow_mut().retain(|h| h.window != window));
        // SAFETY: the controller is closed once, on the thread that made it; the window is destroyed only if it's still
        // there.
        unsafe {
            let _ = self.hosted.controller.Close();
            if IsWindow(Some(self.window)).as_bool() {
                let _ = DestroyWindow(self.window);
            }
        }
    }
}

unsafe extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let Some(hosted) = hosted(window) else {
        // SAFETY: Windows' own handling of a message to this window, its arguments as they came.
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    };
    match message {
        WM_SIZE => {
            // SAFETY: `window` is receiving this message, so it's alive, and `hosted` is its WebView.
            unsafe { fit(window, &hosted) };
            LRESULT(0)
        }
        WM_SETFOCUS | WM_ACTIVATE => {
            // SAFETY: `hosted` is this window's WebView, alive while the window is.
            let _ = unsafe { hosted.controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC) };
            // SAFETY: Windows' own handling of a message to this window, its arguments as they came.
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
        // Over the page: its cursor.
        WM_SETCURSOR if (lparam.0 & 0xffff) as u32 == HTCLIENT => {
            // SAFETY: a cursor handle WebView2 gave (or the arrow): Windows owns it, never freed here.
            unsafe { SetCursor(Some(hosted.cursor.get())) };
            LRESULT(1)
        }
        WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP | WM_MBUTTONDOWN
        | WM_MBUTTONUP | WM_XBUTTONDOWN | WM_XBUTTONUP | WM_MOUSEWHEEL | WM_MOUSEHWHEEL | WM_MOUSELEAVE => {
            // SAFETY: `window` is receiving this message, so it's alive, and `hosted` is its WebView.
            unsafe { pass_mouse(window, &hosted, message, wparam, lparam) };
            LRESULT(0)
        }
        // Closing it destroys it: the sign-in sees it gone.
        // SAFETY: Windows' own handling of a message to this window, its arguments as they came.
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

/// A mouse message of the window, to the WebView: WebView2's mouse events are numbered as Windows' mouse
/// messages, their keys as the messages' (the low word of `wparam`), and a wheel's turn or an X button is its
/// high word. A button held keeps the mouse (a drag out of the window still ends in the page).
unsafe fn pass_mouse(window: HWND, hosted: &Hosted, message: u32, wparam: WPARAM, lparam: LPARAM) {
    let mut point = POINT { x: (lparam.0 & 0xffff) as i16 as i32, y: ((lparam.0 >> 16) & 0xffff) as i16 as i32 };
    let keys = COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS((wparam.0 & 0xffff) as i32);
    let data = match message {
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL | WM_XBUTTONDOWN | WM_XBUTTONUP => {
            ((wparam.0 >> 16) & 0xffff) as i16 as i32 as u32
        }
        _ => 0,
    };
    // SAFETY: the caller passes a live window and its WebView (see `pass_mouse`); `point` is a local.
    unsafe {
        match message {
            // A wheel's position is the screen's.
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                let _ = ScreenToClient(window, &mut point);
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
                SetCapture(window);
            }
            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
                let _ = ReleaseCapture();
            }
            WM_MOUSEMOVE if !hosted.tracking.get() => {
                let mut leave = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: window,
                    dwHoverTime: 0,
                };
                if TrackMouseEvent(&mut leave).is_ok() {
                    hosted.tracking.set(true);
                }
            }
            WM_MOUSELEAVE => hosted.tracking.set(false),
            _ => {}
        }
        let _ = hosted.composition.SendMouseInput(COREWEBVIEW2_MOUSE_EVENT_KIND(message as i32), keys, data, point);
    }
}

/// How long the service's page has to say it's signed in, once its sign-in window has closed signed in.
pub const CONFIRM_WAIT: Duration = Duration::from_secs(20);

/// A sign-in still under way after its window closed signed in: the service's own page loads again, and only
/// once it says it's signed in is the sign-in done (or after `CONFIRM_WAIT`, should it never say). The
/// companion shows it as signing in until then, not as signed out for a moment.
#[derive(Debug, Default)]
pub struct Confirming(Cell<Option<Instant>>);

impl Confirming {
    /// The window closed signed in: confirming from now.
    pub fn start(&self) {
        self.0.set(Some(Instant::now()));
    }

    /// Whether confirming just ended: the page says it's signed in (`signed_in`, as it last said, if it said),
    /// or the wait is over.
    pub fn ended(&self, signed_in: Option<bool>) -> bool {
        let Some(since) = self.0.get() else { return false };
        if signed_in == Some(true) || since.elapsed() >= CONFIRM_WAIT {
            self.0.set(None);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Signed in in its window, the sign-in goes on until the page says it's signed in: not as the page
    /// starts loading again (nothing said yet) nor while it still says signed out, and only once.
    #[test]
    fn confirms_a_sign_in_once_the_page_says_so() {
        let confirming = Confirming::default();
        assert!(!confirming.ended(Some(true)), "nothing to confirm before the window closed");
        confirming.start();
        assert!(!confirming.ended(None));
        assert!(!confirming.ended(Some(false)));
        assert!(confirming.ended(Some(true)));
        assert!(!confirming.ended(Some(true)), "ended once");
    }

    #[test]
    fn is_signed_in_once_back_on_youtube() {
        assert_eq!(progress_at(&YOUTUBE, YOUTUBE.sign_in), Progress::Signing);
        assert_eq!(
            progress_at(&YOUTUBE, "https://accounts.google.com/v3/signin/challenge/pwd?flowName=x"),
            Progress::Signing
        );
        assert_eq!(progress_at(&YOUTUBE, "https://www.youtube.com/"), Progress::SignedIn);
        // YouTube's own step, setting its cookies: not yet.
        assert_eq!(
            progress_at(&YOUTUBE, "https://www.youtube.com/signin?action_handle_signin=true&app=desktop"),
            Progress::Signing
        );
        // Only YouTube itself, not an address mentioning it.
        assert_eq!(progress_at(&YOUTUBE, "https://evil.example/?https://www.youtube.com/"), Progress::Signing);
        assert_eq!(progress_at(&YOUTUBE, ""), Progress::Signing);
    }

    #[test]
    fn is_signed_in_to_spotify_once_back_on_its_web_player() {
        assert_eq!(progress_at(&SPOTIFY, SPOTIFY.sign_in), Progress::Signing);
        assert_eq!(progress_at(&SPOTIFY, "https://accounts.spotify.com/en/status"), Progress::Signing);
        assert_eq!(progress_at(&SPOTIFY, "https://open.spotify.com/"), Progress::SignedIn);
        assert_eq!(progress_at(&SPOTIFY, "https://open.spotify.com/collection/tracks"), Progress::SignedIn);
        assert_eq!(progress_at(&SPOTIFY, "https://evil.example/?https://open.spotify.com/"), Progress::Signing);
        // Signing up: Spotify's own pages until the new account comes back to the web player.
        assert_eq!(progress_at(&SPOTIFY_SIGN_UP, SPOTIFY_SIGN_UP.sign_in), Progress::Signing);
        assert_eq!(progress_at(&SPOTIFY_SIGN_UP, "https://open.spotify.com/"), Progress::SignedIn);
    }

    /// Apple Music's sign-in stays on its web player: signed in only once its page says so.
    #[test]
    fn hears_a_page_say_its_signed_in() {
        assert!(says_signed_in(r#"{"signedIn":true}"#));
        assert!(!says_signed_in(r#"{"signedIn":false}"#));
        assert!(!says_signed_in(r#"{"signedIn":"yes"}"#));
        assert!(!says_signed_in("not json"));
        assert!(APPLE_MUSIC.page_says.is_some(), "its address says nothing");
    }

    #[test]
    fn passes_mouse_messages_as_webview2_numbers_them() {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE, COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOWN,
            COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE, COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL,
            COREWEBVIEW2_MOUSE_EVENT_KIND_X_BUTTON_UP,
        };
        for (message, kind) in [
            (WM_MOUSEMOVE, COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE),
            (WM_LBUTTONDOWN, COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOWN),
            (WM_MOUSEWHEEL, COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL),
            (WM_XBUTTONUP, COREWEBVIEW2_MOUSE_EVENT_KIND_X_BUTTON_UP),
            (WM_MOUSELEAVE, COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE),
        ] {
            assert_eq!(COREWEBVIEW2_MOUSE_EVENT_KIND(message as i32), kind);
        }
    }
}
