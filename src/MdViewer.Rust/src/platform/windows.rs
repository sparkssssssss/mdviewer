#![allow(non_snake_case)]

use serde::Serialize;
use std::cell::RefCell;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::mem;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::SystemTime;
use webview2_com::{
    Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_3, COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_ALLOW, *,
    },
    *,
};
use windows::{
    core::{Interface, HRESULT, PCWSTR, PWSTR},
    Win32::{
        Foundation::{E_POINTER, HINSTANCE, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM},
        Graphics::Gdi,
        System::{Com::*, LibraryLoader},
        UI::{
            HiDpi,
            Input::KeyboardAndMouse,
            Shell,
            WindowsAndMessaging::{self, MSG, WINDOW_LONG_PTR_INDEX, WNDCLASSW},
        },
    },
};

#[derive(Debug)]
pub enum Error {
    WebView2Error(webview2_com::Error),
    WindowsError(windows::core::Error),
    JsonError(serde_json::Error),
    LockError,
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::WebView2Error(err) => write!(f, "WebView2 error: {err}"),
            Error::WindowsError(err) => write!(f, "Windows error: {err}"),
            Error::JsonError(err) => write!(f, "JSON error: {err}"),
            Error::LockError => write!(f, "Lock error"),
            Error::Other(message) => f.write_str(message),
        }
    }
}

impl From<webview2_com::Error> for Error {
    fn from(err: webview2_com::Error) -> Self {
        Self::WebView2Error(err)
    }
}

impl From<windows::core::Error> for Error {
    fn from(err: windows::core::Error) -> Self {
        Self::WindowsError(err)
    }
}

impl From<HRESULT> for Error {
    fn from(err: HRESULT) -> Self {
        Self::WindowsError(windows::core::Error::from(err))
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::JsonError(err)
    }
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        Self::LockError
    }
}

impl<T> From<std::sync::TryLockError<T>> for Error {
    fn from(_: std::sync::TryLockError<T>) -> Self {
        Self::LockError
    }
}

type AppResult<T> = std::result::Result<T, Error>;

const IDM_OPEN: usize = 1001;
const IDM_RELOAD: usize = 1002;
const IDM_EXIT: usize = 1003;
const IDM_TOGGLE_THEME: usize = 2001;
const IDM_ZOOM_IN: usize = 2002;
const IDM_ZOOM_OUT: usize = 2003;
const IDM_RESET_ZOOM: usize = 2004;
const AUTO_REFRESH_TIMER_ID: usize = 3001;

thread_local! {
    static APP_STATE: RefCell<Option<AppState>> = const { RefCell::new(None) };
}

#[derive(Clone)]
pub struct AppState {
    current_file: Rc<RefCell<Option<PathBuf>>>,
    assets_dir: PathBuf,
    webview: Rc<RefCell<Option<WebView>>>,
    viewer_ready: Rc<RefCell<bool>>,
    last_mod_time: Rc<RefCell<Option<SystemTime>>>,
}

#[derive(Clone)]
struct WebViewController(ICoreWebView2Controller);

#[derive(Clone)]
pub struct WebView {
    controller: Rc<WebViewController>,
    webview: Rc<ICoreWebView2>,
    frame: Option<FrameWindow>,
    url: Rc<RefCell<String>>,
}

#[derive(Clone)]
struct FrameWindow {
    window: Rc<HWND>,
    size: Rc<RefCell<SIZE>>,
}

impl FrameWindow {
    fn new() -> Self {
        let hwnd = {
            let class_name = to_wide("MdViewerRustWebView");
            let title = to_wide("MdViewer Rust");
            let window_class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };

            unsafe {
                WindowsAndMessaging::RegisterClassW(&window_class);
                let hwnd = WindowsAndMessaging::CreateWindowExW(
                    Default::default(),
                    PCWSTR(class_name.as_ptr()),
                    PCWSTR(title.as_ptr()),
                    WindowsAndMessaging::WS_OVERLAPPEDWINDOW,
                    WindowsAndMessaging::CW_USEDEFAULT,
                    WindowsAndMessaging::CW_USEDEFAULT,
                    WindowsAndMessaging::CW_USEDEFAULT,
                    WindowsAndMessaging::CW_USEDEFAULT,
                    None,
                    None,
                    LibraryLoader::GetModuleHandleW(None)
                        .ok()
                        .map(|h| HINSTANCE(h.0)),
                    None,
                );
                if let Ok(hwnd) = hwnd {
                    let _ = install_menu(hwnd);
                    Shell::DragAcceptFiles(hwnd, true);
                    let _ = WindowsAndMessaging::ShowWindow(
                        hwnd,
                        WindowsAndMessaging::SW_SHOWMAXIMIZED,
                    );
                    let _ = Gdi::UpdateWindow(hwnd);
                    let _ = KeyboardAndMouse::SetFocus(Some(hwnd));
                }
                hwnd
            }
        };

