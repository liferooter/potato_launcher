{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };

  outputs = {
    nixpkgs,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {inherit system;};
    in {
      devShells.default = pkgs.mkShell {
        shellHook = ''
          export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath (with pkgs; [
            xorg.libX11
            xorg.libXext
            xorg.libXcursor
            xorg.libXrandr
            xorg.libXxf86vm
            xorg.libXrender
            xorg.libXtst
            xorg.libXi
            xorg.xrandr
            libpulseaudio
            libGL
            glfw
            openal

            wayland
            libxkbcommon
          ])}
        '';
        buildInputs = with pkgs; [
          # flatpak build deps
          (python3.withPackages (ps: with ps; [aiohttp toml]))
          flatpak-builder
        ];
        nativeBuildInputs = with pkgs; [
          cargo
          rustc
        ];
      };
    });
}
