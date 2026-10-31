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
        # the fold viewer as a static app (viewer/, COGNITIVE-BROWSER-SDK-2026-10-30 step 1): index.html, its modules, the module and
        # its loader (js/fold-core.ts as browser JS), its stylesheet (fold.css, which every pre-rendered page links too), and the one face. It holds no data: it fetches a document (its fold-data meta,
        # /fold-data/fleet/page.json) and folds it in the browser. Served as is, e.g. at /fold/ by tailscale serve.
        viewer = pkgs.runCommand "fold-viewer" { nativeBuildInputs = [ pkgs.bun pkgs.woff2 ]; } ''
          mkdir -p $out/fonts
          cp ${self}/viewer/index.html ${self}/viewer/fold.css ${self}/viewer/view.js ${self}/viewer/fold-dom.js ${wasm}/lib/fold_core.wasm $out/
          HOME=$TMPDIR bun build ${self}/js/fold-core.ts --target browser --format esm --outfile $out/fold-core.js
          cp ${pkgs.atkinson-hyperlegible-next}/share/fonts/opentype/AtkinsonHyperlegibleNext-Regular.otf face.otf
          woff2_compress face.otf && cp face.woff2 $out/fonts/atkinson-hyperlegible-next.woff2
          # the Home Screen app (brief items 10 and 22): its manifest (no icon until the icon panel picks one) and the service worker,
          # whose shell cache is named by the digest of every other file here, so each build replaces the last one's copy
          cp ${self}/viewer/manifest.webmanifest $out/
          v=$(cd $out && find . -type f | LC_ALL=C sort | xargs sha256sum | sha256sum | cut -c1-16)
          substitute ${self}/viewer/sw.js $out/sw.js --replace-fail @VERSION@ "$v"
        '';
        # the JS tests' dependencies (package.json, bun.lock: lone and linkedom), fetched once by their lockfile so checks run offline
        node-modules = pkgs.stdenvNoCC.mkDerivation {
          pname = "fold-core-node-modules";
          version = "0";
          src = pkgs.lib.fileset.toSource { root = ./.; fileset = pkgs.lib.fileset.unions [ ./package.json ./bun.lock ./.npmrc ]; };
          nativeBuildInputs = [ pkgs.bun pkgs.cacert ];
          buildPhase = "HOME=$TMPDIR bun install --frozen-lockfile --ignore-scripts --no-progress";
          installPhase = "mkdir -p $out && cp -r node_modules $out/";
          dontFixup = true;
          outputHashMode = "recursive";
          outputHashAlgo = "sha256";
          outputHash = "sha256-TMmFhYWDkfai4T0nk2WXpD3E0B3cbdgRQgxNK2FJ6IE=";
        };
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
        # every golden fold's pre-rendered page (goldens/pages, src/page.rs) in headless Chromium with JavaScript off, light and dark:
        # its title, focus on its h1 when entered by a link, same-tab links that land, and lone's colour contrast over the colours
        # Chromium computes from fold.css (viewer/pages-check.ts; known findings in goldens/pages/known.json)
        pages-browser = pkgs.runCommand "fold-core-pages-browser" { nativeBuildInputs = [ pkgs.bun pkgs.chromium ]; } ''
          cp -r ${self}/goldens ${self}/viewer ${self}/package.json .
          ln -s ${self.packages.${pkgs.stdenv.hostPlatform.system}.node-modules}/node_modules node_modules
          export HOME=$TMPDIR FONTCONFIG_FILE=${pkgs.makeFontsConf { fontDirectories = [ ]; }}
          VIEWER=${self.packages.${pkgs.stdenv.hostPlatform.system}.viewer} CHROMIUM=${pkgs.chromium}/bin/chromium bun viewer/pages-check.ts
          touch $out
        '';
      } // {
        test = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
        # the wasm module at byte parity on every golden (js/parity.test.ts)
        wasm-parity = pkgs.runCommand "fold-core-wasm-parity" { nativeBuildInputs = [ pkgs.bun ]; } ''
          cp -r ${self}/js ${self}/goldens .
          HOME=$TMPDIR FOLD_CORE_WASM=${self.packages.${pkgs.stdenv.hostPlatform.system}.wasm}/lib/fold_core.wasm bun test ./js/parity.test.ts
          touch $out
        '';
        # one fold's HTML (src/html.rs, goldens/html) held against fold-dom.js toDom through a DOM, and blessed by lone with no
        # finding beyond goldens/html/lone.known.json (js/html.test.ts); native and WASM writing the same bytes is tests/html.rs and wasm-parity
        html-lone = pkgs.runCommand "fold-core-html-lone" { nativeBuildInputs = [ pkgs.bun ]; } ''
          cp -r ${self}/js ${self}/goldens ${self}/viewer ${self}/package.json .
          ln -s ${self.packages.${pkgs.stdenv.hostPlatform.system}.node-modules}/node_modules node_modules
          HOME=$TMPDIR bun test ./js/html.test.ts
          touch $out
        '';
        # contrast proven from tokens (brief item 17): Baobab 0.2.2's own TS over viewer/theme.tokens.json agrees with the Rust port's
        # goldens/contrast.json bit for bit (js/contrast.test.ts); the proof itself and fold.css's colours are tests/contrast.rs
        contrast-baobab = pkgs.runCommand "fold-core-contrast-baobab" { nativeBuildInputs = [ pkgs.bun ]; } ''
          cp -r ${self}/js ${self}/goldens ${self}/viewer ${self}/package.json .
          ln -s ${self.packages.${pkgs.stdenv.hostPlatform.system}.node-modules}/node_modules node_modules
          HOME=$TMPDIR bun test ./js/contrast.test.ts
          touch $out
        '';
        # the viewer's own path on every golden (viewer/viewer.test.ts): fetched and folded as in the browser, every location's tree,
        # every fold's DOM without a refusal (the AX gate), and the walk over what it shows (the load gate)
        viewer = pkgs.runCommand "fold-core-viewer" { nativeBuildInputs = [ pkgs.bun ]; } ''
          cp -r ${self}/js ${self}/goldens ${self}/viewer .
          HOME=$TMPDIR FOLD_CORE_WASM=${self.packages.${pkgs.stdenv.hostPlatform.system}.wasm}/lib/fold_core.wasm bun test ./viewer/viewer.test.ts
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
