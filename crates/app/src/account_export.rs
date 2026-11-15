//! Account export, import and deletion (REQ-044).
//!
//! The export carries everything one person owns: the profile, the bots, the messages of the
//! conversation of each bot, the bot-scoped memory for each bot, the routines and the artifacts.
//! Shared rooms other people keep in the same space are not theirs and are never included.
//!
//! The archive is a zip with a JSON manifest (`version: 1`). The importer refuses an unknown
//! version, a file outside its section name list and a path that escapes the archive root, and
//! stops there; per-file refusals are reported, never finger-printed.

use std::io::{Cursor, Write as _};

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{App, AppError, ports};

/// The step between "the person asked" and "the archive exists" — no orchestration beyond a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportStartedOutcome {
    /// The export id, from the deployment's id generator.
    pub export_id: String,
    /// `pending` right now, `ready` once the archive is rendered.
    pub status: String,
}

/// The counts of each section the archive carries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExportCounts {
    /// The number of bots.
    pub bots: u64,
    /// The number of threads.
    pub threads: u64,
    /// The number of messages.
    pub messages: u64,
    /// The number of bot-scoped memory documents.
    pub memory: u64,
    /// The number of routines.
    pub routines: u64,
    /// The number of artifacts.
    pub artifacts: u64,
}

impl ExportCounts {
    fn wire(&self) -> ExportCountsWire {
        ExportCountsWire {
            bots: self.bots,
            threads: self.threads,
            messages: self.messages,
            memory: self.memory,
            routines: self.routines,
            artifacts: self.artifacts,
        }
    }
}

/// The outcome of `account.export.status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportStatusOutcome {
    /// The export id.
    pub export_id: String,
    /// `pending` for a fresh request, `ready` for one rendered by a previous call.
    pub status: String,
    /// The counts of each section.
    pub counts: ExportCounts,
    /// Where the browser can pick the archive up.
    pub download_url: String,
}

/// The shape the archive manifest must have.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportManifest {
    /// The manifest format version.
    pub version: u32,
    /// The instant of the export.
    pub exported_at: String,
    /// The id of the person this archive belongs to.
    pub user_id: String,
    /// The counts of each section of the archive.
    pub counts: ExportCountsWire,
    /// The entries the export refused (paths inside the archive).
    #[serde(default)]
    pub skipped: Vec<String>,
}

/// The count section of the manifest, as wire JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportCountsWire {
    /// Bots in the archive.
    pub bots: u64,
    /// Threads in the archive.
    pub threads: u64,
    /// Messages in the archive.
    pub messages: u64,
    /// Bot-scoped memory documents in the archive.
    pub memory: u64,
    /// Routines in the archive.
    pub routines: u64,
    /// Artifacts in the archive.
    pub artifacts: u64,
}

/// The cap on one entry inside the archive.
pub const ENTRY_BYTE_BUDGET: u64 = 8 * 1024 * 1024;

/// The cap on the whole archive.
pub const BYTE_BUDGET: u64 = 32 * 1024 * 1024;

/// The request-size cap for an import.
pub const IMPORT_BYTE_BUDGET: usize = 32 * 1024 * 1024 + 1024;

fn counts_of(bundle: &ports::ExportBundle) -> ExportCounts {
    ExportCounts {
        bots: u64::try_from(bundle.bots.len()).unwrap_or(u64::MAX),
        threads: u64::try_from(bundle.threads.len()).unwrap_or(u64::MAX),
        messages: u64::try_from(bundle.messages.len()).unwrap_or(u64::MAX),
        memory: u64::try_from(bundle.bot_memory.len()).unwrap_or(u64::MAX),
        routines: u64::try_from(bundle.routines.len()).unwrap_or(u64::MAX),
        artifacts: u64::try_from(bundle.artifacts.len()).unwrap_or(u64::MAX),
    }
}

fn serialize_json<T: Serialize>(value: &T) -> Result<Vec<u8>, AppError> {
    serde_json::to_vec_pretty(value).map_err(|_| AppError::Unavailable)
}

fn add_file(
    zip: &mut zip::ZipWriter<Cursor<Vec<u8>>>,
    name: &str,
    bytes: &[u8],
) -> Result<(), AppError> {
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file(name, options)
        .map_err(|_| AppError::Unavailable)?;
    zip.write_all(bytes).map_err(|_| AppError::Unavailable)?;
    Ok(())
}

