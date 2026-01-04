{
  description = "PyQt FHS environment with .venv support";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        devShells.default = (pkgs.buildFHSEnv {
          name = "pyqt-fhs";
          targetPkgs = pkgs: with pkgs; [
            uv
            
            # Python
            python3
            python3Packages.pip
            python3Packages.virtualenv
            python3Packages.dbus-python
            
            # Qt basics
            libsForQt5.qt5.qtbase
            libsForQt5.qt5.qtwayland
            
            # Common libraries for python packages / Qt
            glib
            libGL
            libxkbcommon
            fontconfig
            freetype
            dbus
            zlib
            gcc
            gnumake
            
            # X11 libs (often needed even on Wayland for XWayland or callbacks)
            xorg.libX11
            xorg.libXi
            xorg.libXext
            xorg.libXrender
            xorg.libXcursor
            xorg.libXrandr
            xorg.libXinerama
            xorg.libXfixes
          ];
          
          # Script to run when entering the FHS environment
          runScript = pkgs.writeScript "init-fhs.sh" ''
            # Check if .venv exists, if not create it
            if [ ! -d ".venv" ]; then
              echo "Creating virtual environment (.venv)..."
              python -m venv .venv --system-site-packages
            fi
            
            # Prepare the activation command
            # We use a custom bashrc to source the activate script and then the user's bashrc
            
            echo "Activating .venv..."
            source .venv/bin/activate
            
            # Launch bash with the venv activated
            # We inherit the environment variables from the source above
            echo "You are now in dev shell!"
            exec bash
          '';
        }).env;
      }
    );
}
