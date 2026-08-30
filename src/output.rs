use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct NixExtensions {
    pub extensions: Vec<Extension>,
    pub grammars: Vec<Grammar>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extension {
    pub name: String,
    pub version: String,
    pub src: Source,
    #[serde(rename = "extensionRoot")]
    pub root: Option<String>,
    pub grammars: Vec<String>,
    #[serde(flatten)]
    pub kind: ExtensionKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ExtensionKind {
    Plain,

    Rust {
        #[serde(rename = "cargoRoot")]
        cargo_root: Option<String>,
        #[serde(rename = "cargoHash")]
        cargo_hash: String,
        #[serde(rename = "cargoLock", skip_serializing_if = "Option::is_none")]
        cargo_lock: Option<CargoLock>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoLock {
    #[serde(rename = "lockFile")]
    pub lock_file: PathBuf,
    #[serde(rename = "outputHashes", default)]
    pub output_hashes: BTreeMap<String, String>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub url: String,
    pub rev: String,
    pub date: String,
    pub path: String,
    pub sha256: String,
    pub hash: String,
    #[serde(rename = "fetchLFS")]
    pub fetch_lfs: bool,
    #[serde(rename = "fetchSubmodules")]
    pub fetch_submodules: bool,
    #[serde(rename = "deepClone")]
    pub deep_clone: bool,
    #[serde(rename = "leaveDotGit")]
    pub leave_dot_git: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grammar {
    pub id: String,
    pub name: String,
    pub version: String,
    pub src: Source,
    #[serde(rename = "grammarRoot")]
    pub root: Option<String>,
}
