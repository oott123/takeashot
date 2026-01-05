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
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            pythonEnv
            
            # Qt native deps
            libsForQt5.qt5.qtbase
            libsForQt5.qt5.qtwayland
          ];
          
          shellHook = ''
            # Ensure the python environment is on the path
            export PYTHONPATH=${pythonEnv}/${pythonEnv.sitePackages}
          '';
        };
      }
    );
}
