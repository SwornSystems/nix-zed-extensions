{
  lib,
  pkgsCross,
}:

{
  name,
  src,
  version,
  grammarRoot ? null,
}:

pkgsCross.wasi32.tree-sitter.buildGrammar (
  {
    inherit src version;
    language = name;

    installPhase = ''
      install -Dm644 parser.wasm $out/share/zed/grammars/${name}.wasm
    '';
  }
  // lib.optionalAttrs (grammarRoot != null) {
    location = grammarRoot;
  }
)