        FrameWindow {
            window: Rc::new(hwnd.unwrap_or_default()),
            size: Rc::new(RefCell::new(SIZE { cx: 0, cy: 0 })),
        }
    }
}

#[derive(Serialize)]
struct ViewerPayload {
    #[serde(rename = "Title")]
    title: Option<String>,
    #[serde(rename = "Markdown")]
    markdown: String,
    #[serde(rename = "BasePath")]
    base_path: Option<String>,
    #[serde(rename = "FileBaseUrl")]
    file_base_url: Option<String>,
}

impl Drop for WebViewController {
    fn drop(&mut self) {
        unsafe { self.0.Close() }.ok();
    }
}

pub fn run(args: &[String]) -> AppResult<()> {
    if handle_command_line_action(args)? {
        return Ok(());
    }

    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }
    unsafe {
        HiDpi::SetProcessDpiAwareness(HiDpi::PROCESS_PER_MONITOR_DPI_AWARE)?;
    }

    let app = AppState::new(args)?;
    app.run()
}

pub fn show_error(title: &str, message: &str) {
    show_message(title, message, 0x0000_0010);
}

impl AppState {
    fn new(args: &[String]) -> AppResult<Self> {
        Ok(Self {
            current_file: Rc::new(RefCell::new(extract_initial_file(args))),
            assets_dir: resolve_assets_dir()?,
            webview: Rc::new(RefCell::new(None)),
            viewer_ready: Rc::new(RefCell::new(false)),
            last_mod_time: Rc::new(RefCell::new(None)),
        })
    }

