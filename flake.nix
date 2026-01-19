{
  description = "PyQt development environment";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        
        pythonEnv = pkgs.python3.withPackages (ps: with ps; [
          pip
          virtualenv
          # dbus-python # Removed in favor of PyQt6.QtDBus
          pillow
          evdev
          pyqt6
          pyqt6-sip
          # sip # Often bundled or not needed explicitly if pyqt6 pulls it
          pytest
          pytest-qt
        ]);
      in
      {
        devShells.default = with pkgs; mkShell {
          nativeBuildInputs = [
          ];

          packages = [
            pythonEnv

            qt6.qtbase
            qt6.qtwayland
            qt6.qtdeclarative
            qt6.qtquick3d # Maybe needed for some qml? sticking to basics first
            # qt6.qtquickcontrols2 # Often included in qtdeclarative or qtbase in newer nix versions, but checking...
            # In qt6, qtquickcontrols2 is usually part of qtdeclarative
          ];
          
          shellHook = ''
            # a workaround for setting QML and Qt Plugins path correctly
            setQtEnvironment=$(mktemp --suffix .setQtEnvironment)
            makeWrapper "/bin/sh" "$setQtEnvironment" "''${qtWrapperArgs[@]}"
            export QT_PLUGIN_PATH="$("$setQtEnvironment" -c 'printenv QT_PLUGIN_PATH')"
            export QML2_IMPORT_PATH="$("$setQtEnvironment" -c 'printenv NIXPKGS_QT5_QML_IMPORT_PATH')"
            # end of the workaround, don't touch unless you want to debug for hours

            export PYTHONPATH=${pythonEnv}/${pythonEnv.sitePackages}
            python -V
            echo "nix devshell rebuild success! You can use new dependencies now."
          '';
        };
      }
    );
}
