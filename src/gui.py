#!/usr/bin/env python3
"""any2any — GTK4 + libadwaita graphical front-end for the any2any CLI.

This is a UI layer on top of the exact same detector/config/engine modules
the CLI uses — not a reimplementation. Everything `any2any convert` and
`any2any formats` offer is available here: content-based MIME detection,
the full routing table (including the generic text fallback), raw-text
structure preservation, the markdown-extension tie-break, PDF engine
fallback, missing-binary detection, and overwrite protection.
"""

from __future__ import annotations

import threading
import traceback
from pathlib import Path

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gdk, Gio, GLib, Gtk  # noqa: E402

from config import ROUTES, ConversionRoute, resolve_route  # noqa: E402
from detector import detect_mime_type  # noqa: E402
from engine import BinaryNotFoundError, ConversionResult, resolve_binary, run_conversion  # noqa: E402

APP_ID = "dev.any2any.Converter"
CSS_PATH = Path(__file__).resolve().parent / "style.css"


def human_size(num_bytes: int) -> str:
    """Format a byte count as a short human-readable string."""
    size = float(num_bytes)
    for unit in ("B", "KB", "MB", "GB"):
        if size < 1024 or unit == "GB":
            return f"{size:.0f} {unit}" if unit == "B" else f"{size:.1f} {unit}"
        size /= 1024
    return f"{size:.1f} TB"  # pragma: no cover


def icon_name_for(mime_type: str, raw_text: bool) -> str:
    """Pick a stock Adwaita icon representing a MIME category."""
    if mime_type.startswith("image/"):
        return "image-x-generic"
    if mime_type.startswith("video/"):
        return "video-x-generic"
    if mime_type.startswith("audio/"):
        return "audio-x-generic"
    if "wordprocessingml" in mime_type or mime_type in ("application/rtf", "application/epub+zip"):
        return "x-office-document"
    if mime_type == "text/html":
        return "text-html"
    if raw_text:
        return "text-x-script"
    if mime_type.startswith("text/"):
        return "text-x-generic"
    return "application-x-generic"


class FormatsPage(Adw.NavigationPage):
    """Browsable, searchable list mirroring `any2any formats`."""

    def __init__(self) -> None:
        super().__init__(title="Supported Formats")

        toolbar = Adw.ToolbarView()
        header = Adw.HeaderBar()
        
        self.search = Gtk.SearchEntry(placeholder_text="Filter by MIME type or tool")
        self.search.connect("search-changed", self._on_search_changed)
        self.search.set_size_request(500, -1)
        header.set_title_widget(self.search)
        
        toolbar.add_top_bar(header)

        scroller = Gtk.ScrolledWindow(vexpand=True)
        clamp = Adw.Clamp(maximum_size=1200)
        outer = Gtk.Box(
            orientation=Gtk.Orientation.VERTICAL,
            spacing=18,
            margin_top=18,
            margin_bottom=24,
            margin_start=18,
            margin_end=18,
        )

        self.flowbox = Gtk.FlowBox(
            selection_mode=Gtk.SelectionMode.NONE,
            max_children_per_line=4,
            min_children_per_line=1,
            row_spacing=12,
            column_spacing=12,
            homogeneous=True,
            halign=Gtk.Align.FILL,
            valign=Gtk.Align.START,
        )
        self._rows: list[tuple[Gtk.Widget, str, str]] = []

        for mime_type, route in sorted(ROUTES.items()):
            card = Gtk.CenterBox()
            card.add_css_class("card")
            card.set_margin_top(12)
            card.set_margin_bottom(12)
            card.set_margin_start(16)
            card.set_margin_end(16)
            card.set_size_request(340, -1)

            # Left side (Icon and Text)
            start_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=14)
            icon = Gtk.Image.new_from_icon_name(icon_name_for(mime_type, route.raw_text))
            icon.set_pixel_size(32)
            start_box.append(icon)

            text_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4, valign=Gtk.Align.CENTER)
            title = Gtk.Label(label=mime_type, xalign=0, wrap=True)
            title.add_css_class("heading")
            subtitle = Gtk.Label(label=f"→ {', '.join(route.targets)}", xalign=0, wrap=True)
            subtitle.add_css_class("dim-label")
            subtitle.add_css_class("caption")
            text_box.append(title)
            text_box.append(subtitle)
            start_box.append(text_box)
            
            card.set_start_widget(start_box)

            # Right side (Badge)
            badge = Gtk.Label(label=route.tool.value, css_classes=["pill", "tool-badge"])
            badge.set_valign(Gtk.Align.CENTER)
            
            end_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
            end_box.append(badge)
            card.set_end_widget(end_box)

            self.flowbox.append(card)
            
            child = card.get_parent()
            self._rows.append((child, mime_type.lower(), route.tool.value.lower()))

        outer.append(self.flowbox)

        note = Gtk.Label(
            label=(
                "Any other text-based file — scripts, configs, logs, unusual "
                "source — is also supported through a generic fallback, with "
                "its structure preserved verbatim. Files ending in .md, "
                ".markdown, .mdown, or .mkd are parsed as real markdown "
                "prose instead of that fallback."
            ),
            wrap=True,
        )
        note.set_xalign(0)
        note.add_css_class("dim-label")
        note.add_css_class("caption")
        outer.append(note)

        clamp.set_child(outer)
        scroller.set_child(clamp)
        toolbar.set_content(scroller)
        self.set_child(toolbar)

    def _on_search_changed(self, entry: Gtk.SearchEntry) -> None:
        query = entry.get_text().strip().lower()
        for row, mime_lower, tool_lower in self._rows:
            row.set_visible(query in mime_lower or query in tool_lower)

