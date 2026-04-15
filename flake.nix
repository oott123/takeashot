{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        depLibs = with pkgs; [
          libxkbcommon
          wayland
          libGL
          vulkan-loader
        ];
      in
       {
         devShells.default = with pkgs; mkShell {
           nativeBuildInputs = [ pkg-config clang ];
           buildInputs = depLibs;
           shellHook = ''
             export NIX_LD_LIBRARY_PATH="$NIX_LD_LIBRARY_PATH:${lib.makeLibraryPath depLibs}"
             export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:$NIX_LD_LIBRARY_PATH"
           '';
         };
       }
    );
}
