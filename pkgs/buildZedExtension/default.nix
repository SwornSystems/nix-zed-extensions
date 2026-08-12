{
  lib,
  stdenvNoCC,
  nix-zed-extensions,
  ...
}:

lib.extendMkDerivation {
  constructDrv = stdenvNoCC.mkDerivation;

  excludeDrvArgNames = [
    "name"
    "src"
    "version"
    "extensionRoot"
    "grammars"
  ];

  extendDrvArgs =
    _finalAttrs:

    {
      name,
      src,
      version,
      extensionRoot ? null,
      grammars ? [ ],
      ...
    }:

    let
      extensionDir = if extensionRoot != null then extensionRoot else ".";
      grammarArgs = lib.concatMapStringsSep " " (grammar: "${grammar.name}:${grammar}") grammars;
    in
    {
      pname = "zed-extension-${name}";
      inherit src version;

      nativeBuildInputs = [
        nix-zed-extensions
      ];

      buildPhase = ''
        runHook preBuild

        pushd ${extensionDir}
        nix-zed-extensions populate
        popd

        runHook postBuild
      '';

      installPhase = ''
        runHook preInstall

        pushd ${extensionDir}
        nix-zed-extensions install $out ${grammarArgs}
        popd

        runHook postInstall
      '';
    };
}
