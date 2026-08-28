{
  description = "ChemSpec — development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      inherit (nixpkgs) lib;

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      forAllSystems =
        f:
        lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ (import rust-overlay) ];
            }
          )
        );

      pinned = (lib.importTOML ./rust-toolchain.toml).toolchain;
    in
    {
      devShells = forAllSystems (
        pkgs:
        let
          rustToolchain = pkgs.rust-bin.stable.${pinned.channel}.minimal.override {
            extensions = pinned.components ++ [
              "rust-src"
              "rust-analyzer"
            ];
            targets = [ "wasm32-unknown-unknown" ];
          };

          runtimeLibs = lib.optionals pkgs.stdenv.hostPlatform.isLinux (
            with pkgs;
            [
              wayland
              libxkbcommon
              vulkan-loader
              libGL
              libx11
              libxcursor
              libxi
              libxrandr
              alsa-lib
            ]
          );
        in
        {
          default = pkgs.mkShell {
            packages =
              [
                rustToolchain
                pkgs.just
                pkgs.trunk
                pkgs.wasm-bindgen-cli_0_2_126
                pkgs.binaryen
              ]
              ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [
                pkgs.pkg-config
                pkgs.alsa-lib
              ];

            env.LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs;
          };
        }
      );

      formatter = forAllSystems (pkgs: pkgs.nixfmt-tree);
    };
}
