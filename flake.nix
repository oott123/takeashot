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
         packages.default = with pkgs; stdenv.mkDerivation {
           name = "takeashot";
           src = ./.;

           nativeBuildInputs = [
             makeWrapper
             qt6.wrapQtAppsHook
           ];

           buildInputs = [
             pythonEnv
             qt6.qtbase
             qt6.qtwayland
             qt6.qtdeclarative
             qt6.qtquick3d
           ];

           installPhase = ''
             mkdir -p $out/bin $out/lib/takeashot $out/share/applications
             cp *.py $out/lib/takeashot/
             cp -r annotations $out/lib/takeashot/
             cp Toolbar.qml $out/lib/takeashot/

             cat > $out/bin/takeashot <<EOF
             #!/bin/sh
             export PYTHONPATH=$out/lib/takeashot:\$PYTHONPATH
             exec ${pythonEnv}/bin/python $out/lib/takeashot/main.py "\$@"
             EOF
             chmod +x $out/bin/takeashot

             cat > $out/share/applications/takeashot.desktop <<EOF
             [Desktop Entry]
             Name=Take a Shot
             Exec=$out/bin/takeashot
             Icon=accessories-screenshot
             Type=Application
             Categories=Utility
             EOF

             cat > $out/share/applications/takeashot-service.desktop <<EOF
             [Desktop Entry]
             Name=Take a Shot Python (required for KDE permissions)
             Exec=${pythonEnv}/bin/python
             Icon=accessories-screenshot
             Type=Application
             Categories=Utility
             Hidden=true
             X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2
             EOF
           '';

           postFixup = ''
             wrapQtApp $out/bin/takeashot
           '';
         };

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