    fn run(self) -> AppResult<()> {
        APP_STATE.with(|slot| *slot.borrow_mut() = Some(self.clone()));

        let frame = FrameWindow::new();
        let webview = WebView::create_on_frame(frame, false)?;
        *self.webview.borrow_mut() = Some(webview.clone());

        let app = self.clone();
        unsafe {
            let mut _token = 0;
            webview.webview.add_WebMessageReceived(
                &WebMessageReceivedEventHandler::create(Box::new(move |_sender, args| {
                    if let Some(args) = args {
                        let mut message = PWSTR(ptr::null_mut());
                        if args.WebMessageAsJson(&mut message).is_ok() {
                            let message = CoTaskMemPWSTR::from(message);
                            if let Ok(text) = serde_json::from_str::<String>(&message.to_string()) {
                                match text.as_str() {
                                    "viewer-ready" => {
                                        *app.viewer_ready.borrow_mut() = true;
                                        let _ = app.render_current();
                                    }
                                    "open-file" => {
                                        let _ = app.open_file();
                                    }
                                    "edit-source" => {
                                        app.edit_source();
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    Ok(())
                })),
                &mut _token,
            )?;

            webview.webview.add_NavigationStarting(
                &NavigationStartingEventHandler::create(Box::new(move |_sender, args| {
                    if let Some(args) = args {
                        let uri = read_uri(&args)?;
                        if !is_allowed_uri(&uri) {
                            args.SetCancel(true)?;
                            open_external(&uri);
                        }
                    }
                    Ok(())
                })),
                &mut _token,
            )?;

            webview.webview.add_NewWindowRequested(
                &NewWindowRequestedEventHandler::create(Box::new(move |_sender, args| {
                    if let Some(args) = args {
                        let uri = read_new_window_uri(&args)?;
                        if !is_allowed_uri(&uri) {
                            args.SetHandled(true)?;
                            open_external(&uri);
                        }
                    }
                    Ok(())
                })),
                &mut _token,
            )?;

            webview.controller.0.add_ZoomFactorChanged(
                &ZoomFactorChangedEventHandler::create(Box::new(move |_sender, _args| Ok(()))),
                &mut _token,
            )?;
        }

        webview.map_folder("mdviewer.local", &self.assets_dir)?;
        let viewer_url = "https://mdviewer.local/viewer.html";
        webview.set_title("MdViewer Rust")?.navigate(viewer_url)?;

        if self.current_file.borrow().is_some() {
            // render will happen once viewer-ready arrives
        }
        webview.enable_auto_refresh_timer();

        webview.run()?;
        APP_STATE.with(|slot| *slot.borrow_mut() = None);
        Ok(())
    }

    fn open_file(&self) -> AppResult<()> {
        if let Some(path) = choose_markdown_file()? {
            *self.current_file.borrow_mut() = Some(path);
            *self.last_mod_time.borrow_mut() =
                current_mod_time(self.current_file.borrow().as_ref());
            self.render_current()?;
        }
        Ok(())
    }

    fn edit_source(&self) {
        let Some(path) = self.current_file.borrow().clone() else {
            return;
        };
        open_with_editor(&path);
    }

    fn reload(&self) {
        let _ = self.render_current();
    }

    fn toggle_theme(&self) {
        if let Some(webview) = self.webview.borrow().clone() {
            let _ = webview.eval("window.mdviewer?.toggleTheme?.()");
        }
    }

    fn zoom_in(&self) {
        if let Some(webview) = self.webview.borrow().clone() {
            let _ = webview.adjust_zoom(0.1);
        }
    }

    fn zoom_out(&self) {
        if let Some(webview) = self.webview.borrow().clone() {
            let _ = webview.adjust_zoom(-0.1);
        }
    }

    fn reset_zoom(&self) {
        if let Some(webview) = self.webview.borrow().clone() {
            let _ = webview.set_zoom(1.0);
        }
    }

    fn open_path(&self, path: PathBuf) {
        *self.current_file.borrow_mut() = Some(path);
        *self.last_mod_time.borrow_mut() = current_mod_time(self.current_file.borrow().as_ref());
        let _ = self.render_current();
    }

    fn check_auto_refresh(&self) {
        let current = current_mod_time(self.current_file.borrow().as_ref());
        if current.is_some() && current != *self.last_mod_time.borrow() {
            *self.last_mod_time.borrow_mut() = current;
            let _ = self.render_current();
        }
    }

    fn render_current(&self) -> AppResult<()> {
        if !*self.viewer_ready.borrow() {
            return Ok(());
        }

        let Some(webview) = self.webview.borrow().clone() else {
            return Ok(());
        };

        let payload = if let Some(path) = self.current_file.borrow().clone() {
            let markdown = fs::read_to_string(&path)
                .unwrap_or_else(|err| format!("# Failed to open file\n\n```text\n{err}\n```"));
            let base_dir = path.parent().map(|p| p.to_path_buf());
            if let Some(base_dir) = base_dir.as_ref() {
                webview.map_folder("mdviewer-files.local", base_dir)?;
            }
            ViewerPayload {
                title: path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string()),
                markdown,
                base_path: base_dir.as_ref().map(|p| p.to_string_lossy().to_string()),
                file_base_url: None,
            }
        } else {
            ViewerPayload {
                title: None,
                markdown: "# MdViewer Rust\n\nOpen or drag a Markdown file here to preview it."
                    .to_string(),
                base_path: None,
                file_base_url: None,
            }
        };

        let json = serde_json::to_string(&payload)?;
        webview.eval(&format!(
            "window.mdviewer && window.mdviewer.render({json})"
        ))?;
        Ok(())
    }
}

impl WebView {
    fn create_on_frame(frame: FrameWindow, debug: bool) -> AppResult<Self> {
        Self::create_with_parent(*frame.window, Some(frame), debug)
    }

    fn create_with_parent(
        parent: HWND,
        frame: Option<FrameWindow>,
        debug: bool,
    ) -> AppResult<Self> {
        let environment = {
            let (tx, rx) = mpsc::channel();
            CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
                Box::new(|handler| unsafe {
                    CreateCoreWebView2Environment(&handler)
                        .map_err(webview2_com::Error::WindowsError)
                }),
                Box::new(move |error_code, environment| {
                    error_code?;
                    tx.send(environment.ok_or_else(|| windows::core::Error::from(E_POINTER)))
                        .expect("send over mpsc channel");
                    Ok(())
                }),
            )?;
            webview2_com::wait_with_pump(rx)??
        };

        let controller = {
            let (tx, rx) = mpsc::channel();
            CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
                Box::new(move |handler| unsafe {
                    environment
                        .CreateCoreWebView2Controller(parent, &handler)
                        .map_err(webview2_com::Error::WindowsError)
                }),
                Box::new(move |error_code, controller| {
                    error_code?;
                    tx.send(controller.ok_or_else(|| windows::core::Error::from(E_POINTER)))
                        .expect("send over mpsc channel");
                    Ok(())
                }),
            )?;
            webview2_com::wait_with_pump(rx)??
        };

        let size = get_window_size(parent);
        unsafe {
            controller.SetBounds(RECT {
                left: 0,
                top: 0,
                right: size.cx,
                bottom: size.cy,
            })?;
            controller.SetIsVisible(true)?;
        }

        let webview = unsafe { controller.CoreWebView2()? };
        if !debug {
            unsafe {
                let settings = webview.Settings()?;
                settings.SetAreDefaultContextMenusEnabled(false)?;
                settings.SetAreDevToolsEnabled(false)?;
            }
        }

        if let Some(frame) = frame.as_ref() {
            *frame.size.borrow_mut() = size;
        }

        let webview = WebView {
            controller: Rc::new(WebViewController(controller)),
            webview: Rc::new(webview),
            frame,
            url: Rc::new(RefCell::new(String::new())),
        };

        if webview.frame.is_some() {
            WebView::set_window_webview(parent, Some(Box::new(webview.clone())));
        }

        Ok(webview)
    }

    pub fn run(self) -> AppResult<()> {
        let webview = self.webview.as_ref();
        let url = self.url.borrow().clone();

        if !url.is_empty() {
            let url = CoTaskMemPWSTR::from(url.as_str());
            unsafe {
                webview.Navigate(*url.as_ref().as_pcwstr())?;
            }
        }

        if let Some(frame) = self.frame.as_ref() {
            let hwnd = *frame.window;
            unsafe {
                let _ = Gdi::UpdateWindow(hwnd);
                let _ = KeyboardAndMouse::SetFocus(Some(hwnd));
            }
        }

        let mut msg = MSG::default();
        loop {
            unsafe {
                let result = WindowsAndMessaging::GetMessageW(&mut msg, None, 0, 0).0;
                match result {
                    -1 => break Err(windows::core::Error::from_thread().into()),
                    0 => break Ok(()),
                    _ => match msg.message {
                        WindowsAndMessaging::WM_APP => (),
                        WindowsAndMessaging::WM_TIMER => {
                            APP_STATE.with(|slot| {
                                if let Some(app) = slot.borrow().clone() {
                                    app.check_auto_refresh();
                                }
                            });
                        }
                        _ => {
                            let _ = WindowsAndMessaging::TranslateMessage(&msg);
                            WindowsAndMessaging::DispatchMessageW(&msg);
                        }
                    },
                }
            }
        }
    }

    pub fn set_title(&self, title: &str) -> AppResult<&Self> {
        if let Some(frame) = self.frame.as_ref() {
            let title = CoTaskMemPWSTR::from(title);
            unsafe {
                let _ =
                    WindowsAndMessaging::SetWindowTextW(*frame.window, *title.as_ref().as_pcwstr());
            }
        }
        Ok(self)
    }

    pub fn navigate(&self, url: &str) -> AppResult<&Self> {
        *self.url.borrow_mut() = url.to_string();
        Ok(self)
    }

    pub fn eval(&self, js: &str) -> AppResult<&Self> {
        let js = CoTaskMemPWSTR::from(js);
        unsafe {
            self.webview.ExecuteScript(
                *js.as_ref().as_pcwstr(),
                &ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(()))),
            )?;
        }
        Ok(self)
    }

