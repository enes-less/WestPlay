//! WebView2 rendered offscreen: hosted through a `CompositionController` into a Windows.UI.Composition
//! visual that is never attached to a window. Must be created and used on one STA thread that pumps
//! window messages.
//!
//! Every WebView of the thread renders onto one [`Stage`]: its visuals, in their order (the cabin page on
//! top, YouTube's player under it while it plays), make up the frame that is captured.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_COLOR, COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_ALLOW, COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY_CORS,
    COREWEBVIEW2_MOUSE_EVENT_KIND, COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS, COREWEBVIEW2_PROCESS_FAILED_KIND,
    COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED, COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE, CreateCoreWebView2EnvironmentWithOptions,
    ICoreWebView2, ICoreWebView2_2, ICoreWebView2_3, ICoreWebView2CompositionController, ICoreWebView2Controller,
    ICoreWebView2Controller2, ICoreWebView2Controller3, ICoreWebView2Environment, ICoreWebView2Environment3,
    ICoreWebView2EnvironmentOptions, ICoreWebView2Settings5, ICoreWebView2Settings6,
};
use webview2_com::{
    AddScriptToExecuteOnDocumentCreatedCompletedHandler, CoTaskMemPWSTR, CoreWebView2EnvironmentOptions,
    CreateCoreWebView2CompositionControllerCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler,
    ProcessFailedEventHandler, WebMessageReceivedEventHandler,
};
use windows::System::DispatcherQueueController;
use windows::UI::Composition::{Compositor, ContainerVisual, Visual};
use windows::Win32::Foundation::{E_POINTER, HWND, POINT, RECT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DQTAT_COM_STA, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW};
use windows::core::{HSTRING, Interface, PCWSTR, PWSTR, w};
use windows_numerics::Vector2;

pub type Error = Box<dyn std::error::Error>;

/// A WebView2 process that failed, as far as it matters to WestPlay (the others, such as its GPU process,
/// WebView2 restarts by itself).
/// Ordered by how much they take to recover from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Failure {
    /// The page's own process exited or stopped responding: the page is gone (blank) until reloaded.
    Page,
    /// The browser process exited: the whole WebView is gone and must be created again.
    Browser,
}

/// The frame the thread's WebViews render onto, `width` x `height`: one compositor, and the visual that is
/// captured, holding each WebView's visual.
pub struct Stage {
    _dispatcher: DispatcherQueueController,
    compositor: Compositor,
    root: ContainerVisual,
}

impl Stage {
    pub fn new(width: u32, height: u32) -> Result<Self, Error> {
        // Windows.UI.Composition needs a DispatcherQueue on this thread.
        // SAFETY: the options are complete and describe this thread, which has no queue yet (one `Stage` per thread).
        let dispatcher = unsafe {
            CreateDispatcherQueueController(DispatcherQueueOptions {
                dwSize: size_of::<DispatcherQueueOptions>() as u32,
                threadType: DQTYPE_THREAD_CURRENT,
                apartmentType: DQTAT_COM_STA,
            })?
        };
        let compositor = Compositor::new()?;
        let root = compositor.CreateContainerVisual()?;
        root.SetSize(Vector2 { X: width as f32, Y: height as f32 })?;
        Ok(Self { _dispatcher: dispatcher, compositor, root })
    }

    /// The whole frame, for capture.
    pub fn visual(&self) -> windows::core::Result<Visual> {
        self.root.cast()
    }

    /// Puts a WebView's visual on the frame: over the rest, or under it.
    pub fn add(&self, view: &OffscreenWebView, on_top: bool) -> windows::core::Result<()> {
        let children = self.root.Children()?;
        if on_top { children.InsertAtTop(&view.root) } else { children.InsertAtBottom(&view.root) }
    }

    /// Takes a WebView's visual off the frame (before it's dropped).
    pub fn remove(&self, view: &OffscreenWebView) -> windows::core::Result<()> {
        self.root.Children()?.Remove(&view.root)
    }
}

pub struct OffscreenWebView {
    /// The WebView2 environment it was made in: another WebView on its profile is made from it (a second
    /// environment on a profile in use is refused).
    environment: ICoreWebView2Environment,
    root: ContainerVisual,
    host: HWND,
    composition: ICoreWebView2CompositionController,
    controller: ICoreWebView2Controller,
    webview: ICoreWebView2,
    /// JSON messages the page posted (`chrome.webview.postMessage`), until taken by `take_page_messages`.
    page_messages: Rc<RefCell<Vec<String>>>,
    /// The worst process failure since last taken (`take_failure`).
    failure: Rc<Cell<Option<Failure>>>,
    pub version: String,
}

