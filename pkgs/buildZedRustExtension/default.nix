{
  lib,
  makeRustPlatform,
  rust-bin,
  nix-zed-extensions,
  ...
}:

let
  rust = rust-bin.stable.latest.default.override {
    targets = [ "wasm32-wasip2" ];
  };

  rustPlatform = makeRustPlatform {
    cargo = rust;
    rustc = rust;
  };
in
lib.extendMkDerivation {
  constructDrv = rustPlatform.buildRustPackage;

  excludeDrvArgNames = [
    "name"
    "src"
    "version"
    "extensionRoot"
    "cargoRoot"
    "grammars"
  ];

  extendDrvArgs =
    _finalAttrs:

    {
      name,
      src,
      version,
      extensionRoot ? null,
      cargoRoot ? null,
      grammars ? [ ],
      cargoBuildFlags ? [ ],
      nativeBuildInputs ? [ ],
      ...
    }:

    let
      extensionDir = if extensionRoot == null then (if cargoRoot == null then "." else cargoRoot) else extensionRoot;
      grammarArgs = lib.concatMapStringsSep " " (grammar: "${grammar.name}:${grammar}") grammars;
    in
    {
      pname = "zed-extension-${name}";
      inherit
        src
        version
        cargoRoot
        ;

      nativeBuildInputs = [
        nix-zed-extensions
      ]
      ++ nativeBuildInputs;

      buildPhase = ''
        pushd ${extensionDir}
        cargo build --release --target wasm32-wasip2 --target-dir target ${lib.concatStringsSep " " cargoBuildFlags}
        mv target/wasm32-wasip2/release/*.wasm extension.wasm
        nix-zed-extensions populate
        popd
      '';

      doCheck = false;

      installPhase = ''
        pushd ${extensionDir}
        nix-zed-extensions install $out ${grammarArgs}
        popd
      '';
    };
}
