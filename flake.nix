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
          dbus-python
          pillow
          pyqt5
          pyqt5-sip
          sip
        ]);
      in
      {
        devShells.default = with pkgs; mkShell {
          nativeBuildInputs = [
          ];

          packages = [
            pythonEnv

            libsForQt5.qt5.wrapQtAppsHook
            libsForQt5.qt5.qtbase
            libsForQt5.qt5.qtwayland
            libsForQt5.qt5.qtdeclarative
            libsForQt5.qt5.qtquickcontrols
            libsForQt5.qt5.qtquickcontrols2
          ];
          
          shellHook = ''
            setQtEnvironment=$(mktemp --suffix .setQtEnvironment)
            makeWrapper "/bin/sh" "$setQtEnvironment" "''${qtWrapperArgs[@]}"
            export QT_PLUGIN_PATH=$("$setQtEnvironment" -c "echo $QT_PLUGIN_PATH")
            export QML2_IMPORT_PATH=$("$setQtEnvironment" -c "echo $QML2_IMPORT_PATH")

            export PYTHONPATH=${pythonEnv}/${pythonEnv.sitePackages}
            python -V
            echo "nix devshell rebuild success! You can use new dependencies now."
          '';
        };
      }
    );
}
