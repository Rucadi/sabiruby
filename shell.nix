{ pkgs ? import <nixpkgs> { } }:

let
  inherit (pkgs) lib;
  runtimeLibraries = with pkgs; [
    alsa-lib
    libpulseaudio
    libx11
    libxkbcommon
    udev
    vulkan-loader
    wayland
  ];
in
pkgs.mkShell {
  packages = with pkgs; [
    cargo
    rustc
    rustfmt
    clippy
    gcc
    cmake
    lld
  ];

  nativeBuildInputs = with pkgs; [
    pkg-config
  ];

  # Keep the dynamically linked WGPU/Winit/CPAL binary runnable from the
  # development shell. This is deliberately scoped to nix-shell; the player
  # itself does not inspect or require environment variables.
  buildInputs = runtimeLibraries ++ (with pkgs; [
    libffi
    zlib
  ]);
  LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibraries;

  # sabiruby-compiler builds the vendored mruby compiler through cc.
}
