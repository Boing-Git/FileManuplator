//! `html_view`: runs an HTML file inside a phone-sized WebKit view and runs it
//! again every time the file is saved, so editing and seeing are one loop.
//!
//! WebKitGTK is loaded when the first `html_view` is built, not linked, so the
//! app still starts on systems without it; there the view offers to open the
//! page in the browser instead.

use gtk4 as gtk;
use gtk4::glib::translate::from_glib_none;
use gtk4::prelude::*;
use libadwaita as adw;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CString, c_char, c_int, c_void};
use std::path::Path;
use std::rc::Rc;
use std::sync::OnceLock;

/// Logical size of a typical phone screen.
const PHONE_SIZE: (i32, i32) = (360, 740);
/// Mobile Safari's user agent: the same engine, so pages that sniff for
/// phones get their phone layout and nothing WebKit lacks is promised.
const PHONE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) \
    AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";

type Callbacks = Rc<RefCell<Vec<Box<dyn Fn()>>>>;

/// The handful of WebKitGTK 6.0 functions the preview uses.
struct WebKit {
    _lib: libloading::os::unix::Library,
    web_view_new: unsafe extern "C" fn() -> *mut gtk::ffi::GtkWidget,
    load_uri: unsafe extern "C" fn(*mut c_void, *const c_char),
    reload: unsafe extern "C" fn(*mut c_void),
    get_settings: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    set_user_agent: unsafe extern "C" fn(*mut c_void, *const c_char),
    set_developer_extras: unsafe extern "C" fn(*mut c_void, c_int),
}

impl WebKit {
    fn get() -> Option<&'static WebKit> {
        static WEBKIT: OnceLock<Option<WebKit>> = OnceLock::new();
        WEBKIT
            .get_or_init(|| match unsafe { Self::load() } {
                Ok(webkit) => Some(webkit),
                Err(e) => {
                    eprintln!("HTML preview unavailable: {e}");
                    None
                }
            })
            .as_ref()
    }

    unsafe fn load() -> Result<WebKit, libloading::Error> {
        use libloading::os::unix::{Library, RTLD_LOCAL, RTLD_NOW};
        // RTLD_NOW: a WebKit built for a newer GTK fails here, not mid-call.
        let lib = unsafe { Library::open(Some("libwebkitgtk-6.0.so.4"), RTLD_NOW | RTLD_LOCAL)? };
        unsafe {
            Ok(WebKit {
                web_view_new: *lib.get(b"webkit_web_view_new\0")?,
                load_uri: *lib.get(b"webkit_web_view_load_uri\0")?,
                reload: *lib.get(b"webkit_web_view_reload\0")?,
                get_settings: *lib.get(b"webkit_web_view_get_settings\0")?,
                set_user_agent: *lib.get(b"webkit_settings_set_user_agent\0")?,
                set_developer_extras: *lib.get(b"webkit_settings_set_enable_developer_extras\0")?,
                _lib: lib,
            })
        }
    }
}

/// A WebKitWebView, as a GTK widget plus the calls made on it.
#[derive(Clone)]
struct WebView {
    widget: gtk::Widget,
    webkit: &'static WebKit,
}

impl WebView {
    fn new(webkit: &'static WebKit) -> Self {
        // SAFETY: webkit_web_view_new returns a new floating GtkWidget;
        // from_glib_none sinks and owns it.
        let widget: gtk::Widget = unsafe { from_glib_none((webkit.web_view_new)()) };
        let view = WebView { widget, webkit };
        unsafe {
            let settings = (webkit.get_settings)(view.ptr());
            let ua = CString::new(PHONE_UA).unwrap();
            (webkit.set_user_agent)(settings, ua.as_ptr());
            // Right-click → Inspect Element, for debugging the page being run.
            (webkit.set_developer_extras)(settings, 1);
        }
        view
    }

    fn ptr(&self) -> *mut c_void {
        self.widget.as_ptr() as *mut c_void
    }

    fn load_uri(&self, uri: &str) {
        let uri = CString::new(uri).unwrap();
        unsafe { (self.webkit.load_uri)(self.ptr(), uri.as_ptr()) }
    }

    fn reload(&self) {
        unsafe { (self.webkit.reload)(self.ptr()) }
    }
}