/// Resolves artifact payloads within the budget, decide refusals up front.
fn fit_artifact_payloads(
    artifacts: &[ports::ArtifactMeta],
    bytes: &[Vec<u8>],
) -> (Vec<Option<Vec<u8>>>, Vec<String>) {
    let mut out = Vec::with_capacity(artifacts.len());
    let mut skipped = Vec::new();
    let mut used: u64 = 0;
    for (index, artifact) in artifacts.iter().enumerate() {
        let size = u64::try_from(bytes[index].len()).unwrap_or(u64::MAX);
        if size > ENTRY_BYTE_BUDGET {
            skipped.push(format!("artifacts/{}: entry exceeds 8 MiB", artifact.id));
            out.push(None);
            continue;
        }
        if used.saturating_add(size) > BYTE_BUDGET {
            skipped.push(format!("artifacts/{}: archive exceeds 32 MiB", artifact.id));
            out.push(None);
            continue;
        }
        used = used.saturating_add(size);
        out.push(Some(bytes[index].clone()));
    }
    (out, skipped)
}

/// Builds the zip for one bundle.
fn render_zip(
    bundle: &ports::ExportBundle,
    exported_at: &str,
    artifact_bytes: &[Vec<u8>],
    fitting_skipped: Vec<String>,
) -> Result<(Vec<u8>, ExportCounts), AppError> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let counts = counts_of(bundle);

    let manifest = ExportManifest {
        version: 1,
        exported_at: exported_at.to_owned(),
        user_id: bundle.profile.id.as_str().to_owned(),
        counts: counts.wire(),
        skipped: fitting_skipped,
    };
    add_file(&mut zip, "manifest.json", &serialize_json(&manifest)?)?;
    add_file(&mut zip, "profile.json", &serialize_json(&bundle.profile)?)?;
    add_file(&mut zip, "bots.json", &serialize_json(&bundle.bots)?)?;
    add_file(&mut zip, "threads.json", &serialize_json(&bundle.threads)?)?;
    add_file(
        &mut zip,
        "messages.json",
        &serialize_json(&bundle.messages)?,
    )?;
    add_file(
        &mut zip,
        "memories.json",
        &serialize_json(&bundle.bot_memory)?,
    )?;
    add_file(
        &mut zip,
        "routines.json",
        &serialize_json(&bundle.routines)?,
    )?;
    add_file(
        &mut zip,
        "artifacts.json",
        &serialize_json(&bundle.artifacts)?,
    )?;
    for (artifact, bytes) in bundle.artifacts.iter().zip(artifact_bytes.iter()) {
        if bytes.is_empty() {
            continue;
        }
        add_file(
            &mut zip,
            &format!("artifacts/{}", artifact.storage_key),
            bytes,
        )?;
    }
    let zip = zip
        .finish()
        .map_err(|_| AppError::Unavailable)?
        .into_inner();
    Ok((zip, counts))
}

/// Reads the bytes of each artifact within `cp: []` budget-order.
async fn load_artifact_bytes(
    app: &App,
    bundle: &ports::ExportBundle,
) -> Result<(Vec<Vec<u8>>, Vec<String>), AppError> {
    let mut bytes = Vec::with_capacity(bundle.artifacts.len());
    let mut skipped = Vec::new();
    for artifact in &bundle.artifacts {
        match app
            .services
            .artifact_content
            .read(&artifact.storage_key)
            .await
        {
            Ok(artifact_bytes) => bytes.push(artifact_bytes),
            Err(AppError::NotFound) => {
                skipped.push(format!(
                    "artifacts/{}: bytes are absent from storage",
                    artifact.id
                ));
                bytes.push(Vec::new());
            }
            Err(error) => return Err(error),
        }
    }
    Ok((bytes, skipped))
}

/// Starts an export: one row, status pending; the archive renders on the next status/dump.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the caller is not a member, and an error when a store
/// cannot answer.
pub async fn account_export_start(
    app: &App,
    context: &boma_domain::TenantContext,
) -> Result<ExportStartedOutcome, AppError> {
    let user_id = context.user_id().cloned().ok_or(AppError::NotFound)?;
    let export_id = app.services.ids.new_id("exp");
    let now = app.services.clock.now();
    app.stores
        .account_export
        .create(&export_id, &user_id, &now)
        .await?;
    Ok(ExportStartedOutcome {
        export_id,
        status: "pending".to_owned(),
    })
}

