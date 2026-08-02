#!/usr/bin/env nix
#!nix develop .#ci --command nu

# Build test extensions.
def main [
    nixpkgs: string = "github:NixOS/nixpkgs/nixos-26.05", # nixpkgs version to use.
]: nothing -> nothing {
    nix build ".#nix-zed-extensions" --quiet --inputs-from . --override-input nixpkgs $nixpkgs

    let extensions = [
        {
            name: catppuccin
            files: [extension.toml]
            directories: [themes]
        }
        {
            name: nix
            files: [extension.toml extension.wasm]
            directories: [grammars languages]
        }
        {
            name: aura-theme
            files: [extension.toml]
            directories: [themes]
        }
        {
            name: html
            files: [extension.toml extension.wasm]
            directories: [grammars languages]
        }
        {
            name: deputy
            files: [extension.toml extension.wasm]
            directories: []
        }
    ]

    for extension in $extensions {
        let pkg: string = $".#zed-extensions.($extension.name)"
        nix build $pkg --quiet --inputs-from . --override-input nixpkgs $nixpkgs

        let root: string = $"result/share/zed/extensions/($extension.name)"
        tree $root

        for file in $extension.files {
            if ($"($root)/($file)" | path expand | path type) != file {
                print --stderr $"Expected a file: ($root)/($file)"
                exit 1
            }
        }

        for directory in $extension.directories {
            if ($"($root)/($directory)" | path expand | path type) != dir {
                print --stderr $"Expected a directory: ($root)/($directory)"
                exit 1
            }
        }
    }
}
