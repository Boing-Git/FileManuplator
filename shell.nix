{ pkgs ? import <nixpkgs> {} }:

# Development shell for FileManipulator: Rust + GTK4/Libadwaita.
pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    pkg-config
    gobject-introspection
    cargo
    rustc
    rustfmt
    clippy
  ];

  buildInputs = with pkgs; [
    # GUI
    gtk4
    libadwaita
    glib
    pango
    cairo
    gdk-pixbuf
    gsettings-desktop-schemas
    glib-networking
    dbus
    # HTML Runner's phone-sized preview
    webkitgtk_6_0

    # Marketplace downloads over TLS
    openssl

    # Conversion engines the app routes work to
    ffmpeg
    imagemagick
    pandoc
    wkhtmltopdf
    mame-tools
    par2cmdline
    file
  ];

  shellHook = ''
    export XDG_DATA_DIRS="${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk4}/share/gsettings-schemas/${pkgs.gtk4.name}:$XDG_DATA_DIRS"
    export GIO_MODULE_DIR="${pkgs.glib-networking}/lib/gio/modules/"
    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath (with pkgs; [ openssl ])}:$LD_LIBRARY_PATH"

    # Fix for Nix cc-wrapper bug splitting the `out` variable by spaces
    unset out
    # Also strip any bad -L paths from NIX_LDFLAGS that were already injected
    export NIX_LDFLAGS="$(echo $NIX_LDFLAGS | sed 's/-L[^ ]*pc backup[^ ]*//g')"
  '';
}
