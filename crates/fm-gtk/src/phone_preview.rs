//! `html_view`: runs an HTML file inside a phone-sized WebKit view and runs it
//! again every time the file is saved, so editing and seeing are one loop.

use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita as adw;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use webkit6::prelude::*;

/// Logical size of a typical phone screen.
const PHONE_SIZE: (i32, i32) = (360, 740);
/// Mobile Safari's user agent: the same engine, so pages that sniff for
/// phones get their phone layout and nothing WebKit lacks is promised.
const PHONE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) \
    AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";

type Callbacks = Rc<RefCell<Vec<Box<dyn Fn()>>>>;

pub fn build(
    source: &str,
    state: Rc<RefCell<HashMap<String, String>>>,
    on_state_change: &Callbacks,
) -> gtk::Widget {
    let web = webkit6::WebView::new();
    if let Some(settings) = WebViewExt::settings(&web) {
        settings.set_user_agent(Some(PHONE_UA));
        // Right-click → Inspect Element, for debugging the page being run.
        settings.set_enable_developer_extras(true);
    }
    web.set_size_request(PHONE_SIZE.0, PHONE_SIZE.1);

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
    stack.add_named(&web, Some("web"));

    let phone = gtk::Box::new(gtk::Orientation::Vertical, 0);
    phone.add_css_class("phone-frame");
    phone.set_overflow(gtk::Overflow::Hidden);
    phone.set_halign(gtk::Align::Center);
    phone.append(&stack);

    // A page loaded while its tab is hidden runs its scripts at width 0, so
    // a load waits until the view is on screen.
    let pending: Rc<RefCell<Option<String>>> = Rc::default();
    let p = pending.clone();
    web.connect_map(move |web| {
        if let Some(uri) = p.take() {
            web.load_uri(&uri);
        }
    });

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
        if web.is_mapped() {
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