class PluginsPage(Adw.NavigationPage):
    """Browsable list of loaded YAML plugins."""

    def __init__(self) -> None:
        super().__init__(title="Installed Plugins")

        toolbar = Adw.ToolbarView()
        header = Adw.HeaderBar()
        toolbar.add_top_bar(header)

        scroller = Gtk.ScrolledWindow(vexpand=True)
        clamp = Adw.Clamp(maximum_size=800)
        outer = Gtk.Box(
            orientation=Gtk.Orientation.VERTICAL,
            spacing=18,
            margin_top=18,
            margin_bottom=24,
            margin_start=18,
            margin_end=18,
        )

        from plugin_loader import load_plugins, PLUGIN_ROUTES
        import yaml
        
        load_plugins()
        
        # We need to find the unique plugins (targets) since PLUGIN_ROUTES is keyed by mime
        plugins_seen = set()
        
        for mime_type, targets in PLUGIN_ROUTES.items():
            for target, convert_fn in targets.items():
                if target in plugins_seen:
                    continue
                plugins_seen.add(target)
                
                # To get descriptions, we could parse the yaml again, but we just want to list them
                # For simplicity, we just show the name and mime types
                # Actually, the user wanted Name, Mime Types, Description.
                # Let's re-scan the plugins folder to show them natively.
                
        # Better: just scan plugins folder and read YAMLs directly for the UI
        from pathlib import Path
        bundled_dir = Path(__file__).parent / "plugins"
        user_dir = Path.home() / ".local" / "share" / "any2any" / "plugins"
        dirs = [bundled_dir, user_dir]
        
        flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=1)
        
        count = 0
        for pdir in dirs:
            if not pdir.exists(): continue
            for yml_file in list(pdir.glob("*.yaml")) + list(pdir.glob("*.yml")):
                try:
                    with open(yml_file, 'r') as f:
                        data = yaml.safe_load(f)
                    
                    target = data.get("target", yml_file.stem)
                    desc = data.get("description", "No description provided.")
                    mimes = data.get("mime_types", ["*/*"])
                    
                    card = Adw.ActionRow(title=f"Target: {target}", subtitle=desc)
                    badge = Gtk.Label(label=", ".join(mimes), css_classes=["pill", "tool-badge"])
                    badge.set_valign(Gtk.Align.CENTER)
                    card.add_suffix(badge)
                    card.add_prefix(Gtk.Image.new_from_icon_name("application-x-addon"))
                    
                    group = Adw.PreferencesGroup()
                    group.add(card)
                    flow.append(group)
                    count += 1
                except Exception:
                    pass

        if count == 0:
            empty = Adw.StatusPage(title="No Plugins Found", description="Drop .yaml files in ~/.local/share/any2any/plugins/", icon_name="edit-clear-symbolic")
            outer.append(empty)
        else:
            outer.append(flow)

        clamp.set_child(outer)
        scroller.set_child(clamp)
        toolbar.set_content(scroller)
        self.set_child(toolbar)