/// The status of an export; freshly renders the bundle on every call so the answer is honest
/// without holding the bytes hostage.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the export is not the caller's, and an error when a store
/// cannot answer.
pub async fn account_export_status(
    app: &App,
    context: &boma_domain::TenantContext,
    id: &str,
) -> Result<ExportStatusOutcome, AppError> {
    let user_id = context.user_id().cloned().ok_or(AppError::NotFound)?;
    let row = app
        .stores
        .account_export
        .find(id, &user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let bundle = app.stores.account_export.bundle(&user_id).await?;
    Ok(ExportStatusOutcome {
        export_id: row.id,
        status: row.status,
        counts: counts_of(&bundle),
        download_url: format!("/api/v1/me/exports/{id}"),
    })
}

/// The archive bytes; renders on demand, marks the export ready, keeps the auditor honest.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the export is not the caller's, and an error when a store
/// cannot answer — including when an artifact was unreadable and the archive cannot be written.
pub async fn account_export_download(
    app: &App,
    context: &boma_domain::TenantContext,
    id: &str,
) -> Result<Vec<u8>, AppError> {
    let user_id = context.user_id().cloned().ok_or(AppError::NotFound)?;
    let row = app
        .stores
        .account_export
        .find(id, &user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let _ = row;
    let bundle = app.stores.account_export.bundle(&user_id).await?;
    let exported_at = app.services.clock.now();
    let (artifact_bytes, read_skipped) = load_artifact_bytes(app, &bundle).await?;
    let (fitting, fit_skipped) = fit_artifact_payloads(&bundle.artifacts, &artifact_bytes);
    let skipped = read_skipped
        .into_iter()
        .chain(fit_skipped)
        .collect::<Vec<_>>();
    let fitting_bytes: Vec<Vec<u8>> = fitting.into_iter().map(Option::unwrap_or_default).collect();
    let (zip, _counts) = render_zip(&bundle, &exported_at, &fitting_bytes, skipped.clone())?;
    let now = app.services.clock.now();
    app.stores
        .account_export
        .mark_ready(
            id,
            i64::try_from(zip.len()).unwrap_or(i64::MAX),
            skipped,
            &now,
        )
        .await?;
    Ok(zip)
}

/// The outcome of `account.delete` once the confirmation is accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteOutcome {
    /// Always `deleted` on success; useful for clients to branch on.
    pub status: String,
}

/// The outcome of `account.import`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOutcome {
    /// The counts of the imported sections, limited to what was accepted.
    pub counts: ExportCounts,
    /// The entries inside the archive that could not be imported, named by section/key.
    pub refused: Vec<String>,
}

/// Validates the confirmation word before any irreversible work happens.
#[must_use]
pub fn confirmation_is_literal(body: &str) -> bool {
    body.trim().eq_ignore_ascii_case("delete")
}

/// Refuses to delete the account when another person shares one of the spaces it belongs to.
///
/// The wording is deliberate: it names the space that holds back the deletion, and never says
/// who the other person is.
///
/// # Errors
///
/// Returns an error when a space cannot name its membership, when a billing check fails and the
/// account must settle first, or when the database refuses the cascade.
pub async fn account_delete(
    app: &App,
    context: &boma_domain::TenantContext,
) -> Result<DeleteOutcome, AppError> {
    let user_id = context.user_id().cloned().ok_or(AppError::Forbidden)?;

    let spaces = app.stores.spaces.spaces_of(&user_id).await?;
    let mut blocks: Vec<String> = Vec::new();
    for space in &spaces {
        let members = app.stores.spaces.member_ids_of(&space.id).await?;
        let others = members.iter().filter(|member| **member != user_id).count();
        if others > 0 {
            blocks.push(space.name.clone());
        }
    }
    if !blocks.is_empty() {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            format!(
                "the account shares a space with other people: {}; leave it or ask for removal first",
                blocks.join(", ")
            ),
        )));
    }

    // Any active pro subscription in any owned space blocks deletion: the billing relationship
    // must be settled first (REQ-044).
    for space in &spaces {
        let member = boma_domain::TenantContext::Member {
            space_id: space.id.clone(),
            user_id: user_id.clone(),
        };
        let subscription = app.stores.billing.subscription(&member).await?;
        if subscription.plan == "pro" && subscription.status == "active" {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                format!(
                    "the space \"{}\" is on an active pro plan; cancel it before deleting the account",
                    space.name
                ),
            )));
        }
    }

    // Collect artifact storage keys *before* the cascade: artifact bytes are out-of-band.
    let bundle = app.stores.account_export.bundle(&user_id).await?;
    let artifact_keys: Vec<String> = bundle
        .artifacts
        .iter()
        .map(|artifact| artifact.storage_key.clone())
        .collect();

    // The cascade: every organization for which the caller is the sole member disappears; the
    // users row is removed last; membership cascade happens through foreign keys.
    let now = app.services.clock.now();
    app.stores.accounts.delete_account(&user_id, &now).await?;

    // Artifact bytes are out-of-band; delete them now, best effort. A missing blob is not a
    // failure: the row is gone either way and a stranded file is only wasted disk.
    for key in artifact_keys {
        let _ = app.services.artifact_content.remove(&key).await;
    }

    let _ = app
        .stores
        .audit
        .append(crate::ports::NewAuditEvent {
            id: app.services.ids.new_id("aud"),
            space_id: None,
            actor_user_id: None,
            kind: boma_domain::AuditKind::AccountDeleted,
            target: "account".to_owned(),
            metadata: serde_json::json!({}),
            created_at: now,
        })
        .await;

    Ok(DeleteOutcome {
        status: "deleted".to_owned(),
    })
}

