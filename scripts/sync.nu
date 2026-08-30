#!/usr/bin/env nix
#!nix develop .#ci --command nu

# Sync the latest generated extensions.
def main []: nothing -> nothing {
    nix-zed-extensions sync
}
