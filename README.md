# filemanuplator 🔄

**filemanuplator** is a universal file converter wrapped in a modern, responsive GTK4 + Libadwaita graphical interface. 

Rather than reinventing the wheel, filemanuplator acts as a smart routing layer on top of industry-standard conversion engines (**FFmpeg**, **ImageMagick**, and **Pandoc**). It detects files by their actual content (MIME type)—not just their extension—and automatically routes them to the correct tool with the optimal arguments.

## ✨ Features

* **Modern GTK4/Libadwaita UI:** Fully responsive design that adapts to tiling window managers and floating desktop environments seamlessly.
* **Content-Based Detection:** Uses strict MIME type detection to figure out what a file *actually* is.
* **Intelligent Routing:** 
  * Videos and Audio -> **FFmpeg**
  * Images -> **ImageMagick** (Animated GIFs route safely to FFmpeg to prevent memory exhaustion)
  * Documents -> **Pandoc**
* **Raw-Text Preservation:** Automatically detects configuration files, scripts, and logs, wrapping them safely to preserve indentation and syntax instead of mangling them as markdown prose.
* **Safety First:** Includes missing-binary detection, clear error logs, and overwrite protection dialogs.

---

## 📦 Installation (End Users)

The easiest way to use filemanuplator is to download the pre-packaged release, which includes statically compiled versions of the heavy conversion engines (FFmpeg, ImageMagick, Pandoc) so you don't have to install them manually.

### 1. Install System Dependencies
Because this is a native GTK4 Python application, you must install the PyGObject bindings via your system's package manager:

* **Arch Linux:** `sudo pacman -S python-gobject gtk4 libadwaita`
* **Ubuntu/Debian:** `sudo apt install python3-gi gir1.2-gtk-4.0 gir1.2-adw-1`
* **Fedora:** `sudo dnf install python3-gobject gtk4 libadwaita`

### 2. Download and Run
1. Head over to the [Releases page](../../releases) and download the latest `filemanuplator-linux-x86_64.tar.gz`.
2. Extract the archive.
3. Open your terminal in the extracted folder and run:
   ```bash
   ./filemanuplator
