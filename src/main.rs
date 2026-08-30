use std::{
    collections::{BTreeMap, HashSet},
    env::temp_dir,
    num::NonZero,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use futures_util::stream::FuturesUnordered;
use registry::RegistryExtension;
use smol::fs::unix;
use smol::{fs, lock::Semaphore, stream::StreamExt};
use smol_macros::main;
use tracing::Instrument;

use crate::{
    manifest::ExtensionManifest, output::NixExtensions, registry::RegistryEntry,
    sync::process_extension, wasm::extract_zed_api_version,
};

pub mod copy;
pub mod manifest;
pub mod output;
pub mod registry;
pub mod sync;
pub mod wasm;

main! {
    async fn main() -> anyhow::Result<()> {
        run().await
    }
}

async fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .compact()
        .without_time()
        .with_target(false)
        .init();

    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("sync") => {
            let mut output = NixExtensions::default();

            // Load existing extensions
            let extensions_dir = Path::new("generated/extensions");
            if extensions_dir.exists() {
                tracing::info!("Loading existing extensions");

                let mut entries = fs::read_dir(extensions_dir).await?;
                while let Some(entry) = entries.try_next().await? {
                    let path = entry.path();
                    if path
                        .extension()
                        .is_some_and(|extension| extension == "json")
                    {
                        let content = fs::read_to_string(&path).await?;
                        if let Ok(extension) = serde_json::from_str(&content) {
                            output.extensions.push(extension);
                        }
                    }
                }
            }

            // Load existing grammars
            let grammars_dir = Path::new("generated/grammars");
            if grammars_dir.exists() {
                tracing::info!("Loading existing grammars");

                let mut entries = fs::read_dir(grammars_dir).await?;
                while let Some(entry) = entries.try_next().await? {
                    let path = entry.path();
                    if path
                        .extension()
                        .is_some_and(|extension| extension == "json")
                    {
                        let content = fs::read_to_string(&path).await?;
                        if let Ok(grammar) = serde_json::from_str(&content) {
                            output.grammars.push(grammar);
                        }
                    }
                }
            }

            tracing::info!("Cloning extensions registry");

            let tmp_registry = temp_dir().join("registry");
            if tmp_registry.exists() {
                fs::remove_dir_all(&tmp_registry).await?;
            }

            let clone = Command::new("git")
                .args([
                    "clone",
                    "--depth",
                    "1",
                    "https://github.com/zed-industries/extensions",
                    &tmp_registry.to_string_lossy(),
                ])
                .status()?;

            if !clone.success() {
                anyhow::bail!("Failed to clone extensions repository");
            }

            // Lookup registry extensions
            let registry = tmp_registry.join("extensions.toml");
            let registry = fs::read_to_string(registry).await?;
            let registry: BTreeMap<String, RegistryEntry> = toml::from_str(&registry)?;

            // Parse submodule revisions
            let submodules = Command::new("git")
                .current_dir(&tmp_registry)
                .args(["submodule", "status"])
                .output()?;

            if !submodules.status.success() {
                anyhow::bail!("Failed to get submodule status");
            }

            let submodules = String::from_utf8(submodules.stdout)?.trim().to_owned();

            let mut revisions: BTreeMap<String, String> = BTreeMap::new();
            for line in submodules.lines() {
                let parts: Vec<&str> = line.splitn(2, ' ').collect();

                let revision = parts[0].trim_start_matches('-').to_owned();
                let path = parts[1].to_owned();

                revisions.insert(path, revision);
            }

            // Parse submodule repositories
            let gitmodules = Command::new("git")
                .current_dir(&tmp_registry)
                .args(["config", "--file", ".gitmodules", "--list"])
                .output()?;

            if !gitmodules.status.success() {
                anyhow::bail!("Failed to get submodule repositories");
            }

            let gitmodules = String::from_utf8(gitmodules.stdout)?.trim().to_owned();

            let mut repositories: BTreeMap<String, String> = BTreeMap::new();
            for line in gitmodules.lines() {
                let parts: Vec<&str> = line.splitn(2, '=').collect();

                let path = parts[0]
                    .trim_start_matches("submodule.")
                    .trim_end_matches(".url")
                    .to_owned();

                let repository = parts[1].trim_end_matches(".git").to_owned();
                repositories.insert(path, repository);
            }

            // Merge details
            let mut extensions: Vec<RegistryExtension> = vec![];
            for (name, entry) in &registry {
                let Some(repository) = repositories.get(&entry.submodule) else {
                    tracing::warn!(
                        submodule = ?entry.submodule,
                        "Missing submodule repository"
                    );

                    continue;
                };

                let Some(revision) = revisions.get(&entry.submodule) else {
                    tracing::warn!(
                        submodule = ?entry.submodule,
                        "Missing submodule revision"
                    );

                    continue;
                };

                extensions.push(RegistryExtension {
                    name: name.clone(),
                    version: entry.version.clone(),
                    repository: repository.clone(),
                    path: entry.path.clone(),
                    rev: revision.clone(),
                });
            }

            let extension_names: HashSet<String> = registry
                .iter()
                .map(|extension| extension.0.clone())
                .collect();

            // Handle removed extensions/grammars
            let removed_extensions: Vec<String> = output
                .extensions
                .iter()
                .filter(|existing| !extension_names.contains(&existing.name))
                .map(|ext| ext.name.clone())
                .collect();

            for name in &removed_extensions {
                tracing::info!(
                    name = name,
                    "Removing extension that is no longer maintained"
                );

                if let Some(extension) = output.extensions.iter().find(|e| &e.name == name) {
                    output
                        .grammars
                        .retain(|grammar| !extension.grammars.contains(&grammar.id));
                }
            }

            output
                .extensions
                .retain(|existing| !removed_extensions.contains(&existing.name));

            // Filter remaining extensions/grammars
            let extensions = extensions
                .into_iter()
                .filter(|extension| {
                    // Skip extension that haven't changed.
                    if let Some(existing) = output
                        .extensions
                        .iter()
                        .find(|existing| existing.name == extension.name)
                    {
                        if existing.version >= extension.version {
                            tracing::debug!(name = extension.name, "Skipping unchanged extension");
                            return false;
                        }

                        tracing::info!(name = extension.name, "New extension version");
                    }

                    true
                })
                .collect::<Vec<_>>();

            let limit = std::thread::available_parallelism().map_or(1, NonZero::get) * 2;
            let semaphore = Arc::new(Semaphore::new(limit));

            let mut futures = FuturesUnordered::new();
            for extension in extensions {
                let semaphore = Arc::clone(&semaphore);

                let span = tracing::info_span!(
                    "process_extension",
                    name = %extension.name,
                    version = %extension.version,
                );

                let future = async move {
                    let _acquire = semaphore.acquire().await;
                    process_extension(extension).instrument(span).await
                };

                futures.push(future);
            }

            while let Some(result) = futures.next().await {
                match result {
                    Ok(Some((extension, grammars))) => {
                        // Remove outdated extensions and grammars from output.
                        if let Some(outdated) = output
                            .extensions
                            .iter()
                            .find(|existing| existing.name == extension.name)
                            .map(|existing| existing.grammars.clone())
                        {
                            output
                                .grammars
                                .retain(|grammar| !outdated.contains(&grammar.id));

                            output
                                .extensions
                                .retain(|existing| existing.name != extension.name);
                        }

                        output.extensions.push(extension);
                        output.grammars.extend(grammars);
                    },
                    Ok(_) => (),
                    Err(err) => tracing::error!(
                        err = ?err,
                        "Error processing extension"
                    ),
                }
            }

            tracing::info!("Writing output");

            output.extensions.sort_by(|a, b| a.name.cmp(&b.name));
            output.grammars.sort_by(|a, b| a.id.cmp(&b.id));

            // Write extension files
            if !extensions_dir.exists() {
                fs::create_dir_all(extensions_dir).await?;
            }

            let mut existing = fs::read_dir(extensions_dir).await?;
            while let Some(entry) = existing.try_next().await? {
                let file_name = entry.file_name();
                let file_name = file_name.to_string_lossy();

                if let Some(name) = file_name.strip_suffix(".json")
                    && !output
                        .extensions
                        .iter()
                        .any(|extension| extension.name == name)
                {
                    tracing::info!(name = name, "Removing stale extension file");
                    fs::remove_file(entry.path()).await?;
                }

                if let Some(name) = file_name.strip_suffix(".lock")
                    && !output
                        .extensions
                        .iter()
                        .any(|extension| extension.name == name)
                {
                    tracing::info!(name = name, "Removing stale lockfile");
                    fs::remove_file(entry.path()).await?;
                }
            }

            for extension in &output.extensions {
                let name = &extension.name;
                let path = extensions_dir.join(format!("{name}.json"));
                let json = serde_json::to_string_pretty(&extension)?;
                fs::write(path, json).await?;
            }

            // Write grammar files
            if !grammars_dir.exists() {
                fs::create_dir_all(grammars_dir).await?;
            }

            let mut existing = fs::read_dir(grammars_dir).await?;
            while let Some(entry) = existing.try_next().await? {
                let id = entry
                    .file_name()
                    .to_string_lossy()
                    .trim_end_matches(".json")
                    .to_owned();

                if !output.grammars.iter().any(|grammar| grammar.id == id) {
                    tracing::info!(id = id, "Removing stale grammar file");
                    fs::remove_file(entry.path()).await?;
                }
            }

            for grammar in &output.grammars {
                let id = &grammar.id;
                let path = grammars_dir.join(format!("{id}.json"));
                let json = serde_json::to_string_pretty(&grammar)?;
                fs::write(path, json).await?;
            }

            fs::remove_dir_all(tmp_registry).await?;
        },

        Some("populate") => {
            let path = Path::new(".");

            let manifest_path = path.join("extension.toml");
            if !manifest_path.exists() {
                anyhow::bail!("Missing extension.toml");
            }

            let manifest = fs::read_to_string(&manifest_path).await?;
            let mut manifest: ExtensionManifest = toml::from_str(&manifest)?;

            let wasm = &path.join("extension.wasm");
            if wasm.exists() {
                let version = extract_zed_api_version(wasm)?;
                manifest.lib.version = Some(version);
            }

            let languages = &path.join("languages");
            if languages.exists() {
                let mut language_entries = fs::read_dir(languages).await?;
                while let Some(language) = language_entries.try_next().await? {
                    let language_path = language.path();
                    let config = language_path.join("config.toml");
                    if fs::metadata(&config).await.is_ok() {
                        let relative_language_dir = language_path.strip_prefix(path)?.to_path_buf();
                        if !manifest.languages.contains(&relative_language_dir) {
                            manifest.languages.push(relative_language_dir);
                        }
                    }
                }
            }

            let themes = &path.join("themes");
            if themes.exists() {
                let mut theme_entries = fs::read_dir(themes).await?;
                while let Some(theme) = theme_entries.try_next().await? {
                    let theme_path = theme.path();
                    if theme_path.extension() == Some("json".as_ref()) {
                        let relative_theme_path = theme_path.strip_prefix(path)?.to_path_buf();
                        if !manifest.themes.contains(&relative_theme_path) {
                            manifest.themes.push(relative_theme_path);
                        }
                    }
                }
            }

            let icon_themes = &path.join("icon_themes");
            if icon_themes.exists() {
                let mut icon_theme_entries = fs::read_dir(icon_themes).await?;
                while let Some(icon_theme) = icon_theme_entries.try_next().await? {
                    let icon_theme_path = icon_theme.path();
                    if icon_theme_path.extension() == Some("json".as_ref()) {
                        let relative_icon_theme_path =
                            icon_theme_path.strip_prefix(path)?.to_path_buf();
                        if !manifest.icon_themes.contains(&relative_icon_theme_path) {
                            manifest.icon_themes.push(relative_icon_theme_path);
                        }
                    }
                }
            }

            let snippets = &path.join("snippets.json");
            if manifest.snippets.is_none() && fs::metadata(snippets).await.is_ok() {
                manifest.snippets = Some(manifest::ExtensionSnippets::Single(snippets.to_owned()));
            }

            tracing::info!("Writing output");
            let manifest = toml::to_string_pretty(&manifest)?;
            fs::write(manifest_path, manifest).await?;
        },

        Some("install") => {
            let out = PathBuf::from(&args[2]);
            let grammars: Vec<(&str, PathBuf)> = args[3..]
                .iter()
                .map(|grammar| {
                    let Some((name, path)) = grammar.split_once(':') else {
                        anyhow::bail!("Invalid grammar argument: '{grammar}' (expected name:path)");
                    };

                    Ok((name, Path::new(path).join("share/zed/grammars")))
                })
                .collect::<anyhow::Result<_>>()?;

            let manifest_path = Path::new("extension.toml");
            if !manifest_path.exists() {
                anyhow::bail!("Missing extension.toml");
            }

            let manifest = fs::read_to_string(manifest_path).await?;
            let manifest: ExtensionManifest = toml::from_str(&manifest)?;

            let extension_dir = out.join("share/zed/extensions").join(&manifest.id);
            fs::create_dir_all(&extension_dir).await?;
            fs::copy(manifest_path, extension_dir.join("extension.toml")).await?;

            let wasm = Path::new("extension.wasm");
            if wasm.exists() {
                fs::copy(wasm, extension_dir.join("extension.wasm")).await?;
            }

            copy::copy_items(&manifest.themes, &extension_dir).await?;
            copy::copy_items(&manifest.icon_themes, &extension_dir).await?;
            copy::copy_items(&manifest.languages, &extension_dir).await?;

            if let Some(snippets) = &manifest.snippets {
                let snippets: Vec<_> = snippets.paths().collect();
                copy::copy_items(&snippets, &extension_dir).await?;
            }

            if !manifest.icon_themes.is_empty() {
                let icons = Path::new("icons");
                if icons.exists() {
                    copy::copy_items(&[icons], &extension_dir).await?;
                }
            }

            for (name, entry) in &manifest.debug_adapters {
                let schema_path = entry.schema_path.clone().unwrap_or_else(|| {
                    Path::new("debug_adapter_schemas")
                        .join(name)
                        .with_extension("json")
                });

                if schema_path.exists() {
                    copy::copy_items(&[&schema_path], &extension_dir).await?;
                }
            }

            for entry in manifest.agent_servers.values() {
                if let Some(icon) = &entry.icon {
                    let icon_path = Path::new(icon);
                    if icon_path.exists() {
                        copy::copy_items(&[icon_path], &extension_dir).await?;
                    }
                }
            }

            // Symlink grammars
            if !grammars.is_empty() {
                let grammars_dir = extension_dir.join("grammars");
                fs::create_dir_all(&grammars_dir).await?;

                for (_, path) in &grammars {
                    let mut entries = fs::read_dir(path).await?;
                    while let Some(entry) = entries.try_next().await? {
                        unix::symlink(entry.path(), grammars_dir.join(entry.file_name())).await?;
                    }
                }
            }

            tracing::info!("Extension installed");
        },

        _ => {
            anyhow::bail!("Unknown command");
        },
    }

    Ok(())
}
