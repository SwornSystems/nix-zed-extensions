[![sync](https://github.com/SwornSystems/nix-zed-extensions/actions/workflows/sync.yml/badge.svg)](https://github.com/SwornSystems/nix-zed-extensions/actions/workflows/sync.yml)
[![ci](https://github.com/SwornSystems/nix-zed-extensions/actions/workflows/ci.yml/badge.svg)](https://github.com/SwornSystems/nix-zed-extensions/actions/workflows/ci.yml)

# `nix-zed-extensions`

Nix expressions for Zed extensions.

- Over 800 [extensions generated](generated/extensions).
- Daily sync from the [Zed extensions registry](https://github.com/zed-industries/extensions).

## Usage

```nix
{
  inputs = {
    zed-extensions = {
      url = "github:SwornSystems/nix-zed-extensions";
    };
  };
}
```

### Overlay

This registers every extension under `pkgs.zed-extensions`, and every grammar under `pkgs.zed-grammars`.

```nix
nixpkgs.overlays = [
  zed-extensions.overlays.default
];
```

Each extension takes the name `<extension_id>`.

```bash
> nix eval --json .#zed-extensions
{
  "0x96f": "/nix/store/hzmxbivy2hbv4456x92v2wfcfqrz4ylq-zed-extension-0x96f-1.3.1",
  "actionscript": "/nix/store/ld75wsldpf34m4v6n1l6dzbjkv65jc7j-zed-extension-actionscript-0.0.1",
  "activitywatch": "/nix/store/7n94l7931xn3zgpiv2ff5pabbh20lkpl-zed-extension-activitywatch-0.1.2",
  ...
}
```

Each grammar takes the name `<extension_id>_<grammar_id>`.

```bash
> nix eval --json .#zed-grammars
{
  "actionscript_actionscript": "/nix/store/skdvlxrzbgl5731xxgx6cnx3v86305fp-zed-grammar-actionscript-24919034fc78fdf9bedaac6616b6a60af20ab9b5",
  "ada_ada": "/nix/store/h7acvmsrrw0av4sk01255lxx19i99q2m-zed-grammar-ada-e8e2515465cc2d7c444498e68bdb9f1d86767f95",
  "aiken_aiken": "/nix/store/s83ydp2xkklgcfa0vqwazd7migs5xd5y-zed-grammar-aiken-229c5fa484468e0fd13f6264710a7f6cbb7436f1",
  ...
}
```

### Home Manager Module

A `home-manager` module for installing extensions.

Use it alongside your existing `zed-editor` config.

```nix
home-manager.sharedModules = [
  zed-extensions.homeManagerModules.default
];
```

```nix
{
  pkgs,
}:

{
  programs.zed-editor = {
    ...
  };

  programs.zed-editor-extensions = {
    enable = true;
    packages = with pkgs.zed-extensions; [
      nix
    ];
  };
}
```

### Building Grammars Manually

Use the `buildZedGrammar` builder function.

#### buildZedGrammar

```nix
{
  buildZedGrammar,
  fetchFromGitHub,
}:

buildZedGrammar (finalAttrs: {
  name = "nix";
  version = "b3cda619248e7dd0f216088bd152f59ce0bbe488";

  src = fetchFromGitHub {
    owner = "nix-community";
    repo = "tree-sitter-nix";
    rev = finalAttrs.version;
    hash = "sha256-Ib83CECi3hvm2GfeAJXIkapeN8rrpFQxCWWFFsIvB/Y=";
  };
})
```

```bash
> tree result
result
└── share
    └── zed
        └── grammars
            └── nix.wasm
```

### Building Extensions Manually

Use the `buildZedExtension` and `buildZedRustExtension` builder functions.

#### buildZedExtension

```nix
{
  buildZedExtension,
  fetchFromGitHub,
}:

buildZedExtension (finalAttrs: {
  name = "catppuccin-icons";
  version = "1.19.0";

  src = fetchFromGitHub {
    owner = "catppuccin";
    repo = "zed-icons";
    rev = "v${finalAttrs.version}";
    hash = "sha256-1S4I9fJyShkrBUqGaF8BijyRJfBgVh32HLn1ZoNlnsU=";
  };
})
```

```bash
> tree result
result
└── share
    └── zed
        └── extensions
            └── catppuccin-icons
                ├── extension.toml
                ├── icons
                │   └── ...
                └── icon_themes
                    └── catppuccin-icons.json
```

#### buildZedRustExtension

```nix
{
  buildZedRustExtension,
  fetchFromGitHub,
  zed-nix-grammar,
}:

buildZedRustExtension (finalAttrs: {
  name = "nix";
  version = "0.1.1";

  src = fetchFromGitHub {
    owner = "zed-extensions";
    repo = "nix";
    rev = "v${finalAttrs.version}";
    hash = "sha256-2+Joy2kYqDK33E51pfUSYlLgWLFLLDrBlwJkPWyPUoo=";
  };

  cargoHash = "sha256-F+qW+5SIiZNxdMSmtiwKj9j73Sd9uy5HZXGptcd3vSY=";

  grammars = [
    zed-nix-grammar
  ];
})
```

```bash
> tree result
result
└── share
    └── zed
        └── extensions
            └── nix
                ├── extension.toml
                ├── extension.wasm
                ├── grammars
                │   └── nix.wasm -> /nix/store/...
                └── languages
                    └── nix
                        ├── brackets.scm
                        ├── config.toml
                        ├── highlights.scm
                        ├── indents.scm
                        ├── injections.scm
                        └── outline.scm
```

## License

Licensed under the terms of the [GNU General Public License (Version 3.0)](LICENSE).

This project has a re-implementation of [Zed's extension builder](https://github.com/zed-industries/zed/tree/main/crates/extension), which carries the same license.
