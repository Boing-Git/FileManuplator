# filemanuplator 🔄

**filemanuplator** is a fast, extensible universal file converter and manipulator. The desktop app is built with Rust, GTK4 and libadwaita; the same engine and the same plugins also run in an Android and iOS app. Both follow Material 3 Expressive.

Rather than reinventing the wheel, filemanuplator acts as a smart routing and orchestration layer on top of industry-standard conversion engines (**FFmpeg**, **ImageMagick**, and **Pandoc**). It detects files by their content (MIME type) and supports dynamically loaded plugins to provide rich, specialized editing workflows.

---

## ✨ Features

* **Material 3 Expressive interface:** an M3 navigation bar (one destination per tool, with a pill indicator that grows from the icon), segmented lists, pill buttons that change shape when pressed, M3E checkboxes, radios and sliders, and one set of motion curves throughout.
* **Colour schemes you choose:** the palette button in the header bar opens a picker with six preset schemes, your desktop's own accent colour, or any custom colour, plus Auto / Light / Dark. Every colour in the app is an M3 colour role generated from that seed with Google's Material colour algorithm, so the whole app (plugin tabs included) changes together. The choice is saved in `~/.config/filemanuplator/appearance.toml`.
* **Colour roles for plugin authors:** a plugin can give any widget `color = "error"` (or `primary`, `secondary`, `tertiary`) to show danger or emphasis. The role follows whatever scheme the user picked, in light and dark.
* **HTML Runner:** pick an HTML file and see it run the way a phone shows it, live in a phone-sized frame on desktop (it runs again every time you save the file) and full-screen in the mobile app.
* **Mobile app:** Android (APK and AAB) and iOS (unsigned IPA) builds that bundle the plugins. Tools that only need the built-in engine (PhotoForge, the HTML Runner, image conversion) work on the phone; plugins that need FFmpeg, ImageMagick or Pandoc stay desktop-only and the app says so.
* **Declarative TOML Plugin Architecture:** Create complete custom tools and conversion pipelines by defining a single TOML file—no app recompilation required.
* **Interactive Image Canvas:** Built-in interactive crop/canvas tool featuring rule-of-thirds grid, corner resize handles, selection translation (middle drag), aspect ratio locking, and strict image bounds clamping.
* **Multi-Engine Pipeline Execution:** Seamlessly chain external tools (`ImageMagick`, `FFmpeg`, `Pandoc`, `par2`, `chdman`) with dynamic variable interpolation. Commands are run directly, never through a shell.
* **Content-Based Detection:** Strict MIME type matching ensures optimal tool selection regardless of file extension.

---

## 🔌 TOML Plugin Specification

