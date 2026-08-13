{
  description = "exos: effects executed over streams";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    rust-overlay = {
      inputs.nixpkgs.follows = "nixpkgs";
      url = "github:oxalica/rust-overlay";
    };
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      ...
    }:
    let
      systems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-darwin"
        "x86_64-linux"
      ];

      forEachSystem =
        make:
        nixpkgs.lib.genAttrs systems (
          system:
          make (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );

      # The oldest toolchain the workspace claims to build on, taken from the
      # claim itself so the two cannot drift. Cargo lets rust-version omit the
      # patch; rust-overlay wants all three.
      msrv =
        let
          declared = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.rust-version;
        in
        if builtins.length (builtins.splitVersion declared) < 3 then "${declared}.0" else declared;
    in
    {
      devShells = forEachSystem (pkgs: {
        default = pkgs.mkShell {
          packages = [
            # One toolchain for everything, pinned by flake.lock rather than by
            # whatever rustup last installed. rust-analyzer comes from here too,
            # so an editor cannot end up a different version from the compiler.
            (pkgs.rust-bin.stable.latest.default.override {
              extensions = [
                "rust-analyzer"
                "rust-src"
              ];
            })

            # For the client runtime's tests, and for nothing else. Applications
            # built with exos need no node, which is what `asset!` is for; the
            # runtime is 1500 lines of JavaScript and testing it needs a DOM.
            pkgs.nodejs
          ];
        };

        # What the MSRV check builds with. Deliberately not the shell anyone
        # develops in: the point is to compile the workspace with the oldest
        # compiler it promises to support, and nothing else.
        msrv = pkgs.mkShell {
          packages = [ pkgs.rust-bin.stable.${msrv}.minimal ];
        };
      });

      formatter = forEachSystem (pkgs: pkgs.nixfmt-tree);
    };
}
