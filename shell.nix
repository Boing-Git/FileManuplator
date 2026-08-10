{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  packages = [
    # Python and required pip packages
    (pkgs.python3.withPackages (python-pkgs: [
      python-pkgs.typer
      python-pkgs.rich
      python-pkgs.python-magic
    ]))
    
    # System dependencies for routing
    pkgs.ffmpeg
    pkgs.imagemagick
    pkgs.pandoc
    
    # Required for python-magic to function
    pkgs.file
  ];
}