Plugins allow developers and users to build fully native tools and workflows simply by dropping a `.toml` file into:
- User plugin directory: `~/.local/share/filemanuplator/plugins/*.toml`
- The official plugins live in [FM-Plugins](https://github.com/Boing-Git/FileManuplator-Plugins) and can be installed from **Plugins & Marketplace**. The mobile app has them built in.

### 1. Metadata Block `[plugin]`

```toml
[plugin]
name = "PhotoForge"
version = "1.0"
description = "Native photo editing, resizing, cropping, and target compression tool."
change_extension = false # false = preserve original file extension, true = apply custom format
```

### 2. Tab Configuration `[tab]`

Gives the plugin its own destination in the navigation bar.

```toml
[tab]
title = "PhotoForge"
icon = "image-x-generic-symbolic" # Any freedesktop symbolic icon name
```

Keep `title` short: it is the label under the icon. Destinations cannot be closed, so `closable` (from older plugins) is ignored. Alt+1 … Alt+9 jump to the destinations in order.

### 3. Declarative UI Widgets `[[ui]]`

Widgets are rendered in order inside the tab's layout.

| Widget Type | Key Properties | Description |
| :--- | :--- | :--- |
| `file_picker` | `id`, `label`, `filter` | File picker button that loads paths and detects image metadata. Automatically populates `{file.path}`, `{file.width}`, `{file.height}`, `{file.aspect_ratio}`. `filter` limits the chooser, e.g. `filter = ["image/*"]` or `filter = ["*.mp4"]`. |
| `picker` | `id`, `label`, `options`, `default`, `overwrites` | Dropdown picker. When `overwrites` is specified (e.g. `overwrites = ["width", "height"]`), selecting a preset (e.g. `"Avatar {400x400}"`) automatically populates target input values. |
| `radio_group` | `id`, `label`, `options`, `default` | Radio selection control (e.g. for switching crop modes). |
| `image_canvas` | `id`, `image_source`, `lock_aspect_ratio`, `aspect_ratio_source`, `show_grid`, `shade_outside`, `show_handles`, `draggable_selection`, `lock_inside_image` | Rich interactive image canvas and cropping tool (see detailed properties below). |
| `text_input` | `id`, `label`, `default` | Single-line text/number input field. |
| `slider` | `id`, `label`, `min`, `max`, `default` | Numeric slider; the current value is shown beside it. |
| `button` | `id`, `label`, `on_click` | Action trigger. `on_click = "execute_pipeline"` prompts for save destination and executes the pipeline. |
| `notice` | `text` | A message in a tonal container, e.g. a warning before something destructive. It needs no `id`. |
| `html_view` | `id`, `source` | Runs an HTML file, e.g. `source = "{file.path}"` after a `file_picker`. On desktop it is a phone-sized frame (360 × 740, a phone's user agent) that runs the page again whenever the file is saved, with right-click → Inspect Element; in the mobile app it fills the screen and has a full-screen button. A plugin with an `html_view` needs no pipeline or button. |

Every widget also accepts `color` (see below).

#### Colour roles: `color`

```toml
[[ui]]
type = "notice"
text = "This permanently deletes the original file. There is no undo."
color = "error"

[[ui]]
type = "button"
id = "go"
label = "Delete forever"
on_click = "execute_pipeline"
color = "error"
```

`color` names an M3 colour role: `primary`, `secondary`, `tertiary` or `error`. It is a role, not a colour, so it always matches the scheme the user picked, in light and dark, on desktop and on the phone. What it changes:

| Widget | With `color` |
| :--- | :--- |
| `button` | Filled with the role (default: primary). |
| `notice` | The role's container colour (default: secondary). |
| `file_picker`, `picker` | The label, and the button or dropdown in the role's container colour. |
| `radio_group`, `slider`, `text_input` | The label, and the selected radio, the slider's track and handle, or the field's focus line. |

`image_canvas` and `html_view` show other content, so `color` has no effect on them (the validator warns). A misspelled role is an error with a suggestion: `unknown color 'eror' (did you mean 'error'?)`.

#### `image_canvas` Detailed Options

- `image_source`: Variable expression pointing to the image file (e.g. `"{file.path}"`).
- `lock_aspect_ratio`: Variable expression linking to a mode selector (e.g. `"{crop_mode}"`). When set to `"Locked to Aspect Ratio"`, forces the crop box to adhere to the active aspect ratio.
- `aspect_ratio_source`: Variable expression linking to preset dimensions or aspect ratio (e.g. `"{preset_size}"`). Automatically resolves `"{width}x{height}"` patterns or image aspect ratio.
- `show_grid`: `true` or `false` — Displays rule-of-thirds alignment grid lines inside the active selection.
- `shade_outside`: Float `0.0 - 1.0` — Dimming opacity for areas outside the crop box.
- `show_handles`: `true` or `false` — Renders interactive resize handles on the corners of the crop box.
- `draggable_selection`: `true` or `false` — Allows clicking and dragging the interior of the crop box to move/translate the selection.
- `lock_inside_image`: `true` or `false` — When `true`, strictly clamps crop creation, translation, and corner resizing within the visible image boundary.

### 4. Processing Pipeline `[pipeline]`

Chains sequential operations to transform the file. Template variables (`{file.path}`, `{output.path}`, widget ids such as `{width}`, and canvas values such as `{preview_canvas.crop_data}`) are evaluated at runtime. Steps run in `step` order. A plugin should name its pipeline `default` (`[[pipeline.default]]`) or, as in most plugins, use `[[pipeline.process]]` when it has only one.

```toml
[pipeline]
[[pipeline.process]]
step = 1
action = "export_image"
target_file = "{file.path}"
output_file = "{output.path}"
crop_data = "{preview_canvas.crop_data}"
width = "{width}"
height = "{height}"
fit = "{fit_mode}"         # contain, cover or stretch
max_kb = "{target_size}"  # highest JPEG quality that fits

[[pipeline.process]]
step = 2
action = "checksum"
condition = "{file.size} > 40000"
algorithm = "sha256"
destination = "{output.dir}/{file.name}.sha256"
```

Available actions include `copy`, `regex_copy`, `delete`, `echo`, `checksum`, `split_bytes`, `join_bytes`, `split_csv`, `join_csv`, `write_text`, `resize`, `crop_image_visually`, `export_image`, `rotate`, `adjust`, `convert_format`, `create_directories`, the general-purpose `exec`, and the external-tool shorthands `ffmpeg`, `magick`, `pandoc`, `wkhtmltopdf`, `chdman`, `par2`. Each action's fields and examples, and the full `condition` syntax (`==`, `<`, `>`, `and`, `or`, `not`, `in`, `.startswith()`), are documented in the [FM-Plugins README](https://github.com/Boing-Git/FileManuplator-Plugins#readme).

**On phones** there are no external programs, so a plugin that uses `exec`, `ffmpeg`, `magick`, `pandoc`, `wkhtmltopdf`, `chdman` or `par2` is shown as desktop-only in the mobile app. Everything else, `export_image` included, runs on the phone. The mobile app saves one result per run, the file a pipeline writes to `{output.path}`.

### 5. Validation

Plugins are checked when they are loaded. A file with an unknown action, a misspelled setting, a duplicate widget id, a `{placeholder}` that refers to nothing, an unknown colour role, or a missing required field is **not** loaded silently — it is listed with the reason in **Plugins & Marketplace**, naming the step or widget at fault:

```
video_trimmer.toml
error: step 2: unknown action 'ffmepg' (did you mean 'ffmpeg'?)
```

### 6. How commands are run

External tools are executed directly, never through a shell. An `args` string is split into arguments first, and `{variables}` are substituted into those arguments afterwards. Two consequences:

* File names containing spaces, apostrophes or any other punctuation work without the plugin author doing anything.
* A value coming from a text box or a file name cannot add arguments or run a second command.

---

## 🎨 Colour schemes

The palette button in the header bar (on the phone: the top app bar) opens the picker.

* **Desktop accent** (desktop only) follows your desktop theme's accent colour, read when the app starts. If your `~/.config/gtk-4.0/gtk.css` sets one (Matugen and similar wallpaper-based generators do), the app's palette grows from it.
* **Lavender, Ocean, Forest, Ember, Rose, Sand** are fixed seeds.
* **Custom colour…** (desktop) grows a scheme from any colour you pick.
* **Auto / Light / Dark**: Auto follows the system's light or dark preference and switches live.

The scheme is generated with the Material colour algorithm (the `material-colors` crate, tonal-spot variant), so every role (primary, containers, surfaces, outlines…) keeps M3 contrast in both light and dark. On desktop the scheme is applied above the user's `gtk.css`, so a wallpaper-generated GTK theme cannot half-override the scheme you picked here; choose **Desktop accent** to stay in step with it.

---

## 📱 Mobile app

`crates/fm-mobile` is a Tauri 2 app for Android and iOS. Its interface (`ui/`) is plain HTML, CSS and JavaScript with the same M3 Expressive components, colour roles and schemes as the desktop app; its Rust side runs pipelines with the same `fm-core` engine. The plugins from FM-Plugins are compiled in.

* **Converter:** convert images between PNG, JPG, WebP, GIF, BMP and TIFF on the device. Other file types need the desktop engines.
* **Plugin tools:** every plugin with a `[tab]` that needs no external program, e.g. PhotoForge and the HTML Runner.
* **Saving:** **Save** opens the system's save dialog (on iOS also see Files › On My iPhone › FileManipulator).
* **Android back** closes the colour sheet or leaves the HTML Runner's full screen first.

Release builds are made by CI (see below). To build one yourself, see [crates/fm-mobile/README.md](crates/fm-mobile/README.md).

---

## 🛠️ Building & Running

FileManipulator's desktop app is a Rust application with a GTK4 / Libadwaita frontend.

### NixOS

Prebuilt Linux binaries (the tarball and the bare executable) cannot find their libraries on NixOS. Build the Nix package instead; it brings GTK, the conversion engines and WebKit with it:
```bash
nix-build                      # then ./result/bin/filemanuplator
nix-env -if .                  # or install it for your user
```
or add `(import /path/to/FM { inherit pkgs; })` to `environment.systemPackages`. The AppImage also works through `appimage-run`.

### Using Nix Shell (development)
```bash
nix-shell
cargo run -p fm-gtk
```

### Manual Build
Install the system dependencies (GTK4 and Libadwaita development packages, a C
toolchain, and the conversion engines; WebKitGTK 6.0 is optional), then build:
```bash
bash scripts/install_dependencies.sh   # system packages
cargo build --release -p fm-gtk
./target/release/fm-gtk
```
Rust itself comes from [rustup](https://rustup.rs) or your distribution.

### Tests
```bash
cargo test --workspace
```
Some tests run the real plugins from an [FM-Plugins](https://github.com/Boing-Git/FileManuplator-Plugins) checkout next to this repository (`../FM-Plugins`) and shell out to `ffmpeg`; `nix-shell` provides ffmpeg, and the tests skip themselves when either is missing.

### Packaging
```bash
bash packaging/linux/build_linux.sh    # release tarball with an installer
bash packaging/linux/install.sh        # install into ~/.local
```

### Releases
Pushing a `v*` tag runs both workflows and attaches everything to that tag's GitHub release:

| File | Built by |
| :--- | :--- |
| `filemanuplator-linux-x86_64` (the bare executable), `FileManuplator-Linux.tar.gz`, `FileManipulator-x86_64.AppImage`, `filemanuplator.flatpak` | `build.yml` |
| `FileManipulator-android.apk` (installable), `FileManipulator-android.aab` (for Google Play) | `mobile.yml` |
| `FileManipulator-ios-unsigned.ipa` | `mobile.yml` |

The APK and AAB are signed with your key when the repository has the `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD` and `ANDROID_KEY_ALIAS` secrets; otherwise with a throwaway key, which installs fine but cannot update an install signed differently. The IPA is unsigned: sideload it with a tool such as AltStore or Sideloadly, or sign it yourself.

What each Linux file needs:

| File | Needs on the system |
| :--- | :--- |
| AppImage | Nothing (FUSE to mount it, or run it with `--appimage-extract-and-run`) |
| Flatpak | Flatpak with Flathub (it installs the GNOME runtime) |
| Tarball, bare executable | GTK 4.14+ and libadwaita 1.5+ (Ubuntu 24.04, Debian 13, Fedora 40, Arch and newer); `install.sh` names anything missing |

WebKitGTK 6.0 is optional everywhere: it is loaded only when the HTML Runner shows a page. Without it the app runs normally and the HTML Runner offers to open the page in your browser.

## 📦 Repository Layout

| Path | Contents |
| :--- | :--- |
| `crates/fm-core` | Plugin schema, validation, condition evaluator, pipeline engine, conversion routing, colour schemes |
| `crates/fm-gtk` | The GTK4 / Libadwaita desktop app |
| `crates/fm-mobile` | The Android / iOS app (Tauri 2). Built by the Tauri CLI, so it is outside the Cargo workspace. |
| `packaging/icon` | The app icon (SVG source and 1024 px PNG) |
| `packaging/linux` | Tarball installer, Flatpak manifest |

The PhotoForge plugin (in FM-Plugins) is a native port of the [PhotoForge](https://github.com/surinder83singh/PhotoForge) web app.
