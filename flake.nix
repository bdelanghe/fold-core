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
        # the core as WebAssembly with a plain C ABI (src/wasm.rs), loaded by js/fold-core.ts. No tzdb: the host gives the zone rules
        # (src/tz.rs), which keeps the module under its size budget (checks.wasm-size); wasm-opt -Oz after cargo.
        wasm = default.overrideAttrs (old: {
          pname = "fold-core-wasm";
          nativeBuildInputs = old.nativeBuildInputs ++ [ pkgs.lld pkgs.binaryen ];
          # the build dir is random per build (/nix/var/nix/builds/nix-<pid>-<n>) and panic paths name it: remapped, the bytes are the same
          # on every build, so a consumer can pin their digest (home checks.fold-core-parity)
          buildPhase = "RUSTFLAGS=\"--remap-path-prefix=$NIX_BUILD_TOP=/build\" cargo build --offline --release --lib --no-default-features --target wasm32-unknown-unknown && wasm-opt -Oz --enable-bulk-memory --enable-sign-ext --enable-nontrapping-float-to-int --enable-mutable-globals target/wasm32-unknown-unknown/release/fold_core.wasm -o fold_core.wasm";
          doCheck = false;
          installPhase = "mkdir -p $out/lib && cp fold_core.wasm js/fold-core.ts $out/lib/";
          dontFixup = true;
        });
        # the core as a native static library with a C ABI (src/native.rs, include/fold_core.h) for a host that links it in-process
        # (Allie's native tree). No tzdb: the host defines fold_core_tz_offset (src/tz.rs), so the zone data (~1.9 MB) stays out.
        native = default.overrideAttrs (old: {
          pname = "fold-core-native";
          buildPhase = "RUSTFLAGS=\"--remap-path-prefix=$NIX_BUILD_TOP=/build\" cargo rustc --offline --release --lib --no-default-features --crate-type staticlib --target ${pkgs.stdenv.hostPlatform.rust.rustcTarget}";
          doCheck = false;
          installPhase = "mkdir -p $out/lib $out/include && cp target/${pkgs.stdenv.hostPlatform.rust.rustcTarget}/release/libfold_core.a $out/lib/ && cp include/fold_core.h $out/include/";
          dontFixup = true;
        });
        # the fold viewer as a static app (viewer/, COGNITIVE-BROWSER-SDK-2026-10-30 step 1): index.html, its modules, the module and
        # its loader (js/fold-core.ts as browser JS), and the one face. It holds no data: it fetches a document (its fold-data meta,
        # /fold-data/fleet/page.json) and folds it in the browser. Served as is, e.g. at /fold/ by tailscale serve.
        viewer = pkgs.runCommand "fold-viewer" { nativeBuildInputs = [ pkgs.bun pkgs.woff2 ]; } ''
          mkdir -p $out/fonts
          cp ${self}/viewer/index.html ${self}/viewer/view.js ${self}/viewer/fold-dom.js ${wasm}/lib/fold_core.wasm $out/
          HOME=$TMPDIR bun build ${self}/js/fold-core.ts --target browser --format esm --outfile $out/fold-core.js
          cp ${pkgs.atkinson-hyperlegible-next}/share/fonts/opentype/AtkinsonHyperlegibleNext-Regular.otf face.otf
          woff2_compress face.otf && cp face.woff2 $out/fonts/atkinson-hyperlegible-next.woff2
        '';
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "fold-core";
          version = "0.1.0";
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
          # cargo test runs tests/parity.rs over goldens/: every case at byte parity with fold.ts.
          doCheck = true;
        };
      });

      checks = each (pkgs: pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
        # the built viewer in headless Chromium at the phone's and the iPad's size, over a golden page served as its data (viewer/browser-check.ts);
        # Linux only, where nixpkgs builds Chromium
        viewer-browser = pkgs.runCommand "fold-core-viewer-browser" { nativeBuildInputs = [ pkgs.bun pkgs.chromium ]; } ''
          cp -r ${self}/js ${self}/goldens ${self}/viewer .
          export HOME=$TMPDIR FONTCONFIG_FILE=${pkgs.makeFontsConf { fontDirectories = [ ]; }}
          VIEWER=${self.packages.${pkgs.stdenv.hostPlatform.system}.viewer} CHROMIUM=${pkgs.chromium}/bin/chromium bun viewer/browser-check.ts sample
          touch $out
        '';
      } // {
        test = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
        # the wasm module at byte parity on every golden (js/parity.test.ts), and its rotor and yank on Allie's (js/rotor.test.ts)
        wasm-parity = pkgs.runCommand "fold-core-wasm-parity" { nativeBuildInputs = [ pkgs.bun ]; } ''
          cp -r ${self}/js ${self}/goldens .
          HOME=$TMPDIR FOLD_CORE_WASM=${self.packages.${pkgs.stdenv.hostPlatform.system}.wasm}/lib/fold_core.wasm bun test ./js/parity.test.ts ./js/rotor.test.ts
          touch $out
        '';
        # the viewer's own path on every golden (viewer/viewer.test.ts): fetched and folded as in the browser, every location's tree,
        # every fold's DOM without a refusal (the AX gate), and the walk over what it shows (the load gate)
        viewer = pkgs.runCommand "fold-core-viewer" { nativeBuildInputs = [ pkgs.bun ]; } ''
          cp -r ${self}/js ${self}/goldens ${self}/viewer .
          HOME=$TMPDIR FOLD_CORE_WASM=${self.packages.${pkgs.stdenv.hostPlatform.system}.wasm}/lib/fold_core.wasm bun test ./viewer/viewer.test.ts
          touch $out
        '';
        # the native library through its C ABI on every golden (js/parity.test.ts with FOLD_CORE_NATIVE): native/roundtrip.c links it as a
        # host does, zone rules from tzdata, and the error path from C. Its size is printed for the record (no tzdb).
        native = pkgs.runCommand "fold-core-native-parity" { nativeBuildInputs = [ pkgs.bun pkgs.stdenv.cc ]; } ''
          cp -r ${self}/js ${self}/goldens ${self}/native .
          n=${self.packages.${pkgs.stdenv.hostPlatform.system}.native}
          echo "libfold_core.a: $(wc -c < $n/lib/libfold_core.a) bytes"
          cc -O2 -I$n/include native/roundtrip.c $n/lib/libfold_core.a ${pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux "-lpthread -ldl -lm"} -o roundtrip
          echo "roundtrip (C host + library): $(wc -c < roundtrip) bytes"
          HOME=$TMPDIR TZDIR=${pkgs.tzdata}/share/zoneinfo FOLD_CORE_NATIVE=$PWD/roundtrip bun test ./js/parity.test.ts
          touch $out
        '';
        # the C header is what cbindgen makes of src/native.rs today: a changed signature fails here until include/fold_core.h is regenerated
        native-header = pkgs.runCommand "fold-core-native-header" { nativeBuildInputs = [ pkgs.rust-cbindgen ]; } ''
          cbindgen -q -c ${self}/cbindgen.toml -o fold_core.h ${self}/src/native.rs
          diff -u ${self}/include/fold_core.h fold_core.h
          touch $out
        '';
        # the size budget (scratch-main-79 10-29, for the fleet page's iPad first load): at most 200 KB gzipped. At 926ac44 it was 2,183,512 bytes, 539,305 gzipped.
        wasm-size = pkgs.runCommand "fold-core-wasm-size" { nativeBuildInputs = [ pkgs.gzip ]; } ''
          w=${self.packages.${pkgs.stdenv.hostPlatform.system}.wasm}/lib/fold_core.wasm
          raw=$(wc -c < $w); gz=$(gzip -9c $w | wc -c); budget=$((200 * 1024))
          echo "fold_core.wasm: $raw bytes, $gz gzipped (budget $budget gzipped)"
          [ "$gz" -le "$budget" ] || { echo "over the size budget by $((gz - budget)) bytes"; exit 1; }
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