    pub fn map_folder(&self, host: &str, folder: &Path) -> AppResult<&Self> {
        let webview3: ICoreWebView2_3 = self.webview.cast()?;
        let host = CoTaskMemPWSTR::from(host);
        let folder = CoTaskMemPWSTR::from(folder.to_string_lossy().as_ref());
        unsafe {
            webview3.SetVirtualHostNameToFolderMapping(
                *host.as_ref().as_pcwstr(),
                *folder.as_ref().as_pcwstr(),
                COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_ALLOW,
            )?;
        }
        Ok(self)
    }

    pub fn adjust_zoom(&self, delta: f64) -> AppResult<&Self> {
        let mut zoom = 1.0;
        unsafe {
            self.controller.0.ZoomFactor(&mut zoom)?;
            self.controller.0.SetZoomFactor((zoom + delta).max(0.5))?;
        }
        Ok(self)
    }

    pub fn set_zoom(&self, zoom: f64) -> AppResult<&Self> {
        unsafe {
            self.controller.0.SetZoomFactor(zoom)?;
        }
        Ok(self)
    }

    pub fn enable_auto_refresh_timer(&self) {
        if let Some(frame) = self.frame.as_ref() {
            unsafe {
                let _ = WindowsAndMessaging::SetTimer(
                    Some(*frame.window),
                    AUTO_REFRESH_TIMER_ID,
                    1000,
                    None,
                );
            }
        }
    }