pub fn build(
    source: &str,
    state: Rc<RefCell<HashMap<String, String>>>,
    on_state_change: &Callbacks,
) -> gtk::Widget {
    let empty = adw::StatusPage::builder()
        .icon_name("phone-symbolic")
        .title("Nothing running")
        .description("Choose an HTML file to run it here, at phone size.")
        .build();
    empty.add_css_class("compact");

    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .build();
    stack.add_named(&empty, Some("empty"));

    let web = WebKit::get().map(WebView::new);
    let fallback_button = gtk::Button::builder()
        .label("Open in browser")
        .halign(gtk::Align::Center)
        .css_classes(["pill", "suggested-action"])
        .build();
    match &web {
        Some(web) => {
            web.widget.set_size_request(PHONE_SIZE.0, PHONE_SIZE.1);
            stack.add_named(&web.widget, Some("web"));
        }
        None => {
            let missing = adw::StatusPage::builder()
                .icon_name("web-browser-symbolic")
                .title("Preview needs WebKitGTK")
                .description(
                    "Install WebKitGTK 6.0 (libwebkitgtk-6.0-4 on Debian/Ubuntu, \
                     webkitgtk6.0 on Fedora, webkitgtk-6.0 on Arch) to run pages here.",
                )
                .child(&fallback_button)
                .build();
            missing.add_css_class("compact");
            missing.set_size_request(PHONE_SIZE.0, PHONE_SIZE.1);
            stack.add_named(&missing, Some("web"));
        }
    }

    // Without WebKit: the button opens whichever page is current.
    let fallback_file: Rc<RefCell<Option<gtk::gio::File>>> = Rc::default();
    let f = fallback_file.clone();
    fallback_button.connect_clicked(move |button| {
        if let Some(file) = f.borrow().as_ref() {
            gtk::FileLauncher::new(Some(file)).launch(
                button.root().and_downcast_ref::<gtk::Window>(),
                None::<&gtk::gio::Cancellable>,
                |_| {},
            );
        }
    });

    let phone = gtk::Box::new(gtk::Orientation::Vertical, 0);
    phone.add_css_class("phone-frame");
    phone.set_overflow(gtk::Overflow::Hidden);
    phone.set_halign(gtk::Align::Center);
    phone.append(&stack);

    // A page loaded while its tab is hidden runs its scripts at width 0, so
    // a load waits until the view is on screen.
    let pending: Rc<RefCell<Option<String>>> = Rc::default();
    if let Some(web) = &web {
        let (p, w) = (pending.clone(), web.clone());
        web.widget.connect_map(move |_| {
            if let Some(uri) = p.take() {
                w.load_uri(&uri);
            }
        });
    }

    let shown = Rc::new(RefCell::new(String::new()));
    let monitor: Rc<RefCell<Option<gtk::gio::FileMonitor>>> = Rc::default();
    let source = source.to_string();
    let refresh = move || {
        let path = fill(&source, &state.borrow());
        if *shown.borrow() == path {
            return;
        }
        shown.replace(path.clone());
        if !Path::new(&path).is_file() {
            stack.set_visible_child_name("empty");
            monitor.replace(None);
            return;
        }
        let file = gtk::gio::File::for_path(&path);
        stack.set_visible_child_name("web");
        let Some(web) = &web else {
            fallback_file.replace(Some(file));
            return;
        };
        if web.widget.is_mapped() {
            web.load_uri(&file.uri());
        } else {
            pending.replace(Some(file.uri().to_string()));
        }
        let watcher = file
            .monitor_file(
                gtk::gio::FileMonitorFlags::NONE,
                None::<&gtk::gio::Cancellable>,
            )
            .ok();
        if let Some(watcher) = &watcher {
            let web = web.clone();
            watcher.connect_changed(move |_, _, _, event| {
                if event == gtk::gio::FileMonitorEvent::ChangesDoneHint {
                    web.reload();
                }
            });
        }
        monitor.replace(watcher);
    };
    refresh();
    on_state_change.borrow_mut().push(Box::new(refresh));

    phone.upcast()
}

/// Replace `{key}` placeholders with the tab's current widget values.
fn fill(template: &str, state: &HashMap<String, String>) -> String {
    state
        .iter()
        .fold(template.to_string(), |text, (key, value)| {
            text.replace(&format!("{{{key}}}"), value)
        })
}
