{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    gobject-introspection
    wrapGAppsHook4
  ];

  buildInputs = with pkgs; [
    # Python and required pip packages
    (python3.withPackages (python-pkgs: [
      python-pkgs.typer
      python-pkgs.rich
      python-pkgs.python-magic
      python-pkgs.pygobject3
    ]))
    
    # GUI Dependencies
    gtk4
    libadwaita
    gsettings-desktop-schemas # <-- Provides the missing desktop schemas
    glib                      # <-- Provides schema compiler tools

    # System dependencies for routing
    ffmpeg
    imagemagick
    pandoc
    
    # Required for python-magic to function
    file
  ];

  # Explicitly link the schemas into the environment so the GTK fallback picker works
  shellHook = ''
    export XDG_DATA_DIRS=${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk4}/share/gsettings-schemas/${pkgs.gtk4.name}:$XDG_DATA_DIRS
  '';
}
