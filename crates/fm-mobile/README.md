# fm-mobile

filemanuplator for Android and iOS, built with [Tauri 2](https://v2.tauri.app).

| Path | What it is |
| :--- | :--- |
| `ui/` | The whole interface: plain HTML, CSS and JavaScript, no build step. M3 Expressive, with the same colour roles and schemes as the desktop app. |
| `src-tauri/src/lib.rs` | The Rust side: lists the bundled plugins, generates colour schemes (via `fm-core`), and runs a pipeline on the bytes of the file the page picked. |
| `src-tauri/build.rs` | Compiles every `.toml` from an FM-Plugins checkout into the app. |
| `src-tauri/Info.ios.plist` | Merged into the iOS app's Info.plist: results saved to the app's Documents folder show up in the Files app. |

The page reads files with an ordinary `<input type="file">`, sends them to Rust, and saves results through the system save dialog. Rust writes the file itself (an Android `content://` URI via the fs plugin; on iOS the dialog exports a copy staged in Documents).

## Plugins

`build.rs` bundles the plugins from `$FM_PLUGINS_DIR`, or from `../../../../FM-Plugins` (an FM-Plugins checkout next to this repository) when that is unset. Plugins whose pipeline starts a program (`exec`, `ffmpeg`, `magick`, `pandoc`, `wkhtmltopdf`, `chdman`, `par2`) are listed as desktop-only in the app instead of getting a tab.

## Building

Release builds come from `.github/workflows/mobile.yml` on every `v*` tag (and on demand from the Actions tab). To build yourself you need Node.js, Rust and:

* **Android:** the Android SDK and NDK (`ANDROID_HOME`, `NDK_HOME`), a JDK 17 and `rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`.
* **iOS:** macOS with Xcode, `brew install xcodegen` and `rustup target add aarch64-apple-ios`.

```bash
cd crates/fm-mobile
npm ci                                       # the Tauri CLI
npx tauri android init                       # or: npx tauri ios init
npx tauri icon ../../packaging/icon/icon.png
npx tauri android build --apk --aab          # or: npx tauri ios build --no-sign
```

The generated `src-tauri/gen/` projects are not committed; `init` recreates them.

### Signing

* **Android:** CI signs the APK and AAB with the keystore in the `ANDROID_KEYSTORE_BASE64` secret (base64 of a `.jks`), `ANDROID_KEYSTORE_PASSWORD` and `ANDROID_KEY_ALIAS`. Without those it signs with a throwaway key: the APK installs, but every release then has a different key, so Android will not install one over another, and Google Play will not accept the AAB.
* **iOS:** the IPA is unsigned. Sideload it with AltStore or Sideloadly, or re-sign it with your own certificate.

### Trying the interface on a computer

`cargo run` in `src-tauri/` opens the same app in a desktop window (it needs WebKitGTK 4.1 and libsoup 3 on Linux).