    fn set_window_webview(hwnd: HWND, webview: Option<Box<WebView>>) -> Option<Box<WebView>> {
        unsafe {
            match SetWindowLong(
                hwnd,
                WindowsAndMessaging::GWLP_USERDATA,
                match webview {
                    Some(webview) => Box::into_raw(webview) as _,
                    None => 0_isize,
                },
            ) {
                0 => None,
                ptr => Some(Box::from_raw(ptr as *mut _)),
            }
        }
    }

    fn get_window_webview(hwnd: HWND) -> Option<Box<WebView>> {
        unsafe {
            let data = GetWindowLong(hwnd, WindowsAndMessaging::GWLP_USERDATA);
            match data {
                0 => None,
                _ => {
                    let webview_ptr = data as *mut WebView;
                    let raw = Box::from_raw(webview_ptr);
                    let webview = raw.clone();
                    mem::forget(raw);
                    Some(webview)
                }
            }
        }
    }
}

fn get_window_size(hwnd: HWND) -> SIZE {
    let mut client_rect = RECT::default();
    let _ = unsafe { WindowsAndMessaging::GetClientRect(hwnd, &mut client_rect) };
    SIZE {
        cx: client_rect.right - client_rect.left,
        cy: client_rect.bottom - client_rect.top,
    }
}

