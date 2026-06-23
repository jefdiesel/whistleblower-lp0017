# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Whistleblower — Nix build for the Logos Basecamp "ui_qml" module.
#
# This is based on the logos-module-builder tutorial template:
#     nix flake init -t github:logos-co/logos-module-builder/tutorial-v1
#
# It is intentionally written to the *documented* template shape. The exact
# attribute/function names exported by logos-module-builder (e.g. the builder
# function, its argument set, and whether the `.lgx` is produced by a dedicated
# function or a passthru) vary by version. ALIGN the marked spots below with the
# installed logos-module-builder API before building on the provisioned machine.
{
  description = "Whistleblower — censorship-resistant document upload (Logos ui_qml module)";

  inputs = {
    # Follow logos-module-builder's nixpkgs to stay ABI/Qt-compatible with it.
    logos-module-builder.url = "github:logos-co/logos-module-builder/tutorial-v1";
    nixpkgs.follows = "logos-module-builder/nixpkgs";
  };

  outputs = { self, nixpkgs, logos-module-builder, ... }:
    let
      # Systems the module can be built for.
      supportedSystems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };

          # logos-module-builder's per-system library. The template exposes a
          # builder under `lib.<system>` (sometimes `packages.<system>.lib` or a
          # top-level `mkLogosModule`). Resolve defensively so this works across
          # template revisions.
          lmb =
            (logos-module-builder.lib or { }).${system}
            or (logos-module-builder.packages.${system} or { });

          # The module build. `buildLogosModule` is the tutorial-v1 entry point;
          # if the installed version names it differently (e.g. `mkModule`,
          # `buildModule`, or `mkUiModule`), rename this call accordingly.
          whistleblower = lmb.buildLogosModule {
            pname = "whistleblower";
            version = "0.1.0";

            # The module sources: this `app/` directory (CMakeLists.txt, src/,
            # qml/, metadata.json).
            src = ./.;

            # type=ui_qml is also declared in metadata.json; passed here too in
            # case the builder keys off an explicit argument.
            moduleType = "ui_qml";

            # Qt6 toolchain for the QML view + C++ backend. Some template versions
            # supply Qt automatically for ui_qml modules; if so, these become
            # redundant (harmless) or should be dropped.
            nativeBuildInputs = with pkgs; [ cmake ninja qt6.wrapQtAppsHook ];
            buildInputs = with pkgs; [
              qt6.qtbase
              qt6.qtdeclarative # Qml + Quick + Quick Controls 2 + Dialogs
            ];
          };
        in
        {
          # `nix build`  -> the built module.
          default = whistleblower;

          # `nix build .#lgx`  -> the packaged .lgx for Logos Basecamp.
          #
          # tutorial-v1 typically exposes the packaged artifact either as a
          # passthru on the module (`whistleblower.lgx`) or via a dedicated
          # function (`lmb.mkLgx { module = whistleblower; }` /
          # `lmb.packageLgx`). Try the passthru first, then fall back to a
          # packaging function. ALIGN to the installed API.
          lgx =
            whistleblower.lgx
              or (lmb.mkLgx { module = whistleblower; })
              or (lmb.packageLgx { module = whistleblower; });
        });

      # `nix develop` — a shell with the toolchain to iterate locally.
      devShells = forAllSystems (system:
        let pkgs = import nixpkgs { inherit system; };
        in {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cmake
              ninja
              qt6.qtbase
              qt6.qtdeclarative
            ];
          };
        });
    };
}
