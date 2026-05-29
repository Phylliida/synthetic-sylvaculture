{ pkgs ? import <nixpkgs> { } }:

# Dev shell for the native three-d viewer. On NixOS the windowing client
# libraries (Wayland / X11 / xkbcommon / Xcursor) are dlopen'd by winit at
# runtime and are not on the default loader path, so we add them to
# LD_LIBRARY_PATH here. GL itself comes from /run/opengl-driver/lib.
let
  runtimeLibs = with pkgs; [
    wayland
    libxkbcommon
    libGL
    xorg.libX11
    xorg.libXcursor
    xorg.libXrandr
    xorg.libXi
    xorg.libxcb
  ];
in
pkgs.mkShell {
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = runtimeLibs;
  shellHook = ''
    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath runtimeLibs}:/run/opengl-driver/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  '';
}