fn install_menu(hwnd: HWND) -> AppResult<()> {
    unsafe {
        let menu = WindowsAndMessaging::CreateMenu()?;
        let file = WindowsAndMessaging::CreateMenu()?;
        WindowsAndMessaging::AppendMenuW(
            file,
            WindowsAndMessaging::MF_STRING,
            IDM_OPEN,
            PCWSTR(to_wide("Open...").as_ptr()),
        )?;
        WindowsAndMessaging::AppendMenuW(
            file,
            WindowsAndMessaging::MF_STRING,
            IDM_RELOAD,
            PCWSTR(to_wide("Reload").as_ptr()),
        )?;
        WindowsAndMessaging::AppendMenuW(
            file,
            WindowsAndMessaging::MF_SEPARATOR,
            0,
            PCWSTR::null(),
        )?;
        WindowsAndMessaging::AppendMenuW(
            file,
            WindowsAndMessaging::MF_STRING,
            IDM_EXIT,
            PCWSTR(to_wide("Exit").as_ptr()),
        )?;
        WindowsAndMessaging::AppendMenuW(
            menu,
            WindowsAndMessaging::MF_POPUP,
            file.0 as usize,
            PCWSTR(to_wide("File").as_ptr()),
        )?;

        let view = WindowsAndMessaging::CreateMenu()?;
        WindowsAndMessaging::AppendMenuW(
            view,
            WindowsAndMessaging::MF_STRING,
            IDM_TOGGLE_THEME,
            PCWSTR(to_wide("Toggle Theme").as_ptr()),
        )?;
        WindowsAndMessaging::AppendMenuW(
            view,
            WindowsAndMessaging::MF_STRING,
            IDM_ZOOM_IN,
            PCWSTR(to_wide("Zoom In").as_ptr()),
        )?;
        WindowsAndMessaging::AppendMenuW(
            view,
            WindowsAndMessaging::MF_STRING,
            IDM_ZOOM_OUT,
            PCWSTR(to_wide("Zoom Out").as_ptr()),
        )?;
        WindowsAndMessaging::AppendMenuW(
            view,
            WindowsAndMessaging::MF_STRING,
            IDM_RESET_ZOOM,
            PCWSTR(to_wide("Reset Zoom").as_ptr()),
        )?;
        WindowsAndMessaging::AppendMenuW(
            menu,
            WindowsAndMessaging::MF_POPUP,
            view.0 as usize,
            PCWSTR(to_wide("View").as_ptr()),
        )?;

        WindowsAndMessaging::SetMenu(hwnd, Some(menu))?;
    }
    Ok(())
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    match msg {
        WindowsAndMessaging::WM_COMMAND => {
            if let Some(app) = current_app() {
                match w_param.0 & 0xffff {
                    IDM_OPEN => {
                        let _ = app.open_file();
                    }
                    IDM_RELOAD => app.reload(),
                    IDM_EXIT => unsafe {
                        let _ = WindowsAndMessaging::DestroyWindow(hwnd);
                    },
                    IDM_TOGGLE_THEME => app.toggle_theme(),
                    IDM_ZOOM_IN => app.zoom_in(),
                    IDM_ZOOM_OUT => app.zoom_out(),
                    IDM_RESET_ZOOM => app.reset_zoom(),
                    _ => {}
                }
            }
            return LRESULT(0);
        }
        WindowsAndMessaging::WM_DROPFILES => {
            if let Some(app) = current_app() {
                if let Some(path) = read_drop_path(l_param) {
                    app.open_path(path);
                }
            }
            return LRESULT(0);
        }
        WindowsAndMessaging::WM_TIMER => {
            if w_param.0 == AUTO_REFRESH_TIMER_ID {
                if let Some(app) = current_app() {
                    app.check_auto_refresh();
                }
                return LRESULT(0);
            }
        }
        _ => {}
    }

    let webview = match WebView::get_window_webview(hwnd) {
        Some(webview) => webview,
        None => return unsafe { WindowsAndMessaging::DefWindowProcW(hwnd, msg, w_param, l_param) },
    };

    let frame = webview.frame.as_ref().expect("owned windows only");

    match msg {
        WindowsAndMessaging::WM_SIZE => {
            let size = get_window_size(hwnd);
            unsafe {
                webview
                    .controller
                    .0
                    .SetBounds(RECT {
                        left: 0,
                        top: 0,
                        right: size.cx,
                        bottom: size.cy,
                    })
                    .ok();
            }
            *frame.size.borrow_mut() = size;
            LRESULT::default()
        }
        WindowsAndMessaging::WM_CLOSE => {
            unsafe {
                let _ = WindowsAndMessaging::DestroyWindow(hwnd);
            }
            LRESULT::default()
        }
        WindowsAndMessaging::WM_DESTROY => {
            unsafe {
                WindowsAndMessaging::PostQuitMessage(0);
            }
            LRESULT::default()
        }
        _ => unsafe { WindowsAndMessaging::DefWindowProcW(hwnd, msg, w_param, l_param) },
    }
}

fn current_app() -> Option<AppState> {
    APP_STATE.with(|slot| slot.borrow().clone())
}

fn read_drop_path(lparam: LPARAM) -> Option<PathBuf> {
    unsafe {
        let drop = Shell::HDROP(lparam.0 as *mut core::ffi::c_void);
        let len = Shell::DragQueryFileW(drop, 0, None);
        if len == 0 {
            Shell::DragFinish(drop);
            return None;
        }
        let mut buffer = vec![0u16; len as usize + 1];
        let copied = Shell::DragQueryFileW(drop, 0, Some(&mut buffer));
        Shell::DragFinish(drop);
        if copied == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buffer[..copied as usize]);
        Some(normalize_path(&path))
    }
}