// ---------------------------------------------------------------------------
// Import (REQ-044)
// ---------------------------------------------------------------------------

/// Validates one entry name; refuses anything outside the declared archive shape.
fn allowed_entry(name: &str) -> bool {
    // Reject any path with components that would escape the root.
    name.split('/')
        .all(|part| !part.is_empty() && part != "." && part != "..")
        && matches!(
            name,
            "manifest.json"
                | "profile.json"
                | "bots.json"
                | "threads.json"
                | "messages.json"
                | "memories.json"
                | "routines.json"
                | "artifacts.json"
        )
}

/// Reads and validates the archive; returns the named sections.
fn read_archive(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, AppError> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|_| {
        AppError::Rule(boma_domain::DomainError::InvalidName(
            "the file is not a zip archive".to_owned(),
        ))
    })?;
    let mut out = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|_| AppError::NotFound)?;
        let name = entry
            .name()
            .map_err(|_| AppError::Unavailable)?
            .into_owned();
        if !allowed_entry(&name) {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                format!("the file {name:?} is not part of an account export archive"),
            )));
        }
        if entry.size() > ENTRY_BYTE_BUDGET {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                format!("the file {name:?} exceeds the 8 MiB per-entry budget"),
            )));
        }
        let mut buf = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
        std::io::Read::read_to_end(&mut entry, &mut buf).map_err(|_| AppError::Unavailable)?;
        out.insert(name, buf);
    }
    let manifest = out.get("manifest.json").ok_or(AppError::NotFound)?;
    let manifest: ExportManifest = serde_json::from_slice(manifest).map_err(|_| {
        AppError::Rule(boma_domain::DomainError::InvalidName(
            "the manifest does not match the account export schema".to_owned(),
        ))
    })?;
    if manifest.version != 1 {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            format!(
                "the manifest version is {}, this build understands exactly 1",
                manifest.version
            ),
        )));
    }
    Ok(out)
}