class MainWindow(Adw.ApplicationWindow):
    """The primary convert view: pick a file, pick a format, convert."""

    def __init__(self, app: Adw.Application) -> None:
        super().__init__(application=app, title="any2any")
        self.set_default_size(560, 780)

        self.current_file: Path | None = None
        self.current_mime: str | None = None
        self.current_route: ConversionRoute | None = None
        self.treated_as_markdown = False
        self.binary_error: str | None = None
        self.selected_target: str | None = None
        self._output_auto_follow = True
        self._toggle_buttons: list[Gtk.ToggleButton] = []
        self._format_group_children: list[Gtk.Widget] = []
        self._error_group_children: list[Gtk.Widget] = []

        self.toast_overlay = Adw.ToastOverlay()
        self.nav_view = Adw.NavigationView()
        self.nav_view.add(self._build_main_page())
        self.toast_overlay.set_child(self.nav_view)
        self.set_content(self.toast_overlay)

        drop_target = Gtk.DropTarget.new(Gdk.FileList, Gdk.DragAction.COPY)
        drop_target.connect("drop", self._on_drop)
        self.add_controller(drop_target)

    # ---- page construction -------------------------------------------------

    def _build_main_page(self) -> Adw.NavigationPage:
        toolbar = Adw.ToolbarView()

        header = Adw.HeaderBar()
        header.set_title_widget(Adw.WindowTitle(title="any2any", subtitle="Universal file converter"))

        menu = Gio.Menu()
        menu.append("Supported formats", "win.show-formats")
        menu.append("Installed plugins", "win.show-plugins")
        menu.append("About any2any", "win.show-about")
        menu_button = Gtk.MenuButton(icon_name="open-menu-symbolic", menu_model=menu)
        header.pack_end(menu_button)

        open_action = Gio.SimpleAction.new("open-file", None)
        open_action.connect("activate", lambda *_: self._pick_file())
        self.add_action(open_action)
        
        formats_action = Gio.SimpleAction.new("show-formats", None)
        formats_action.connect("activate", lambda *_: self.nav_view.push(FormatsPage()))
        self.add_action(formats_action)
        
        plugins_action = Gio.SimpleAction.new("show-plugins", None)
        plugins_action.connect("activate", lambda *_: self.nav_view.push(PluginsPage()))
        self.add_action(plugins_action)
        about_action = Gio.SimpleAction.new("show-about", None)
        about_action.connect("activate", lambda *_: self._show_about())
        self.add_action(about_action)

        toolbar.add_top_bar(header)

        scroller = Gtk.ScrolledWindow(vexpand=True)
        clamp = Adw.Clamp(maximum_size=520)
        
        self.root_box = Gtk.Box(
            orientation=Gtk.Orientation.VERTICAL,
            spacing=20,
            margin_top=8,
            margin_bottom=32,
            margin_start=18,
            margin_end=18,
            valign=Gtk.Align.CENTER, 
        )

        self.file_stack = Gtk.Stack(transition_type=Gtk.StackTransitionType.NONE)
        self.file_stack.set_vhomogeneous(False)
        self.file_stack.add_named(self._build_drop_zone(), "empty")
        self.file_stack.add_named(self._build_file_card(), "file")
        self.root_box.append(self.file_stack)

        self.format_group = Adw.PreferencesGroup(title="Convert to")
        self.format_group.set_visible(False)
        self.root_box.append(self.format_group)

        self.binary_banner = Adw.Banner(title="")
        self.binary_banner.set_revealed(False)
        self.root_box.append(self.binary_banner)

        self.output_row = Adw.EntryRow(title="Save as")
        self.output_row.connect("changed", self._on_output_changed)
        browse_output_btn = Gtk.Button(icon_name="document-save-as-symbolic", valign=Gtk.Align.CENTER)
        browse_output_btn.add_css_class("flat")
        browse_output_btn.connect("clicked", lambda *_: self._pick_output_path())
        self.output_row.add_suffix(browse_output_btn)
        self.output_group = Adw.PreferencesGroup()
        self.output_group.add(self.output_row)
        self.output_group.set_visible(False)
        self.root_box.append(self.output_group)

        self.convert_button = Gtk.Button()
        convert_content = Adw.ButtonContent(icon_name="document-send-symbolic", label="Convert")
        self.convert_button.set_child(convert_content)
        self.convert_button.add_css_class("suggested-action")
        self.convert_button.add_css_class("pill")
        self.convert_button.add_css_class("convert-button")
        self.convert_button.set_sensitive(False)
        self.convert_button.connect("clicked", lambda *_: self._on_convert_clicked())
        self.root_box.append(self.convert_button)

        self.progress_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10, halign=Gtk.Align.CENTER)
        self.progress_spinner = Gtk.Spinner()
        self.progress_label = Gtk.Label(css_classes=["dim-label"])
        self.progress_box.append(self.progress_spinner)
        self.progress_box.append(self.progress_label)
        self.progress_box.set_visible(False)
        self.root_box.append(self.progress_box)

        self.error_group = Adw.PreferencesGroup(title="Conversion failed")
        self.error_group.set_visible(False)
        self.root_box.append(self.error_group)

        clamp.set_child(self.root_box)
        scroller.set_child(clamp)
        toolbar.set_content(scroller)

        page = Adw.NavigationPage(title="any2any")
        page.set_child(toolbar)
        return page

    def _build_drop_zone(self) -> Gtk.Widget:
        status = Adw.StatusPage(
            icon_name="document-open-symbolic",
            title="Drop a file here",
            description="or click to browse",
        )
        status.add_css_class("compact")

        btn = Gtk.Button()
        btn.set_child(status)
        btn.add_css_class("card")
        btn.set_size_request(-1, 220)
        btn.connect("clicked", lambda *_: self._pick_file())

        return btn

    def _build_file_card(self) -> Gtk.Widget:
        card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=14)
        card.add_css_class("card")
        card.add_css_class("file-card")
        card.set_margin_top(4)
        card.set_margin_bottom(4)
        card.set_margin_start(4)
        card.set_margin_end(4)

        top_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=14)
        top_row.set_margin_top(16)
        top_row.set_margin_start(16)
        top_row.set_margin_end(16)

        self.file_icon = Gtk.Image.new_from_icon_name("text-x-generic")
        self.file_icon.set_pixel_size(40)
        top_row.append(self.file_icon)

        text_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2, hexpand=True)
        self.file_name_label = Gtk.Label(css_classes=["title-4"], xalign=0, wrap=True)
        self.file_mime_label = Gtk.Label(css_classes=["dim-label", "caption"], xalign=0)
        text_box.append(self.file_name_label)
        text_box.append(self.file_mime_label)
        top_row.append(text_box)

        change_btn = Gtk.Button(icon_name="document-open-recent-symbolic", valign=Gtk.Align.CENTER)
        change_btn.add_css_class("flat")
        change_btn.set_tooltip_text("Choose a different file")
        change_btn.connect("clicked", lambda *_: self._pick_file())
        top_row.append(change_btn)

        card.append(top_row)

        self.badge_flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=4)
        self.badge_flow.set_margin_start(16)
        self.badge_flow.set_margin_end(16)
        self.badge_flow.set_margin_bottom(16)
        self.badge_flow.set_visible(False)
        card.append(self.badge_flow)

        return card

    # ---- file selection ------------------------------------------------

    def _pick_file(self) -> None:
        dialog = Gtk.FileChooserDialog(
            title="Choose a file to convert",
            transient_for=self,
            action=Gtk.FileChooserAction.OPEN,
        )
        dialog.add_buttons(
            "_Cancel", Gtk.ResponseType.CANCEL,
            "_Open", Gtk.ResponseType.ACCEPT,
        )
        dialog.set_default_size(800, 600)

        def on_response(dialog_widget, response):
            if response == Gtk.ResponseType.ACCEPT:
                gfile = dialog_widget.get_file()
                if gfile:
                    path = gfile.get_path()
                    if path:
                        self._load_file(Path(path))
            dialog_widget.destroy()

        dialog.connect("response", on_response)
        dialog.present()

    def _on_drop(self, drop_target: Gtk.DropTarget, value: Gdk.FileList, x: float, y: float) -> bool:
        files = value.get_files()
        if not files:
            return False
        path = files[0].get_path()
        if path:
            self._load_file(Path(path))
            return True
        return False

    def _load_file(self, path: Path) -> None:
        if not path.is_file():
            self._show_toast(f"'{path.name}' isn't a readable file.")
            return

        mime_type = detect_mime_type(path)
        route, treated_as_markdown = resolve_route(mime_type, path)

        self.current_file = path
        self.current_mime = mime_type
        self.current_route = route
        self.treated_as_markdown = treated_as_markdown
        self.binary_error = None

        if route is not None:
            try:
                resolve_binary(route)
            except BinaryNotFoundError as exc:
                self.binary_error = str(exc)

        self._refresh_file_card()
        self._refresh_format_group()
        self._clear_error()
        self.file_stack.set_visible_child_name("file")

    # ---- file card / badges --------------------------------------------

    def _refresh_file_card(self) -> None:
        assert self.current_file is not None and self.current_mime is not None
        path = self.current_file
        mime_type = self.current_mime
        raw_text = self.current_route.raw_text if self.current_route else False

        self.file_icon.set_from_icon_name(icon_name_for(mime_type, raw_text))
        self.file_name_label.set_label(path.name)
        try:
            size_label = human_size(path.stat().st_size)
        except OSError:
            size_label = "unknown size"
        self.file_mime_label.set_label(f"{mime_type} · {size_label}")

        child = self.badge_flow.get_first_child()
        while child is not None:
            nxt = child.get_next_sibling()
            self.badge_flow.remove(child)
            child = nxt

        badges: list[tuple[str, str]] = []
        if self.treated_as_markdown:
            badges.append(("Parsed as markdown prose", "accent-badge"))
        elif self.current_route is not None and self.current_route.raw_text:
            badges.append(("Structure preserved verbatim", "pill-badge"))
        if self.current_route is None:
            badges.append(("Unsupported file type", "warning-badge"))
        elif self.binary_error is not None:
            badges.append(("Missing dependency", "warning-badge"))

        for text, css_class in badges:
            label = Gtk.Label(label=text, css_classes=["pill", css_class])
            self.badge_flow.append(label)
        self.badge_flow.set_visible(bool(badges))

    # ---- format selection -----------------------------------------------

    def _refresh_format_group(self) -> None:
        for child in self._format_group_children:
            self.format_group.remove(child)
        self._format_group_children = []

        self._toggle_buttons = []
        self.selected_target = None
        self.binary_banner.set_revealed(False)

        route = self.current_route

        if route is None:
            self.format_group.set_visible(True)
            self.format_group.set_title("Convert to")
            self.format_group.set_description(None)
            row = Adw.ActionRow(
                title="No conversion route available",
                subtitle=f"any2any doesn't know how to convert '{self.current_mime}'.",
            )
            row.add_prefix(Gtk.Image.new_from_icon_name("dialog-warning-symbolic"))
            self.format_group.add(row)
            self._format_group_children.append(row)
            self.output_group.set_visible(False)
            self._update_convert_sensitivity()
            return

        self.format_group.set_visible(True)
        self.format_group.set_title("Convert to")
        self.format_group.set_description(f"via {route.tool.value}")

        flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=6, row_spacing=8, column_spacing=8)
        flow.set_margin_top(4)
        flow.set_margin_bottom(4)

        first_button: Gtk.ToggleButton | None = None
        for fmt in route.targets:
            btn = Gtk.ToggleButton(label=f".{fmt}")
            btn.add_css_class("pill")
            btn.add_css_class("format-pill")
            btn.connect("toggled", self._on_format_toggled, fmt)
            if first_button is None:
                first_button = btn
            flow.append(btn)
            self._toggle_buttons.append(btn)

        for btn in self._toggle_buttons[1:]:
            btn.set_group(self._toggle_buttons[0])

        wrapper_row = Gtk.ListBoxRow(selectable=False, activatable=False)
        wrapper_row.set_child(flow)
        wrapper_row.add_css_class("format-row")
        self.format_group.add(wrapper_row)
        self._format_group_children.append(wrapper_row)

        if self.binary_error is not None:
            self.binary_banner.set_title(self.binary_error)
            self.binary_banner.set_revealed(True)

        self.output_group.set_visible(True)
        self._output_auto_follow = True

        if first_button is not None:
            first_button.set_active(True)  # triggers _on_format_toggled

    def _on_format_toggled(self, button: Gtk.ToggleButton, fmt: str) -> None:
        if not button.get_active():
            return
        self.selected_target = fmt
        if self._output_auto_follow and self.current_file is not None:
            default_path = self.current_file.with_suffix(f".{fmt}")
            self._set_output_programmatically(str(default_path))
        self._update_convert_sensitivity()

    # ---- output path ------------------------------------------------------

    def _set_output_programmatically(self, text: str) -> None:
        self._programmatic_output_update = True
        self.output_row.set_text(text)
        self._programmatic_output_update = False

    def _on_output_changed(self, entry: Adw.EntryRow) -> None:
        if getattr(self, "_programmatic_output_update", False):
            return
        self._output_auto_follow = False
        self._update_convert_sensitivity()

    def _pick_output_path(self) -> None:
        dialog = Gtk.FileChooserDialog(
            title="Save converted file as",
            transient_for=self,
            action=Gtk.FileChooserAction.SAVE,
        )
        dialog.add_buttons(
            "_Cancel", Gtk.ResponseType.CANCEL,
            "_Save", Gtk.ResponseType.ACCEPT,
        )
        dialog.set_default_size(800, 600)

        if self.current_file is not None and self.selected_target is not None:
            suggested = self.current_file.with_suffix(f".{self.selected_target}")
            dialog.set_current_folder(Gio.File.new_for_path(str(suggested.parent)))
            dialog.set_current_name(suggested.name)

        def on_response(dialog_widget, response):
            if response == Gtk.ResponseType.ACCEPT:
                gfile = dialog_widget.get_file()
                if gfile:
                    path = gfile.get_path()
                    if path:
                        self._output_auto_follow = False
                        self._set_output_programmatically(path)
            dialog_widget.destroy()

        dialog.connect("response", on_response)
        dialog.present()

    # ---- conversion ------------------------------------------------------

    def _update_convert_sensitivity(self) -> None:
        ready = (
            self.current_file is not None
            and self.current_route is not None
            and self.selected_target is not None
            and self.binary_error is None
            and bool(self.output_row.get_text().strip())
        )
        self.convert_button.set_sensitive(bool(ready))

    def _on_convert_clicked(self) -> None:
        assert self.current_file is not None
        assert self.current_route is not None
        output_path = Path(self.output_row.get_text().strip()).expanduser()

        if output_path.exists():
            self._confirm_overwrite(output_path)
        else:
            self._start_conversion(output_path)

    def _confirm_overwrite(self, output_path: Path) -> None:
        dialog = Adw.AlertDialog(
            heading="File already exists",
            body=f"'{output_path.name}' already exists. Overwrite it?",
        )
        dialog.add_response("cancel", "Cancel")
        dialog.add_response("overwrite", "Overwrite")
        dialog.set_response_appearance("overwrite", Adw.ResponseAppearance.DESTRUCTIVE)
        dialog.set_default_response("cancel")
        dialog.set_close_response("cancel")

        def on_response(_dialog: Adw.AlertDialog, response: str) -> None:
            if response == "overwrite":
                self._start_conversion(output_path)

        dialog.connect("response", on_response)
        dialog.present(self)

    def _start_conversion(self, output_path: Path) -> None:
        assert self.current_file is not None
        assert self.current_route is not None
        route = self.current_route
        input_path = self.current_file

        self._clear_error()
        self.convert_button.set_sensitive(False)
        self.progress_label.set_label(f"Converting {input_path.name}…")
        self.progress_box.set_visible(True)
        self.progress_spinner.start()

        def worker() -> None:
            try:
                result = run_conversion(route, input_path, output_path)
            except BinaryNotFoundError as exc:
                result = ConversionResult(
                    success=False,
                    output_path=output_path,
                    command=(),
                    stdout="",
                    stderr=str(exc),
                    returncode=-1,
                )
            except (Exception, SystemExit) as exc:
                result = ConversionResult(
                    success=False,
                    output_path=output_path,
                    command=("Backend Error",),
                    stdout="",
                    stderr=traceback.format_exc(),
                    returncode=-1,
                )
            GLib.idle_add(self._on_conversion_done, result)

        threading.Thread(target=worker, daemon=True).start()

    def _on_conversion_done(self, result: ConversionResult) -> bool:
        self.progress_spinner.stop()
        self.progress_box.set_visible(False)
        self._update_convert_sensitivity()

        if result.success:
            toast = Adw.Toast(title=f"Saved to {result.output_path}")
            toast.set_button_label("Show in Folder")
            toast.connect("button-clicked", lambda *_: self._reveal_in_folder(result.output_path))
            self.toast_overlay.add_toast(toast)
        else:
            self._show_error(result)

        return GLib.SOURCE_REMOVE

    def _reveal_in_folder(self, path: Path) -> None:
        launcher = Gtk.FileLauncher(file=Gio.File.new_for_path(str(path)))
        launcher.open_containing_folder(self, None, None)

    # ---- error display ------------------------------------------------------

    def _show_error(self, result: ConversionResult) -> None:
        for child in self._error_group_children:
            self.error_group.remove(child)
        self._error_group_children = []

        summary = f"Exit code {result.returncode}"
        expander = Adw.ExpanderRow(title="Conversion failed", subtitle=summary)
        expander.add_prefix(Gtk.Image.new_from_icon_name("dialog-error-symbolic"))

        if result.command:
            cmd_row = Adw.ActionRow(title="Command", subtitle=" ".join(result.command))
            cmd_row.set_subtitle_selectable(True)
            expander.add_row(cmd_row)

        stderr_text = result.stderr.strip() or "(no error output)"
        text_view = Gtk.TextView(editable=False, cursor_visible=False, wrap_mode=Gtk.WrapMode.WORD_CHAR)
        text_view.add_css_class("error-output")
        text_view.get_buffer().set_text(stderr_text)
        text_scroller = Gtk.ScrolledWindow(min_content_height=140, max_content_height=240)
        text_scroller.set_child(text_view)
        text_scroller.set_margin_start(12)
        text_scroller.set_margin_end(12)
        text_scroller.set_margin_bottom(12)
        expander.add_row(text_scroller)

        self.error_group.add(expander)
        self._error_group_children.append(expander)
        self.error_group.set_visible(True)

    def _clear_error(self) -> None:
        for child in self._error_group_children:
            self.error_group.remove(child)
        self._error_group_children = []
        self.error_group.set_visible(False)

    def _show_toast(self, message: str) -> None:
        self.toast_overlay.add_toast(Adw.Toast(title=message))

    def _show_about(self) -> None:
        about = Adw.AboutDialog(
            application_name="any2any",
            application_icon="document-send-symbolic",
            version="0.1.0",
            developer_name="any2any",
            comments="A universal file converter that routes to ffmpeg, ImageMagick, and pandoc.",
            license_type=Gtk.License.GPL_3_0, 
        )
        about.present(self)