impl OffscreenWebView {
    /// A WebView of `width` x `height` on `stage`'s compositor (not on the frame until [`Stage::add`]).
    pub fn new(stage: &Stage, width: u32, height: u32, user_data: &Path) -> Result<Self, Error> {
        let root = stage.compositor.CreateContainerVisual()?;
        root.SetSize(Vector2 { X: width as f32, Y: height as f32 })?;

        // WebView2 needs a parent HWND for focus and DPI; it is never shown.
        // SAFETY: a window of the system's own `STATIC` class, its strings static; no parent or menu.
        let host = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("westplay-offscreen-host"),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                width as i32,
                height as i32,
                None,
                None,
                Some(GetModuleHandleW(None)?.into()),
                None,
            )?
        };
        // The page takes input only from WestPlay (`send_mouse`). Disabled, the window and the WebView's own
        // windows inside it get none of the real mouse: otherwise, once a click makes the WebView capture the
        // mouse, the player's real clicks and the real cursor (wherever the game parked it) reach the page too.
        // SAFETY: `host` was just created on this thread.
        unsafe {
            let _ = EnableWindow(host, false);
        }

        let environment = create_environment(user_data)?;
        // SAFETY: WebView2 writes a CoTaskMem string into `version`, read before it's dropped.
        let version = unsafe {
            let mut version = PWSTR::null();
            environment.BrowserVersionString(&mut version)?;
            version.to_string()?
        };
        let composition = create_composition_controller(&environment.cast()?, host)?;
        let controller: ICoreWebView2Controller = composition.cast()?;
        // SAFETY: the controller and `root` were made above and are alive; the colour and bounds are plain values.
        unsafe {
            composition.SetRootVisualTarget(&root)?;
            // Render at exactly width x height regardless of the monitor's DPI.
            let controller3: ICoreWebView2Controller3 = controller.cast()?;
            controller3.SetShouldDetectMonitorScaleChanges(false)?;
            controller3.SetRasterizationScale(1.0)?;
            // Transparent: what the page leaves transparent stays transparent in the frames, and the game's own
            // screen shows through there (the truck's OEM strip around CarPlay).
            controller.cast::<ICoreWebView2Controller2>()?.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                A: 0,
                R: 0,
                G: 0,
                B: 0,
            })?;
            controller.SetBounds(RECT { left: 0, top: 0, right: width as i32, bottom: height as i32 })?;
            controller.SetIsVisible(true)?;
        }
        // SAFETY: the controller was just created and isn't closed.
        let webview = unsafe { controller.CoreWebView2()? };
        // A car screen, not a browser: no context menu, zooming, link previews in the corner or swipe-to-go-back.
        // SAFETY: the WebView's settings object, alive while the WebView is; plain values only.
        unsafe {
            let settings = webview.Settings()?;
            settings.SetAreDefaultContextMenusEnabled(false)?;
            settings.SetIsZoomControlEnabled(false)?;
            settings.SetIsStatusBarEnabled(false)?;
            settings.cast::<ICoreWebView2Settings5>()?.SetIsPinchZoomEnabled(false)?;
            settings.cast::<ICoreWebView2Settings6>()?.SetIsSwipeNavigationEnabled(false)?;
        }

        let page_messages = Rc::new(RefCell::new(Vec::new()));
        let queue = page_messages.clone();
        let handler = WebMessageReceivedEventHandler::create(Box::new(move |_webview, args| {
            if let Some(args) = args {
                let mut message = PWSTR::null();
                // SAFETY: WebView2 writes a CoTaskMem string into `message`, freed by `CoTaskMemPWSTR`.
                if unsafe { args.WebMessageAsJson(&mut message) }.is_ok() {
                    queue.borrow_mut().push(CoTaskMemPWSTR::from(message).to_string());
                }
            }
            Ok(())
        }));
        let mut token = 0;
        // SAFETY: the handler and the token are locals for the call; WebView2 keeps its own reference to the handler.
        unsafe { webview.add_WebMessageReceived(&handler, &mut token)? };

        let failure = Rc::new(Cell::new(None));
        let failed = failure.clone();
        let handler = ProcessFailedEventHandler::create(Box::new(move |_webview, args| {
            if let Some(args) = args {
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                // SAFETY: `args` is the event's, alive through the callback; `kind` is a local it writes.
                unsafe { args.ProcessFailedKind(&mut kind)? };
                if let Some(this) = failure_of(kind) {
                    // A browser failure outranks a page failure noted before it.
                    failed.set(failed.get().max(Some(this)));
                }
            }
            Ok(())
        }));
        let mut token = 0;
        // SAFETY: the handler and the token are locals for the call; WebView2 keeps its own reference to the handler.
        unsafe { webview.add_ProcessFailed(&handler, &mut token)? };

        Ok(Self { environment, root, host, composition, controller, webview, page_messages, failure, version })
    }

    /// The worst WebView2 process failure since the last call, if any.
    pub fn take_failure(&self) -> Option<Failure> {
        self.failure.take()
    }

    /// Loads the page again (after its process failed).
    pub fn reload(&self) -> windows::core::Result<()> {
        // SAFETY: the WebView is alive as long as `self`.
        unsafe { self.webview.Reload() }
    }

    /// Where on the stage the WebView shows, and its size: the page lays itself out at that size.
    pub fn place(&self, x: f32, y: f32, width: u32, height: u32) -> windows::core::Result<()> {
        self.root.SetOffset(windows_numerics::Vector3 { X: x, Y: y, Z: 0.0 })?;
        self.root.SetScale(windows_numerics::Vector3 { X: 1.0, Y: 1.0, Z: 1.0 })?;
        self.root.SetSize(Vector2 { X: width as f32, Y: height as f32 })?;
        // SAFETY: the controller is alive as long as `self`; the bounds are plain values.
        unsafe { self.controller.SetBounds(RECT { left: 0, top: 0, right: width as i32, bottom: height as i32 }) }
    }

    /// The page laid out large, in the shape of the rectangle `x, y, width, height` of the stage and as big
    /// as fits in `room` (width, height), its picture shrunk evenly into the rectangle: a small view of a
    /// page that stays large (a video there keeps its quality, and its shape: it's contained in the page).
    pub fn fit(&self, room: (u32, u32), x: f32, y: f32, width: u32, height: u32) -> windows::core::Result<()> {
        let (page_width, page_height, scale) = fitted(room, width, height);
        self.place(x, y, page_width, page_height)?;
        self.root.SetScale(windows_numerics::Vector3 { X: scale, Y: scale, Z: 1.0 })
    }

    /// Its WebView2 environment, for another WebView on its profile (a window: signin.rs).
    pub fn environment(&self) -> &ICoreWebView2Environment {
        &self.environment
    }

    /// Forgets every cookie of the WebView's profile (signing out of whatever it was signed in to), coming back
    /// once they're gone: a page loaded after this is signed out. (Deleting them through the cookie manager only
    /// starts it: a page loaded straight after could still find the sign-in, and say so.) Cookies only: the
    /// cabin page shares a profile with YouTube's, and keeps its settings in its own storage.
    pub fn delete_cookies(&self) -> windows::core::Result<()> {
        use webview2_com::ClearBrowsingDataCompletedHandler;
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            COREWEBVIEW2_BROWSING_DATA_KINDS_COOKIES, ICoreWebView2_13, ICoreWebView2Profile2,
        };
        // SAFETY: the WebView is alive as long as `self`, and so is the profile it gives.
        let profile: ICoreWebView2Profile2 = unsafe { self.webview.cast::<ICoreWebView2_13>()?.Profile()? }.cast()?;
        let cleared = ClearBrowsingDataCompletedHandler::wait_for_async_operation(
            // SAFETY: the profile lives in the closure through the call; the handler is kept alive until WebView2
            // calls it.
            Box::new(move |handler| unsafe {
                profile
                    .ClearBrowsingData(COREWEBVIEW2_BROWSING_DATA_KINDS_COOKIES, &handler)
                    .map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(|error| error),
        );
        if cleared.is_ok() {
            return Ok(());
        }
        // Not waited for, then: the cookie manager's way, at least started.
        // SAFETY: the WebView is alive as long as `self`, and so is the cookie manager it gives.
        unsafe { self.webview.cast::<ICoreWebView2_2>()?.CookieManager()?.DeleteAllCookies() }
    }

    /// Whether the WebView shows on the stage (hidden, its page runs on: a video still plays its sound).
    pub fn show(&self, shown: bool) -> windows::core::Result<()> {
        self.root.SetIsVisible(shown)
    }

    /// Mutes the WebView's sound where it leaves for the PC's speakers (its page still makes it: what the
    /// page measures of it is as it was).
    #[cfg(test)]
    pub fn set_muted(&self, muted: bool) -> windows::core::Result<()> {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_8;
        // SAFETY: the WebView is alive as long as `self`; a plain value.
        unsafe { self.webview.cast::<ICoreWebView2_8>()?.SetIsMuted(muted) }
    }

    /// Runs `script` in every page the WebView loads, before the page's own scripts.
    pub fn add_startup_script(&self, script: &str) -> Result<(), Error> {
        let webview = self.webview.clone();
        let script = HSTRING::from(script);
        AddScriptToExecuteOnDocumentCreatedCompletedHandler::wait_for_async_operation(
            // SAFETY: `script` lives through the call; the handler is kept alive until WebView2 calls it.
            Box::new(move |handler| unsafe {
                webview
                    .AddScriptToExecuteOnDocumentCreated(&script, &handler)
                    .map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(|error, _id| error),
        )?;
        Ok(())
    }

    /// Tests: the page's picture, as WebView2 draws it, written to `path` as a PNG (to see what a page shows).
    #[cfg(test)]
    pub fn capture_png(&self, path: &Path) -> Result<(), Error> {
        use webview2_com::CapturePreviewCompletedHandler;
        use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG;
        use windows::Win32::System::Com::{STGM_CREATE, STGM_WRITE};
        use windows::Win32::UI::Shell::SHCreateStreamOnFileEx;
        // SAFETY: the path lives through the call; the stream it gives is ours.
        let stream =
            unsafe { SHCreateStreamOnFileEx(&HSTRING::from(path.as_os_str()), (STGM_CREATE | STGM_WRITE).0, 0, true, None)? };
        let webview = self.webview.clone();
        CapturePreviewCompletedHandler::wait_for_async_operation(
            // SAFETY: the stream lives in the closure through the call; the handler is kept alive until WebView2
            // calls it.
            Box::new(move |handler| unsafe {
                webview
                    .CapturePreview(COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG, &stream, &handler)
                    .map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(|error| error),
        )?;
        Ok(())
    }

    /// Tests: runs `script` in the page, and its result as JSON (to measure what a page lays out).
    #[cfg(test)]
    pub fn evaluate(&self, script: &str) -> Result<String, Error> {
        use webview2_com::ExecuteScriptCompletedHandler;
        let webview = self.webview.clone();
        let script = HSTRING::from(script);
        let result = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
        let written = result.clone();
        ExecuteScriptCompletedHandler::wait_for_async_operation(
            // SAFETY: `script` lives through the call; the handler is kept alive until WebView2 calls it.
            Box::new(move |handler| unsafe {
                webview.ExecuteScript(&script, &handler).map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(move |error, json| {
                *written.borrow_mut() = json;
                error
            }),
        )?;
        Ok(result.take())
    }

    /// Serves `folder` at `https://{host_name}/`, to the page served from it.
    pub fn map_folder(&self, host_name: &str, folder: &Path) -> windows::core::Result<()> {
        self.serve(host_name, folder, false)
    }

    /// Serves `folder` at `https://{host_name}/` to any page of the WebView's, fetched across origins too
    /// (the map's tiles, fetched by MapLibre's worker from the cabin page).
    pub fn map_folder_to_any_page(&self, host_name: &str, folder: &Path) -> windows::core::Result<()> {
        self.serve(host_name, folder, true)
    }

    fn serve(&self, host_name: &str, folder: &Path, any_page: bool) -> windows::core::Result<()> {
        // WebView2 serves nothing from a folder named in Windows' long form (`\\?\C:\…`, as an installed
        // WestPlay's own folder comes): every file fails with ERR_INVALID_URL. Named plainly, it does.
        let folder = westplay_setup::places::plain(folder);
        let access = if any_page {
            COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_ALLOW
        } else {
            COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY_CORS
        };
        // SAFETY: the WebView is alive as long as `self`; the strings live through the call.
        unsafe {
            self.webview.cast::<ICoreWebView2_3>()?.SetVirtualHostNameToFolderMapping(
                &HSTRING::from(host_name),
                &HSTRING::from(folder.as_os_str()),
                access,
            )
        }
    }

    pub fn navigate(&self, url: &str) -> windows::core::Result<()> {
        // SAFETY: the WebView is alive as long as `self`; the address lives through the call.
        unsafe { self.webview.Navigate(&HSTRING::from(url)) }
    }

    pub fn navigate_to_string(&self, html: &str) -> windows::core::Result<()> {
        // SAFETY: the WebView is alive as long as `self`; the page lives through the call.
        unsafe { self.webview.NavigateToString(&HSTRING::from(html)) }
    }

    /// Messages the page posted since the last call, oldest first.
    pub fn take_page_messages(&self) -> Vec<String> {
        std::mem::take(&mut self.page_messages.borrow_mut())
    }

    pub fn post_json(&self, json: &str) -> windows::core::Result<()> {
        // SAFETY: the WebView is alive as long as `self`; the message lives through the call.
        unsafe { self.webview.PostWebMessageAsJson(&HSTRING::from(json)) }
    }

    /// Mouse input for the page, at `point` in page pixels (`data`: wheel delta, or which X button).
    pub fn send_mouse(
        &self,
        kind: COREWEBVIEW2_MOUSE_EVENT_KIND,
        keys: COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS,
        data: u32,
        point: POINT,
    ) -> windows::core::Result<()> {
        // SAFETY: the composition controller is alive as long as `self`; plain values only.
        unsafe { self.composition.SendMouseInput(kind, keys, data, point) }
    }
}

impl Drop for OffscreenWebView {
    fn drop(&mut self) {
        // SAFETY: the controller is closed once, on the thread that made it, before its window goes.
        unsafe {
            let _ = self.controller.Close();
            let _ = DestroyWindow(self.host);
        }
    }
}

/// The page for [`OffscreenWebView::fit`]: `width` x `height` grown evenly until it fills `room` one way,
/// and how much to shrink it by to be `width` x `height` again.
fn fitted((room_width, room_height): (u32, u32), width: u32, height: u32) -> (u32, u32, f32) {
    let (width, height) = (width.max(1) as f32, height.max(1) as f32);
    let grow = (room_width as f32 / width).min(room_height as f32 / height).max(1.0);
    ((width * grow).round() as u32, (height * grow).round() as u32, 1.0 / grow)
}

/// What a WebView2 process failure means for WestPlay (Microsoft's guidance: reload after the page's
/// process exits or hangs, create the WebView again after the browser process exits; WebView2 restarts its
/// other processes itself, and a frame's process is no concern of a page without frames).
fn failure_of(kind: COREWEBVIEW2_PROCESS_FAILED_KIND) -> Option<Failure> {
    match kind {
        COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED => Some(Failure::Browser),
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
        | COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE => Some(Failure::Page),
        _ => None,
    }
}

fn create_environment(user_data: &Path) -> Result<ICoreWebView2Environment, Error> {
    let options = CoreWebView2EnvironmentOptions::default();
    // Chromium throttles or stops rendering for hidden or occluded windows; ours is never shown.
    // SAFETY: the options object is ours and alive; the arguments are a plain string.
    unsafe {
        options.set_additional_browser_arguments(
            "--disable-features=CalculateNativeWinOcclusion --disable-background-timer-throttling \
             --disable-renderer-backgrounding --disable-backgrounding-occluded-windows \
             --autoplay-policy=no-user-gesture-required"
                .to_string(),
        );
    }
    let options: ICoreWebView2EnvironmentOptions = options.into();
    let user_data = HSTRING::from(user_data.as_os_str());

    let (tx, rx) = mpsc::channel();
    CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
        // SAFETY: the profile folder and the options live through the call; the handler is kept alive until WebView2
        // calls it.
        Box::new(move |handler| unsafe {
            CreateCoreWebView2EnvironmentWithOptions(PCWSTR::null(), &user_data, &options, &handler)
                .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |error, environment| {
            error?;
            tx.send(environment.ok_or_else(|| windows::core::Error::from(E_POINTER))).expect("send environment");
            Ok(())
        }),
    )?;
    Ok(rx.recv()??)
}

fn create_composition_controller(
    environment: &ICoreWebView2Environment3,
    host: HWND,
) -> Result<ICoreWebView2CompositionController, Error> {
    let environment = environment.clone();
    let (tx, rx) = mpsc::channel();
    CreateCoreWebView2CompositionControllerCompletedHandler::wait_for_async_operation(
        // SAFETY: `host` is the WebView's live window; the handler is kept alive until WebView2 calls it.
        Box::new(move |handler| unsafe {
            environment
                .CreateCoreWebView2CompositionController(host, &handler)
                .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |error, controller| {
            error?;
            tx.send(controller.ok_or_else(|| windows::core::Error::from(E_POINTER))).expect("send controller");
            Ok(())
        }),
    )?;
    Ok(rx.recv()??)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A built cab page (`WESTPLAY_UI`: the folder with `cabin.html`, as installed) served and loaded as the
    /// host does, and what it says: whether its scripts ran, and every error on the way.
    #[test]
    #[ignore = "loads a built cab page (WESTPLAY_UI)"]
    fn a_built_cab_page_runs() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        let Some(ui) = std::env::var_os("WESTPLAY_UI") else { panic!("WESTPLAY_UI: the built UI's folder") };
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        // A profile of its own, or the player's (WESTPLAY_PROFILE: WestPlay's `webview2` folder, WestPlay closed).
        let user_data = std::env::var_os("WESTPLAY_PROFILE").map_or_else(
            || std::env::temp_dir().join(format!("westplay-built-cab-{}", std::process::id())),
            std::path::PathBuf::from,
        );
        let stage = Stage::new(1280, 720).unwrap();
        let view = OffscreenWebView::new(&stage, 1280, 720, &user_data).unwrap();
        view.add_startup_script(
            r#"window.__errors = [];
            addEventListener('error', (e) => __errors.push('error: ' + (e.message || (e.target && (e.target.src || e.target.href)) || e.type)), true);
            addEventListener('unhandledrejection', (e) => __errors.push('rejection: ' + String(e.reason && (e.reason.stack || e.reason))));
            const error = console.error.bind(console);
            console.error = (...a) => { __errors.push('console: ' + a.map(String).join(' ')); error(...a) };"#,
        )
        .unwrap();
        view.map_folder("westplay.cabin", Path::new(&ui)).unwrap();
        view.navigate("https://westplay.cabin/cabin.html").unwrap();
        // As the host goes on (crate::run): the maps' and the player's own looks' folders, mapped as the page loads.
        for (host, name) in [("westplay.maps", "maps"), ("westplay.customize", "customize")] {
            let dir = std::env::temp_dir().join(format!("westplay-built-cab-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            view.map_folder_to_any_page(host, &dir).unwrap();
        }
        let until = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while std::time::Instant::now() < until {
            crate::pump_messages();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let state = view
            .evaluate(
                r#"({ url: location.href, ready: document.readyState, root: document.getElementById('root')?.innerHTML.length,
                     shell: !!document.querySelector('.shell'), errors: window.__errors })"#,
            )
            .unwrap();
        println!("page messages: {:?}\n{state}", view.take_page_messages());
    }

    /// Signing out: a cookie set by a page is gone once `delete_cookies` comes back, for the page loaded right
    /// after (a profile of its own; no real sign-in).
    #[test]
    #[ignore = "runs WebView2"]
    fn a_page_loaded_after_deleting_the_cookies_has_none() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let root = std::env::temp_dir().join(format!("westplay-cookies-{}", std::process::id()));
        let site = root.join("site");
        std::fs::create_dir_all(&site).unwrap();
        std::fs::write(site.join("index.html"), "<!doctype html><title>cookies</title>").unwrap();
        let stage = Stage::new(320, 200).unwrap();
        let view = OffscreenWebView::new(&stage, 320, 200, &root.join("profile")).unwrap();
        view.map_folder("westplay.cookies", &site).unwrap();
        let pump = |seconds: f64| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
            while std::time::Instant::now() < until {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        };
        view.navigate("https://westplay.cookies/index.html").unwrap();
        pump(2.0);
        view.evaluate("document.cookie = 'signed=in; max-age=3600; path=/'").unwrap();
        view.navigate("https://westplay.cookies/index.html").unwrap();
        pump(1.5);
        assert!(view.evaluate("document.cookie").unwrap().contains("signed=in"), "the cookie wasn't kept");
        view.delete_cookies().unwrap();
        view.navigate("https://westplay.cookies/index.html").unwrap();
        pump(1.5);
        assert_eq!(view.evaluate("document.cookie").unwrap(), "\"\"", "a cookie outlived the sign-out");
    }

    /// The truck's speakers (cab_sound.js) follow a page that plays from one element, then another, then the
    /// first again (as Spotify's player does around a pause): the sound goes on through them, never silent.
    /// Plain audio, a tone, in a profile of its own; muted (the speakers still measure it).
    #[test]
    #[ignore = "runs WebView2"]
    fn the_speakers_follow_a_page_back_to_an_element_it_played_before() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let root = std::env::temp_dir().join(format!("westplay-speakers-switch-{}", std::process::id()));
        let site = root.join("site");
        std::fs::create_dir_all(&site).unwrap();
        // Ten seconds of a 440 Hz tone, 16-bit mono at 44.1 kHz.
        let rate = 44_100u32;
        let samples: Vec<u8> = (0..rate * 10)
            .flat_map(|i| (((i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 12_000.0) as i16).to_le_bytes())
            .collect();
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&(36 + samples.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        for value in [16u32, 1 | (1 << 16), rate, rate * 2, 2 | (16 << 16)] {
            wav.extend_from_slice(&value.to_le_bytes());
        }
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(samples.len() as u32).to_le_bytes());
        wav.extend_from_slice(&samples);
        std::fs::write(site.join("tone.wav"), wav).unwrap();
        let speakers = serde_json::to_string(&crate::speakers::in_truck("scania.s_2016").unwrap()).unwrap();
        std::fs::write(
            site.join("index.html"),
            format!(
                "<!doctype html><audio id=a src=tone.wav loop></audio><audio id=b src=tone.wav loop></audio><script>{}\n\
                 let now = null\n\
                 const cab = window.westplayCabSound(() => now)\n\
                 cab.speakers({speakers})\n\
                 cab.head({{ x: -0.47, y: 1.33, z: 0.32, heading: 0, pitch: 0 }})\n\
                 window.playFrom = (id) => {{ for (const e of document.querySelectorAll('audio')) e.pause(); now = document.getElementById(id); now.play(); cab.tick() }}\n\
                 setInterval(() => cab.tick(), 250)\n\
                 window.level = () => cab.level()\n\
                 </script>",
                include_str!("cab_sound.js")
            ),
        )
        .unwrap();
        let stage = Stage::new(320, 200).unwrap();
        let view = OffscreenWebView::new(&stage, 320, 200, &root.join("profile")).unwrap();
        view.set_muted(true).unwrap();
        view.map_folder("westplay.speakers", &site).unwrap();
        let pump = |seconds: f64| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
            while std::time::Instant::now() < until {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        };
        view.navigate("https://westplay.speakers/index.html").unwrap();
        pump(2.0);
        let mut heard = Vec::new();
        for id in ["a", "b", "a"] {
            view.evaluate(&format!("playFrom('{id}')")).unwrap();
            pump(1.5);
            let level = view.evaluate("JSON.stringify(window.level() ?? null)").unwrap();
            println!("playing from {id}: {level}");
            let ears: Vec<f64> = serde_json::from_str::<String>(&level)
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default();
            heard.push(ears.iter().take(2).any(|ear| *ear > 0.0005));
        }
        assert_eq!(heard, [true, true, true], "silent through the speakers at some point (a, b, then a again)");
    }

    #[test]
    fn fits_a_small_view_in_its_own_shape() {
        // A 16:9 card on a 16:9 screen: the whole screen, shrunk.
        assert_eq!(fitted((1280, 720), 142, 80), (1278, 720, 80.0 / 720.0));
        // A 16:9 card on a squarer screen: as wide as the screen, in the card's shape.
        let (w, h, scale) = fitted((1024, 768), 160, 90);
        assert_eq!((w, h), (1024, 576));
        assert!((w as f32 * scale - 160.0).abs() < 0.5 && (h as f32 * scale - 90.0).abs() < 0.5);
        // Never grown past the screen, nor shrunk below the view's own size.
        assert_eq!(fitted((1280, 720), 2000, 1000).2, 1.0);
    }
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED,
    };

    #[test]
    fn reloads_after_the_page_fails_and_starts_over_after_the_browser_does() {
        assert_eq!(failure_of(COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED), Some(Failure::Page));
        assert_eq!(failure_of(COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE), Some(Failure::Page));
        assert_eq!(failure_of(COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED), Some(Failure::Browser));
        // WebView2 restarts its GPU process itself; the page has no frames.
        assert_eq!(failure_of(COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED), None);
        assert_eq!(failure_of(COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED), None);
        // The browser failing outranks the page failing.
        assert_eq!(Some(Failure::Page).max(Some(Failure::Browser)), Some(Failure::Browser));
    }
}