/// Best-effort import of one archive into the caller's space.
///
/// Rules the caller must know (also surfaced in the UI copy once the import screen exists):
///
/// * The import lands in the **caller's current default space**, never anywhere else.
/// * Existing bot ids, thread ids, message ids and artifact ids in the archive are **not reused**;
///   everything the importer writes is freshly minted.
/// * Profile data (the person) is not imported; passwords are never inside an archive.
/// * Artifacts only import when they fit the per-entry and whole-archive budgets.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] for a bad archive shape and an error when a store refuses a
/// section or the caller tries to import into a space that no longer exists.
#[allow(clippy::too_many_lines)] // one path per section; removing it cuts nothing else
pub async fn account_import(
    app: &App,
    context: &boma_domain::TenantContext,
    bytes: &[u8],
) -> Result<ImportOutcome, AppError> {
    let entries = read_archive(bytes)?;
    let mut counts = ExportCounts {
        bots: 0,
        threads: 0,
        messages: 0,
        memory: 0,
        routines: 0,
        artifacts: 0,
    };
    let mut refused: Vec<String> = Vec::new();

    // Parse optional sections.
    let bots: Vec<boma_domain::Bot> = read_optional(&entries, "bots.json", app).unwrap_or_default();
    let _threads: Vec<boma_domain::Thread> =
        read_optional(&entries, "threads.json", app).unwrap_or_default();
    let messages: Vec<boma_domain::Message> =
        read_optional(&entries, "messages.json", app).unwrap_or_default();
    let memories: Vec<boma_domain::MemoryDocument> =
        read_optional(&entries, "memories.json", app).unwrap_or_default();
    let routines: Vec<crate::ports::Routine> =
        read_optional(&entries, "routines.json", app).unwrap_or_default();

    // The bots become new, caller-owned bots; their threads follow the same creation helper.
    let mut new_bot_ids: BTreeMap<String, boma_domain::BotId> = BTreeMap::new();
    for bot in &bots {
        match crate::usecases::create_bot(
            app,
            context,
            &bot.name,
            bot.description.clone(),
            bot.color.clone(),
        )
        .await
        {
            Ok(created) => {
                new_bot_ids.insert(bot.id.as_str().to_owned(), created.bot.id);
                counts.bots += 1;
            }
            Err(error) => {
                refused.push(format!("bots/{}: {}", bot.id, error));
            }
        }
    }

    // Memory follows, once the new bot exists.
    for doc in &memories {
        let new_bot_id = match &doc.bot_id {
            Some(bot_id) => new_bot_ids.get(bot_id.as_str()).cloned(),
            None => None,
        };
        let Some(bot_id) = new_bot_id else {
            refused.push(format!(
                "memories/{}: source bot was not imported",
                doc.path
            ));
            continue;
        };
        match crate::usecases::memory_write(
            app,
            context,
            "bot",
            Some(bot_id.as_str()),
            &doc.path,
            &doc.content,
        )
        .await
        {
            Ok(_) => counts.memory += 1,
            Err(error) => {
                refused.push(format!("memories/{}: {}", doc.path, error));
            }
        }
    }

    // Routines follow the bot
    for routine in &routines {
        let new_bot_id = new_bot_ids.get(routine.bot_id.as_str()).cloned();
        let Some(bot_id) = new_bot_id else {
            refused.push(format!(
                "routines/{}: source bot was not imported",
                routine.id
            ));
            continue;
        };
        match crate::usecases::routines_create(
            app,
            context,
            bot_id.as_str(),
            &routine.name,
            &routine.prompt,
            &routine.crons,
            &routine.timezone,
            routine.active,
            routine.notify,
            routine.github_enabled,
            routine.email_senders.clone(),
        )
        .await
        {
            Ok(_) => counts.routines += 1,
            Err(error) => {
                refused.push(format!("routines/{}: {}", routine.id, error));
            }
        }
    }

    // Messages land last: the thread for each new bot exists once create_bot runs.
    let mut thread_of_bot: BTreeMap<String, boma_domain::ThreadId> = BTreeMap::new();
    for bot in &bots {
        let new_id = new_bot_ids.get(bot.id.as_str()).cloned();
        let Some(new_id) = new_id else { continue };
        let thread = app.stores.threads.for_bot(context, &new_id).await;
        if let Ok(Some(thread)) = thread {
            thread_of_bot.insert(bot.id.as_str().to_owned(), thread.id);
        }
    }
    for message in messages {
        let thread = thread_of_bot.values().next().cloned();
        let Some(thread_id) = thread else {
            refused.push(format!("messages/{}: no imported bot threads", message.id));
            continue;
        };
        let now = app.services.clock.now();
        match app
            .stores
            .messages
            .insert(
                context,
                crate::ports::NewMessage {
                    id: boma_domain::MessageId::new(app.services.ids.new_id("msg"))?,
                    space_id: context.space_id().cloned().ok_or(AppError::NotFound)?,
                    thread_id,
                    role: message.role,
                    blocks: message.blocks,
                    bot_id: None,
                    author_user_id: message.author_user_id,
                    run_id: None,
                    client_nonce: None,
                    created_at: now,
                    reply_to_message_id: message.reply_to_message_id,
                    reply_quote: message.reply_quote,
                },
            )
            .await
        {
            Ok(_) => counts.messages += 1,
            Err(error) => {
                refused.push(format!("messages/{}: {}", message.id, error));
            }
        }
    }

    let _ = app
        .stores
        .audit
        .append(crate::ports::NewAuditEvent {
            id: app.services.ids.new_id("aud"),
            space_id: None,
            actor_user_id: None,
            kind: boma_domain::AuditKind::ImportCompleted,
            target: "account-export-import".to_owned(),
            metadata: serde_json::json!({
                "bots": counts.bots,
                "messages": counts.messages,
                "memories": counts.memory,
                "routines": counts.routines,
            }),
            created_at: app.services.clock.now(),
        })
        .await;

    Ok(ImportOutcome { counts, refused })
}

/// Reads one optional JSON section; missing sections are empty, present ones are strictly parsed.
fn read_optional<T: serde::de::DeserializeOwned>(
    entries: &BTreeMap<String, Vec<u8>>,
    key: &str,
    _app: &App,
) -> Result<Vec<T>, AppError> {
    let Some(bytes) = entries.get(key) else {
        return Ok(Vec::new());
    };
    serde_json::from_slice(bytes).map_err(|_| {
        AppError::Rule(boma_domain::DomainError::InvalidName(format!(
            "the archive section {key:?} does not match the account export schema"
        )))
    })
}

/// A file's safe name, never a path.
#[must_use]
pub fn export_filename(export_id: &str) -> String {
    format!("{export_id}.zip")
}
