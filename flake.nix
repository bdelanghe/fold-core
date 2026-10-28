{
  description = "fold-core: the shared fold core, byte-identical to the fold viewer it was ported from";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-darwin" "x86_64-darwin" "aarch64-linux" "x86_64-linux" ];
      each = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = each (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "fold-core";
          version = "0.1.0";
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
          # cargo test runs tests/parity.rs over goldens/: every case at byte parity with fold.ts.
          doCheck = true;
        };
      });

      checks = each (pkgs: {
        test = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
        clippy = self.packages.${pkgs.stdenv.hostPlatform.system}.default.overrideAttrs (old: {
          pname = "fold-core-clippy";
          nativeBuildInputs = old.nativeBuildInputs ++ [ pkgs.clippy ];
          buildPhase = "cargo clippy --offline --all-targets -- -D warnings";
          doCheck = false;
          installPhase = "touch $out";
        });
      });

      devShells = each (pkgs: {
        default = pkgs.mkShell { packages = [ pkgs.rustc pkgs.cargo pkgs.clippy pkgs.rustfmt pkgs.bun ]; };
      });
    };
}