fn read_uri(args: &ICoreWebView2NavigationStartingEventArgs) -> windows::core::Result<String> {
    unsafe {
        let mut uri = PWSTR(ptr::null_mut());
        args.Uri(&mut uri)?;
        Ok(CoTaskMemPWSTR::from(uri).to_string())
    }
}

fn read_new_window_uri(
    args: &ICoreWebView2NewWindowRequestedEventArgs,
) -> windows::core::Result<String> {
    unsafe {
        let mut uri = PWSTR(ptr::null_mut());
        args.Uri(&mut uri)?;
        Ok(CoTaskMemPWSTR::from(uri).to_string())
    }
}

fn is_allowed_uri(uri: &str) -> bool {
    uri.starts_with("https://mdviewer.local/") || uri.starts_with("https://mdviewer-files.local/")
}

fn open_external(uri: &str) {
    let _ = Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", uri])
        .creation_flags(0x0800_0000)
        .spawn();
}

fn current_mod_time(path: Option<&PathBuf>) -> Option<SystemTime> {
    path.and_then(|path| fs::metadata(path).ok()?.modified().ok())
}

fn handle_command_line_action(args: &[String]) -> AppResult<bool> {
    if args.len() < 2 {
        return Ok(false);
    }

    match args[1].to_lowercase().as_str() {
        "--help" | "-h" | "/?" => {
            println!("MdViewer Rust");
            println!("Usage:");
            println!("  MdViewerRust.exe <file.md>");
            println!("  MdViewerRust.exe --associate");
            println!("  MdViewerRust.exe --unassociate");
            Ok(true)
        }
        "--associate" => {
            associate_file_types()?;
            Ok(true)
        }
        "--unassociate" => {
            unassociate_file_types()?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn associate_file_types() -> AppResult<()> {
    let exe = std::env::current_exe().map_err(|e| Error::Other(e.to_string()))?;
    let exe = normalize_path(exe.to_string_lossy().as_ref());
    let exe_text = exe.to_string_lossy().to_string();
    let icon = exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("app.ico");
    let icon_value = if icon.exists() {
        icon.to_string_lossy().to_string()
    } else {
        format!("{exe_text},0")
    };
    let script = format!(
        r#"
New-Item -Path 'HKCU:\Software\Classes\.md' -Force | Out-Null
Set-ItemProperty -Path 'HKCU:\Software\Classes\.md' -Name '(default)' -Value 'MdViewerRust.md'
New-Item -Path 'HKCU:\Software\Classes\.markdown' -Force | Out-Null
Set-ItemProperty -Path 'HKCU:\Software\Classes\.markdown' -Name '(default)' -Value 'MdViewerRust.md'
New-Item -Path 'HKCU:\Software\Classes\.mdown' -Force | Out-Null
Set-ItemProperty -Path 'HKCU:\Software\Classes\.mdown' -Name '(default)' -Value 'MdViewerRust.md'
New-Item -Path 'HKCU:\Software\Classes\MdViewerRust.md' -Force | Out-Null
Set-ItemProperty -Path 'HKCU:\Software\Classes\MdViewerRust.md' -Name '(default)' -Value 'Markdown Document'
New-Item -Path 'HKCU:\Software\Classes\MdViewerRust.md\DefaultIcon' -Force | Out-Null
Set-ItemProperty -Path 'HKCU:\Software\Classes\MdViewerRust.md\DefaultIcon' -Name '(default)' -Value '{icon_value}'
New-Item -Path 'HKCU:\Software\Classes\MdViewerRust.md\shell\open\command' -Force | Out-Null
Set-ItemProperty -Path 'HKCU:\Software\Classes\MdViewerRust.md\shell\open\command' -Name '(default)' -Value '"{exe_text}" "%1"'
"#
    );
    run_powershell(&script)
}

fn unassociate_file_types() -> AppResult<()> {
    let script = r#"
Remove-Item -Path 'HKCU:\Software\Classes\.md' -Force -ErrorAction SilentlyContinue
Remove-Item -Path 'HKCU:\Software\Classes\.markdown' -Force -ErrorAction SilentlyContinue
Remove-Item -Path 'HKCU:\Software\Classes\.mdown' -Force -ErrorAction SilentlyContinue
Remove-Item -Path 'HKCU:\Software\Classes\MdViewerRust.md' -Recurse -Force -ErrorAction SilentlyContinue
"#;
    run_powershell(script)
}

fn run_powershell(command: &str) -> AppResult<()> {
    let status = Command::new("powershell")
        .args(["-NoProfile", "-Command", command])
        .creation_flags(0x0800_0000)
        .status()
        .map_err(|e| Error::Other(e.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Other("powershell command failed".into()))
    }
}

#[allow(non_snake_case)]
#[cfg(target_pointer_width = "32")]
unsafe fn SetWindowLong(window: HWND, index: WINDOW_LONG_PTR_INDEX, value: isize) -> isize {
    WindowsAndMessaging::SetWindowLongW(window, index, value as _) as _
}

#[allow(non_snake_case)]
#[cfg(target_pointer_width = "64")]
unsafe fn SetWindowLong(window: HWND, index: WINDOW_LONG_PTR_INDEX, value: isize) -> isize {
    WindowsAndMessaging::SetWindowLongPtrW(window, index, value)
}

#[allow(non_snake_case)]
#[cfg(target_pointer_width = "32")]
unsafe fn GetWindowLong(window: HWND, index: WINDOW_LONG_PTR_INDEX) -> isize {
    WindowsAndMessaging::GetWindowLongW(window, index) as _
}

#[allow(non_snake_case)]
#[cfg(target_pointer_width = "64")]
unsafe fn GetWindowLong(window: HWND, index: WINDOW_LONG_PTR_INDEX) -> isize {
    WindowsAndMessaging::GetWindowLongPtrW(window, index)
}

fn resolve_assets_dir() -> AppResult<PathBuf> {
    let exe = std::env::current_exe().map_err(|e| Error::Other(e.to_string()))?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| Error::Other("missing exe dir".into()))?;
    let candidate = exe_dir.join("assets");
    if candidate.join("viewer.html").exists() {
        return Ok(candidate);
    }

    let fallback = PathBuf::from("src/MdViewer/assets");
    if fallback.join("viewer.html").exists() {
        return Ok(fallback);
    }

    Err(Error::Other("viewer assets not found".into()))
}

fn extract_initial_file(args: &[String]) -> Option<PathBuf> {
    args.iter()
        .skip(1)
        .find(|arg| !arg.starts_with('-'))
        .map(|arg| normalize_path(arg.trim_matches('"')))
}

fn choose_markdown_file() -> AppResult<Option<PathBuf>> {
    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$d = New-Object System.Windows.Forms.OpenFileDialog
$d.Filter = 'Markdown files (*.md;*.markdown;*.mdown)|*.md;*.markdown;*.mdown|Text files (*.txt)|*.txt|All files (*.*)|*.*'
$d.Title = 'Open Markdown file'
if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::Write($d.FileName) }
"#;
    let output = Command::new("powershell")
        .args(["-NoProfile", "-STA", "-Command", script])
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|err| Error::Other(err.to_string()))?;

    if !output.status.success() {
        return Ok(None);
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        Ok(None)
    } else {
        Ok(Some(normalize_path(&path)))
    }
}

fn open_with_editor(path: &Path) {
    let path_text = path.to_string_lossy().to_string();
    if try_start("code.cmd", &["--goto", &format!("{path_text}:1:1")]) {
        return;
    }
    if try_start("code", &["--goto", &format!("{path_text}:1:1")]) {
        return;
    }
    if try_start("notepad.exe", &[&path_text]) {
        return;
    }
    let _ = Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", &path_text])
        .creation_flags(0x0800_0000)
        .spawn();
}

fn try_start(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .creation_flags(0x0800_0000)
        .spawn()
        .is_ok()
}

fn normalize_path(path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    path.canonicalize().unwrap_or(path)
}

fn show_message(title: &str, message: &str, icon: u32) {
    let title_w = to_wide(title);
    let message_w = to_wide(message);
    unsafe {
        let _ = WindowsAndMessaging::MessageBoxW(
            None,
            PCWSTR(message_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            WindowsAndMessaging::MESSAGEBOX_STYLE(icon),
        );
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}
