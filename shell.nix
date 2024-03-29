let
  pkgs = import (fetchTarball ("channel:nixpkgs-unstable")) { };

  libraries = with pkgs;[
    # tauri
    webkitgtk
    gtk3
    cairo
    gdk-pixbuf
    glib
    dbus
    openssl_3

    # tray icon
    libappindicator
  ];

  packages = with pkgs; [
    # tauri
    pkg-config
    dbus
    openssl_3
    glib
    gtk3
    libsoup
    webkitgtk
    appimagekit
    cargo-tauri
    nodejs_21

    # rust
    rustc
    rustfmt
    cargo
    gcc
    clippy
    rust-analyzer
  ];
in
pkgs.mkShell {
  buildInputs = packages;

  RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";

  shellHook =
    ''
      export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath libraries}:$LD_LIBRARY_PATH
      export XDG_DATA_DIRS=${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:$XDG_DATA_DIRS
    '';
}