class Any2AnyApplication(Adw.Application):
    def __init__(self) -> None:
        super().__init__(application_id=APP_ID, flags=Gio.ApplicationFlags.HANDLES_OPEN)
        self.window: MainWindow | None = None

    def do_startup(self) -> None:
        Adw.Application.do_startup(self)
        Adw.StyleManager.get_default().set_color_scheme(Adw.ColorScheme.PREFER_DARK)

        if CSS_PATH.exists():
            provider = Gtk.CssProvider()
            provider.load_from_path(str(CSS_PATH))
            Gtk.StyleContext.add_provider_for_display(
                Gdk.Display.get_default(),
                provider,
                Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION,
            )

        open_action = Gio.SimpleAction.new("open-file", None)
        open_action.connect("activate", lambda *_: self.window._pick_file() if self.window else None)
        self.add_action(open_action)
        self.set_accels_for_action("win.open-file", ["<Control>o"])

    def do_activate(self) -> None:
        if self.window is None:
            self.window = MainWindow(self)
        self.window.present()

    def do_open(self, files: list[Gio.File], n_files: int, hint: str) -> None:
        self.do_activate()
        if self.window is not None and n_files > 0:
            path = files[0].get_path()
            if path:
                self.window._load_file(Path(path))


def main() -> int:
    app = Any2AnyApplication()
    return app.run(None)


if __name__ == "__main__":
    raise SystemExit(main())
