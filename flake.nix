{
  description = "fold-core: the shared fold core, byte-identical to the fold viewer it was ported from";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-darwin" "x86_64-darwin" "aarch64-linux" "x86_64-linux" ];
      each = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = each (pkgs: rec {
        # the core as WebAssembly with a plain C ABI (src/wasm.rs), loaded by js/fold-core.ts
        wasm = default.overrideAttrs (old: {
          pname = "fold-core-wasm";
          nativeBuildInputs = old.nativeBuildInputs ++ [ pkgs.lld ];
          buildPhase = "cargo build --offline --release --lib --target wasm32-unknown-unknown";
          doCheck = false;
          installPhase = "mkdir -p $out/lib && cp target/wasm32-unknown-unknown/release/fold_core.wasm $out/lib/ && cp js/fold-core.ts $out/lib/";
          dontFixup = true;
        });
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
        # the wasm module at byte parity on every golden (js/parity.test.ts)
        wasm-parity = pkgs.runCommand "fold-core-wasm-parity" { nativeBuildInputs = [ pkgs.bun ]; } ''
          cp -r ${self}/js ${self}/goldens .
          HOME=$TMPDIR FOLD_CORE_WASM=${self.packages.${pkgs.stdenv.hostPlatform.system}.wasm}/lib/fold_core.wasm bun test ./js/parity.test.ts
          touch $out
        '';
        clippy = self.packages.${pkgs.stdenv.hostPlatform.system}.default.overrideAttrs (old: {
          pname = "fold-core-clippy";
          nativeBuildInputs = old.nativeBuildInputs ++ [ pkgs.clippy ];
          buildPhase = "cargo clippy --offline --all-targets -- -D warnings";
          doCheck = false;
          installPhase = "touch $out";
        });
      });

      devShells = each (pkgs: {
        default = pkgs.mkShell { packages = [ pkgs.rustc pkgs.cargo pkgs.clippy pkgs.rustfmt pkgs.bun pkgs.lld ]; };
      });
    };
}
