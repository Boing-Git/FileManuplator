{ pkgs ? import <nixpkgs> { } }:

# The desktop app as a Nix package, for NixOS (prebuilt Linux binaries cannot
# find their libraries there):
#   nix-build && ./result/bin/filemanuplator
#   nix-env -if .                      # install for your user
# or add `(import /path/to/FM { inherit pkgs; })` to environment.systemPackages.
let
  inherit (pkgs) lib;
  # The conversion engines plugins and the Converter call.
  engines = with pkgs; [ ffmpeg imagemagick pandoc file par2cmdline mame-tools ];
in
pkgs.rustPlatform.buildRustPackage {
  pname = "filemanuplator";
  version = "0.1.0";

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./crates/fm-core
      ./crates/fm-gtk
      ./packaging/linux/icon.png
    ];
  };
  cargoLock.lockFile = ./Cargo.lock;
  cargoBuildFlags = [ "-p" "fm-gtk" ];
  # The tests need the FM-Plugins checkout and the engines; run them in nix-shell.
  doCheck = false;

  nativeBuildInputs = with pkgs; [ pkg-config wrapGAppsHook4 ];
  buildInputs = with pkgs; [ gtk4 libadwaita glib-networking openssl ];

  postInstall = ''
    mv $out/bin/fm-gtk $out/bin/filemanuplator
    install -Dm644 packaging/linux/icon.png $out/share/icons/hicolor/256x256/apps/filemanuplator.png
    install -Dm644 /dev/stdin $out/share/applications/filemanuplator.desktop <<EOF
    [Desktop Entry]
    Name=FileManipulator
    Comment=Universal file converter
    Exec=filemanuplator %U
    Icon=filemanuplator
    Type=Application
    Terminal=false
    Categories=Utility;
    EOF
  '';

  preFixup = ''
    gappsWrapperArgs+=(
      --prefix PATH : ${lib.makeBinPath engines}
      # WebKitGTK is loaded at run time (dlopen) for the HTML Runner's preview.
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath [ pkgs.webkitgtk_6_0 ]}
      # Fallback icons and the MIME database (WebKit needs it to see that a
      # file:// page is HTML), after the system's own.
      --suffix XDG_DATA_DIRS : ${lib.makeSearchPath "share" (with pkgs; [ adwaita-icon-theme hicolor-icon-theme shared-mime-info ])}
    )
  '';

  meta = {
    description = "Universal file converter with Material 3 Expressive plugins";
    homepage = "https://github.com/Boing-Git/FileManuplator";
    mainProgram = "filemanuplator";
    platforms = lib.platforms.linux;
  };
}
