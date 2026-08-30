use std::{collections::BTreeMap, env::temp_dir};

use futures_util::stream::FuturesUnordered;
use smol::{fs, stream::StreamExt};
use tracing::Instrument;

use super::{checkout_git_repo, prefetch_git_repo};
use crate::{manifest::GrammarManifestEntry, output::Grammar};

pub struct ProcessedGrammars {
    pub grammars: Vec<Grammar>,
    pub ids: Vec<String>,
}

pub async fn process_grammars(
    grammars: BTreeMap<String, GrammarManifestEntry>,
    name: &str,
) -> anyhow::Result<ProcessedGrammars> {
    let mut futures = FuturesUnordered::new();
    for (grammar_name, grammar) in grammars {
        let name = name.to_owned();

        let span = tracing::info_span!(
            "process_grammar",
            name = %grammar_name,
            extension = %name,
            repo = %grammar.repository,
            rev = %grammar.rev,
        );

        let future = async move {
            process_grammar(grammar_name, grammar, name)
                .instrument(span)
                .await
        };

        futures.push(future);
    }

    let mut processed_grammars = vec![];
    while let Some(result) = futures.next().await {
        match result {
            Ok(Some(grammar)) => {
                processed_grammars.push(grammar);
            },
            Ok(_) => (),
            Err(err) => {
                tracing::error!(
                    err = ?err,
                    "Error processing grammar"
                );

                return Err(err);
            },
        }
    }

    processed_grammars.sort_by(|a, b| a.id.cmp(&b.id));
    let ids = processed_grammars.iter().map(|g| g.id.clone()).collect();

    Ok(ProcessedGrammars {
        grammars: processed_grammars,
        ids,
    })
}

async fn process_grammar(
    name: String,
    grammar: GrammarManifestEntry,
    extension: String,
) -> anyhow::Result<Option<Grammar>> {
    let id = format!("{extension}_{name}");
    let tmp_repo = temp_dir().join(&id);

    let repo = grammar.repository.clone();
    let rev = grammar.rev.clone();
    checkout_git_repo(&repo, &rev, &tmp_repo).await?;

    let src = prefetch_git_repo(&repo, &rev, false).await?;
    fs::remove_dir_all(&tmp_repo).await?;

    let root = grammar
        .path
        .clone()
        .map(|s| s.trim_start_matches("./").to_owned());

    Ok(Some(Grammar {
        id,
        name: name.clone(),
        version: rev,
        src,
        root,
    }))
}
