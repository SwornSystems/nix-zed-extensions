{
  description = "nix-zed-extensions";

  inputs = {
    nixpkgs = {
      url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";

      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
  };

  # nix flake show
  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      ...
    }:

    let
      perSystem = nixpkgs.lib.genAttrs nixpkgs.lib.systems.flakeExposed;

      systemPkgs = perSystem (
        system:

        import nixpkgs {
          inherit system;

          overlays = [
            self.overlays.default
            (final: _prev: {
              vale-styles = final.symlinkJoin {
                name = "vale-styles";
                paths = with final.valeStyles; [
                  proselint
                  write-good
                  redhat
                ];
              };
            })
          ];
        }
      );

      perSystemPkgs = f: perSystem (system: f (systemPkgs.${system}));
    in
    {
      overlays = {
        default = nixpkgs.lib.composeManyExtensions [
          rust-overlay.overlays.default
          (import ./overlays)
        ];
      };

      homeManagerModules = {
        default = import ./modules/home-manager;
      };

      # nix build .#<name>
      packages = perSystemPkgs (pkgs: {
        nix-zed-extensions = pkgs.nix-zed-extensions;
        wasi-sdk = pkgs.wasi-sdk;
      });

      legacyPackages = perSystemPkgs (pkgs: {
        zed-grammars = pkgs.zed-grammars;
        zed-extensions = pkgs.zed-extensions;
      });

      devShells = perSystemPkgs (pkgs: {
        # nix develop
        default = pkgs.mkShell {
          name = "nix-zed-extensions-shell";

          env = {
            # Nix
            NIX_PATH = "nixpkgs=${nixpkgs.outPath}";

            # Vale
            VALE_STYLES_PATH = "${pkgs.vale-styles}/share/vale/styles";

            # Rust
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          };

          buildInputs = with pkgs; [
            # Rust
            rustc
            cargo
            clippy
            rustfmt
            rust-analyzer
            cargo-deny
            cargo-outdated
            cargo-shear

            # WASM
            wasm-tools

            # Fetch
            fetch-cargo-vendor-util
            nix-prefetch-git

            # CLI
            tree

            # Git
            committed

            # GitHub
            pinact
            zizmor

            # Spellchecking
            typos
            typos-lsp

            # Markdown
            lychee
            vale
            vale-ls

            # TOML
            tombi

            # Nushell
            nushell
            nufmt
            nu-lint

            # Nix
            nix-update
            deadnix
            nixfmt
            nixd
            nil
          ];
        };

        # nix develop .#ci
        ci = pkgs.mkShell {
          name = "nix-zed-extensions-ci-shell";

          env = {
            # Vale
            VALE_STYLES_PATH = "${pkgs.vale-styles}/share/vale/styles";
          };

          buildInputs = with pkgs; [
            # Rust
            rustc
            cargo
            clippy
            rustfmt
            cargo-deny
            cargo-shear

            # WASM
            wasm-tools

            # CLI
            tree

            # Git
            committed

            # GitHub
            zizmor

            # Spellchecking
            typos

            # Markdown
            lychee
            vale

            # TOML
            tombi

            # Nushell
            nushell
            nufmt
            nu-lint

            # Nix
            deadnix
            nixfmt
          ];
        };
      });
    };
}
