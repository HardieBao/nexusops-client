use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{
    content::{
        canonical_manifest, is_text_path, normalize_text, portable_path, reserve_path, sha256_hex,
        ContentError, ContentLimits, PathNode, VerifiedContent,
    },
    types::{AssetKind, FileEntry, ManifestItem},
};

const CONTROL_DIR: &str = ".nexusops-team";
const KIND_MISMATCH_HASH_INPUT: &[u8] = b"nexusops-team-local-node-kind-mismatch-v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DriftStatus {
    NotInstalled,
    Same,
    RemoteUpdate,
    LocalModified,
    BothModified,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallPolicy {
    pub overwrite_local: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallReceipt {
    pub local_hash: String,
    pub install_path: PathBuf,
    pub backup: Option<PathBuf>,
    pub backup_dir: Option<PathBuf>,
    pub operation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestoreReceipt {
    pub local_hash: Option<String>,
    pub install_path: PathBuf,
    pub displaced_backup: Option<PathBuf>,
    pub backup_dir: PathBuf,
    pub operation: String,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    Subscribed,
    Unsubscribed,
}

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error(transparent)]
    Content(#[from] ContentError),
    #[error("the Team install root must be an existing ordinary directory")]
    InvalidRoot,
    #[error("the Team managed path is unsafe")]
    UnsafePath,
    #[error("a symlink or reparse point crosses the Team managed boundary")]
    UnsafeFilesystem,
    #[error("local Team content needs an explicit overwrite decision: {0:?}")]
    Conflict(DriftStatus),
    #[error("the local Team content changed after sync preview")]
    PreviewStale,
    #[error("the Team install could not commit: {0}")]
    Commit(String),
    #[error("an interrupted Team install could not be recovered")]
    Recovery,
    #[error("Team recovery conflict: {0}")]
    RecoveryConflict(String),
    #[error("Team filesystem operation failed")]
    Io(#[from] io::Error),
    #[error("Team install journal is invalid")]
    InvalidJournal,
}

pub fn classify_drift(
    last_installed_hash: Option<&str>,
    local_current_hash: Option<&str>,
    remote_hash: &str,
) -> DriftStatus {
    match (last_installed_hash, local_current_hash) {
        (_, None) => DriftStatus::NotInstalled,
        (None, Some(_)) => DriftStatus::LocalModified,
        (Some(last), Some(local)) if local == last && remote_hash == last => DriftStatus::Same,
        (Some(last), Some(local)) if local == last && remote_hash != last => {
            DriftStatus::RemoteUpdate
        }
        (Some(last), Some(_)) if remote_hash == last => DriftStatus::LocalModified,
        (Some(_), Some(_)) => DriftStatus::BothModified,
    }
}

#[cfg(test)]
pub fn subscription_status(authorized_by_manifest: bool) -> SubscriptionStatus {
    if authorized_by_manifest {
        SubscriptionStatus::Subscribed
    } else {
        SubscriptionStatus::Unsubscribed
    }
}

#[cfg(test)]
pub fn inspect_local(
    item: &ManifestItem,
    root: &Path,
    relative: &Path,
) -> Result<Option<String>, InstallError> {
    inspect_local_with_limits(item, root, relative, ContentLimits::default())
}

pub fn inspect_local_with_limits(
    item: &ManifestItem,
    root: &Path,
    relative: &Path,
    limits: ContentLimits,
) -> Result<Option<String>, InstallError> {
    let context = RootContext::open(root, relative, limits)?;
    let baseline = read_baseline(&context)?;
    if item.kind == AssetKind::Skill && baseline.is_none() && !item.files.is_empty() {
        if !path_exists_no_follow(&context.target)? {
            return Ok(None);
        }
        require_safe_tree(&context.target)?;
        if !context.target.symlink_metadata()?.is_dir() {
            return Ok(Some(sha256_hex(KIND_MISMATCH_HASH_INPUT)));
        }
        return Ok(Some(hash_local_skill(
            &context.target,
            context.limits,
            Some(&item.files),
        )?));
    }
    inspect_target(&context, item.kind.clone(), baseline.as_ref())
}

pub(super) fn inspect_skill_physical_with_limits(
    item: &ManifestItem,
    root: &Path,
    relative: &Path,
    limits: ContentLimits,
) -> Result<Option<String>, InstallError> {
    if item.kind != AssetKind::Skill {
        return Err(InstallError::UnsafePath);
    }
    let context = RootContext::open(root, relative, limits)?;
    if !path_exists_no_follow(&context.target)? {
        return Ok(None);
    }
    require_safe_tree(&context.target)?;
    if !context.target.symlink_metadata()?.is_dir() {
        return Ok(Some(sha256_hex(KIND_MISMATCH_HASH_INPUT)));
    }
    #[cfg(windows)]
    let mode_files = item
        .files
        .iter()
        .cloned()
        .map(|mut file| {
            file.executable = false;
            file
        })
        .collect::<Vec<_>>();
    #[cfg(windows)]
    let mode_files = Some(mode_files.as_slice());
    #[cfg(not(windows))]
    let mode_files = None;
    Ok(Some(hash_local_skill(
        &context.target,
        context.limits,
        mode_files,
    )?))
}

#[cfg(test)]
pub fn install_verified(
    item: &ManifestItem,
    downloaded: &[u8],
    root: &Path,
    relative: &Path,
    baseline_hash: Option<&str>,
    policy: InstallPolicy,
) -> Result<InstallReceipt, InstallError> {
    install_verified_with_limits(
        item,
        downloaded,
        root,
        relative,
        baseline_hash,
        None,
        policy,
        ContentLimits::default(),
    )
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub fn install_verified_with_limits(
    item: &ManifestItem,
    downloaded: &[u8],
    root: &Path,
    relative: &Path,
    baseline_hash: Option<&str>,
    expected_local_hash: Option<Option<&str>>,
    policy: InstallPolicy,
    limits: ContentLimits,
) -> Result<InstallReceipt, InstallError> {
    install_verified_with_limits_and_commit(
        item,
        downloaded,
        root,
        relative,
        baseline_hash,
        expected_local_hash,
        policy,
        limits,
        |_| Ok(()),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn install_verified_with_limits_and_commit<F>(
    item: &ManifestItem,
    downloaded: &[u8],
    root: &Path,
    relative: &Path,
    baseline_hash: Option<&str>,
    expected_local_hash: Option<Option<&str>>,
    policy: InstallPolicy,
    limits: ContentLimits,
    commit: F,
) -> Result<InstallReceipt, InstallError>
where
    F: FnOnce(&InstallReceipt) -> Result<(), InstallError>,
{
    install_inner(
        item,
        downloaded,
        root,
        relative,
        baseline_hash,
        expected_local_hash,
        policy,
        limits,
        FailPoint::None,
        commit,
    )
}

#[cfg(test)]
pub fn recover_installs(root: &Path) -> Result<usize, InstallError> {
    recover_installs_with_committed(root, &HashSet::new())
}

pub fn recover_installs_with_committed(
    root: &Path,
    committed_operations: &HashSet<String>,
) -> Result<usize, InstallError> {
    let root = open_root(root)?;
    let control = root.join(CONTROL_DIR);
    if !path_exists_no_follow(&control)? {
        return Ok(0);
    }
    require_plain_dir(&control)?;
    ensure_beneath(&root, &control)?;
    let journals = control.join("journals");
    if !path_exists_no_follow(&journals)? {
        return Ok(0);
    }
    require_plain_dir(&journals)?;

    let mut operations: HashMap<Uuid, Journal> = HashMap::new();
    for entry in fs::read_dir(&journals)? {
        let entry = entry?;
        let metadata = entry.path().symlink_metadata()?;
        if is_link_or_reparse(&metadata) || !metadata.is_file() {
            return Err(InstallError::UnsafeFilesystem);
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            return Err(InstallError::InvalidJournal);
        };
        if !name.ends_with(".json") {
            continue;
        }
        let bytes = read_limited(&entry.path(), 64 * 1_024)?;
        let journal: Journal =
            serde_json::from_slice(&bytes).map_err(|_| InstallError::InvalidJournal)?;
        journal.validate_filename(&name)?;
        validate_managed_relative(Path::new(&journal.target), ContentLimits::default())?;
        if let Some(previous) = operations.get(&journal.operation) {
            if previous.target != journal.target
                || previous.had_target != journal.had_target
                || previous.had_baseline != journal.had_baseline
                || previous.installed_target != journal.installed_target
                || previous.installed_target_fingerprint != journal.installed_target_fingerprint
                || previous.installed_baseline != journal.installed_baseline
                || previous.installed_baseline_fingerprint != journal.installed_baseline_fingerprint
            {
                return Err(InstallError::InvalidJournal);
            }
        }
        if operations
            .get(&journal.operation)
            .is_none_or(|previous| journal.phase > previous.phase)
        {
            operations.insert(journal.operation, journal);
        }
    }

    let mut pending = operations.values().collect::<Vec<_>>();
    pending.sort_by(|left, right| {
        left.target
            .cmp(&right.target)
            .then_with(|| left.operation.to_string().cmp(&right.operation.to_string()))
    });
    if pending
        .windows(2)
        .any(|pair| pair[0].target == pair[1].target)
    {
        return Err(InstallError::Recovery);
    }
    for journal in pending {
        if committed_operations.contains(&journal.operation.to_string()) {
            finalize_committed_operation(&root, journal)?;
        } else {
            rollback_operation(&root, journal)?;
        }
        remove_operation_journals(&journals, journal.operation)?;
    }
    Ok(operations.len())
}

fn finalize_committed_operation(root: &Path, journal: &Journal) -> Result<(), InstallError> {
    if journal.phase < JournalPhase::Installed {
        return Err(InstallError::Recovery);
    }
    validate_managed_relative(Path::new(&journal.target), ContentLimits::default())?;
    let target = root.join(&journal.target);
    if journal.installed_target {
        require_safe_tree(&target)?;
    } else if path_exists_no_follow(&target)? {
        return Err(InstallError::Recovery);
    }
    let control = root.join(CONTROL_DIR);
    let baseline = control
        .join("baselines")
        .join(format!("{}.json", sha256_hex(journal.target.as_bytes())));
    if journal.installed_baseline {
        require_plain_file(&baseline)?;
    } else if path_exists_no_follow(&baseline)? {
        return Err(InstallError::Recovery);
    }
    remove_exact_if_exists(&control.join("tmp").join(journal.operation.to_string()))
}

pub fn restore_backup_with_limits_and_commit<F>(
    root: &Path,
    relative: &Path,
    selected_backup_dir: &Path,
    kind: AssetKind,
    limits: ContentLimits,
    commit: F,
) -> Result<RestoreReceipt, InstallError>
where
    F: FnOnce(&RestoreReceipt) -> Result<(), InstallError>,
{
    let context = RootContext::open(root, relative, limits)?;
    context.prepare_control()?;
    validate_backup_dir(&context, selected_backup_dir)?;
    let selected_backup = selected_backup_dir.join("payload");
    let has_selected_payload = path_exists_no_follow(&selected_backup)?;
    let selected_absence = selected_backup_dir.join("payload.absent");
    let has_selected_absence = path_exists_no_follow(&selected_absence)?;
    if has_selected_payload == has_selected_absence {
        return Err(InstallError::InvalidJournal);
    }
    if has_selected_payload {
        require_safe_tree(&selected_backup)?;
    } else {
        require_plain_file(&selected_absence)?;
        if selected_absence.metadata()?.len() != 0 {
            return Err(InstallError::InvalidJournal);
        }
    }
    let selected_baseline = selected_backup_dir.join("baseline.json");
    let has_selected_baseline = path_exists_no_follow(&selected_baseline)?;
    if has_selected_baseline {
        require_plain_file(&selected_baseline)?;
    }

    let operation = Uuid::new_v4();
    let temp_dir = context.control.join("tmp").join(operation.to_string());
    let backup_dir = context.control.join("backups").join(operation.to_string());
    fs::create_dir(&temp_dir)?;
    fs::create_dir(&backup_dir)?;
    let staged_payload = temp_dir.join("payload");
    if has_selected_payload {
        copy_safe_tree(&selected_backup, &staged_payload)?;
    }
    let staged_baseline = temp_dir.join("baseline.json");
    if has_selected_baseline {
        fs::copy(&selected_baseline, &staged_baseline)?;
    }
    let installed_target_fingerprint = if has_selected_payload {
        Some(recovery_target_fingerprint(&staged_payload)?.ok_or(InstallError::Recovery)?)
    } else {
        None
    };
    let installed_baseline_fingerprint = if has_selected_baseline {
        Some(recovery_target_fingerprint(&staged_baseline)?.ok_or(InstallError::Recovery)?)
    } else {
        None
    };

    let current_target_fingerprint = recovery_target_fingerprint(&context.target)?;
    let had_target = current_target_fingerprint.is_some();
    if !had_target {
        write_new_file(&backup_dir.join("payload.absent"), b"")?;
    }
    let baseline_path = context.baseline_path();
    let had_baseline = path_exists_no_follow(&baseline_path)?;
    let mut journal = Journal {
        version: 1,
        operation,
        phase: JournalPhase::Prepared,
        target: relative_string(&context.relative),
        had_target,
        had_baseline,
        installed_target: has_selected_payload,
        installed_target_fingerprint,
        installed_baseline: has_selected_baseline,
        installed_baseline_fingerprint,
    };
    write_journal(&context.control, &journal)?;
    let result = (|| {
        if recovery_target_fingerprint(&context.target)? != current_target_fingerprint {
            return Err(InstallError::PreviewStale);
        }
        if let Some(parent) = context.target.parent() {
            let parent_relative = parent
                .strip_prefix(&context.root)
                .map_err(|_| InstallError::UnsafePath)?;
            create_plain_dir_all(&context.root, parent_relative)?;
        }
        if had_target {
            require_safe_tree(&context.target)?;
            fs::rename(&context.target, backup_dir.join("payload"))?;
        }
        if had_baseline {
            require_plain_file(&baseline_path)?;
            fs::rename(&baseline_path, backup_dir.join("baseline.json"))?;
        }
        journal.phase = JournalPhase::BackedUp;
        write_journal(&context.control, &journal)?;
        if has_selected_payload {
            fs::rename(&staged_payload, &context.target)?;
        }
        if has_selected_baseline {
            fs::rename(&staged_baseline, &baseline_path)?;
        }
        journal.phase = JournalPhase::Installed;
        write_journal(&context.control, &journal)?;
        let baseline = read_baseline(&context)?;
        let local_hash = inspect_target(&context, kind.clone(), baseline.as_ref())?;
        commit(&RestoreReceipt {
            local_hash,
            install_path: context.target.clone(),
            displaced_backup: had_target.then(|| backup_dir.join("payload")),
            backup_dir: backup_dir.clone(),
            operation: operation.to_string(),
        })
    })();
    if let Err(error) = result {
        if let Err(rollback_error) = rollback_operation(&context.root, &journal) {
            return match rollback_error {
                error @ InstallError::RecoveryConflict(_) => Err(error),
                _ => Err(InstallError::Recovery),
            };
        }
        if remove_operation_journals(&context.control.join("journals"), operation).is_err() {
            return Err(InstallError::Recovery);
        }
        return Err(error);
    }
    if remove_exact_if_exists(&temp_dir).is_err()
        || remove_operation_journals(&context.control.join("journals"), operation).is_err()
    {
        log::warn!(
            "Team install {} committed; deferred cleanup will finish on next startup",
            operation
        );
    }
    if !had_target {
        if let Err(error) = remove_dir_if_empty(&backup_dir) {
            log::warn!(
                "Team restore {} committed; unused backup directory cleanup was deferred: {error}",
                operation
            );
        }
    }
    Ok(RestoreReceipt {
        local_hash: inspect_target(&context, kind, read_baseline(&context)?.as_ref())?,
        install_path: context.target,
        displaced_backup: had_target.then(|| backup_dir.join("payload")),
        backup_dir,
        operation: operation.to_string(),
    })
}

fn validate_backup_dir(context: &RootContext, selected_backup: &Path) -> Result<(), InstallError> {
    let backup_root = context.control.join("backups");
    if selected_backup.parent() != Some(backup_root.as_path())
        || selected_backup
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| Uuid::parse_str(name).ok())
            .is_none()
    {
        return Err(InstallError::UnsafePath);
    }
    require_plain_dir(selected_backup)?;
    ensure_beneath(&context.root, selected_backup)
}

fn copy_safe_tree(source: &Path, destination: &Path) -> Result<(), InstallError> {
    let metadata = source.symlink_metadata()?;
    if is_link_or_reparse(&metadata) {
        return Err(InstallError::UnsafeFilesystem);
    }
    if metadata.is_file() {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, destination)?;
        fs::set_permissions(destination, metadata.permissions())?;
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(InstallError::UnsafeFilesystem);
    }
    fs::create_dir(destination)?;
    fs::set_permissions(destination, metadata.permissions())?;
    let mut children = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        copy_safe_tree(&child.path(), &destination.join(child.file_name()))?;
    }
    Ok(())
}

#[derive(Debug)]
struct RootContext {
    root: PathBuf,
    relative: PathBuf,
    target: PathBuf,
    control: PathBuf,
    limits: ContentLimits,
}

impl RootContext {
    fn open(root: &Path, relative: &Path, limits: ContentLimits) -> Result<Self, InstallError> {
        let limits = limits.checked()?;
        let root = open_root(root)?;
        validate_managed_relative(relative, limits)?;
        let target = root.join(relative);
        validate_existing_ancestors(&root, relative)?;
        validate_existing_ancestors(&root, &Path::new(CONTROL_DIR).join("baselines"))?;
        Ok(Self {
            control: root.join(CONTROL_DIR),
            root,
            relative: relative.to_path_buf(),
            target,
            limits,
        })
    }

    fn prepare_control(&self) -> Result<(), InstallError> {
        create_plain_dir_all(&self.root, Path::new(CONTROL_DIR))?;
        for directory in ["tmp", "backups", "journals", "baselines"] {
            create_plain_dir_all(&self.root, &Path::new(CONTROL_DIR).join(directory))?;
        }
        Ok(())
    }

    fn baseline_path(&self) -> PathBuf {
        self.control.join("baselines").join(format!(
            "{}.json",
            sha256_hex(relative_string(&self.relative).as_bytes())
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BaselineRecord {
    version: u32,
    target: String,
    content_hash: String,
    files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum JournalPhase {
    Prepared,
    BackedUp,
    Installed,
}

impl JournalPhase {
    fn sequence(self) -> u8 {
        match self {
            Self::Prepared => 0,
            Self::BackedUp => 1,
            Self::Installed => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u32,
    operation: Uuid,
    phase: JournalPhase,
    target: String,
    had_target: bool,
    had_baseline: bool,
    #[serde(default = "default_true")]
    installed_target: bool,
    #[serde(default)]
    installed_target_fingerprint: Option<String>,
    #[serde(default = "default_true")]
    installed_baseline: bool,
    #[serde(default)]
    installed_baseline_fingerprint: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Journal {
    fn validate_filename(&self, filename: &str) -> Result<(), InstallError> {
        if self.version != 1
            || filename != format!("{}.{}.json", self.operation, self.phase.sequence())
            || (!self.installed_target && self.installed_target_fingerprint.is_some())
            || (!self.installed_baseline && self.installed_baseline_fingerprint.is_some())
            || self
                .installed_target_fingerprint
                .as_deref()
                .is_some_and(|value| !valid_recovery_fingerprint(value))
            || self
                .installed_baseline_fingerprint
                .as_deref()
                .is_some_and(|value| !valid_recovery_fingerprint(value))
        {
            return Err(InstallError::InvalidJournal);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailPoint {
    None,
    #[cfg(test)]
    ConcurrentEditBeforeReplace,
    #[cfg(test)]
    AfterBackup,
    #[cfg(test)]
    AfterInstall,
}

#[allow(clippy::too_many_arguments)]
fn install_inner<F>(
    item: &ManifestItem,
    downloaded: &[u8],
    root: &Path,
    relative: &Path,
    baseline_hash: Option<&str>,
    expected_local_hash: Option<Option<&str>>,
    policy: InstallPolicy,
    limits: ContentLimits,
    fail: FailPoint,
    commit: F,
) -> Result<InstallReceipt, InstallError>
where
    F: FnOnce(&InstallReceipt) -> Result<(), InstallError>,
{
    #[cfg(not(test))]
    let _ = fail;
    let verified = super::content::verify_download(item, downloaded, limits)?;
    let context = RootContext::open(root, relative, limits)?;
    let baseline = read_baseline(&context)?;
    let local_hash = inspect_target(&context, item.kind.clone(), baseline.as_ref())?;
    if let Some(expected) = expected_local_hash {
        if local_hash.as_deref() != expected {
            return Err(InstallError::PreviewStale);
        }
    }
    let drift = classify_drift(baseline_hash, local_hash.as_deref(), &verified.content_hash);
    if drift == DriftStatus::Same {
        return Ok(InstallReceipt {
            local_hash: verified.content_hash,
            install_path: context.target,
            backup: None,
            backup_dir: None,
            operation: None,
        });
    }
    if matches!(
        drift,
        DriftStatus::LocalModified | DriftStatus::BothModified
    ) && !policy.overwrite_local
    {
        return Err(InstallError::Conflict(drift));
    }

    context.prepare_control()?;
    validate_existing_ancestors(&context.root, &context.relative)?;
    let operation = Uuid::new_v4();
    let temp_dir = context.control.join("tmp").join(operation.to_string());
    let backup_dir = context.control.join("backups").join(operation.to_string());
    fs::create_dir(&temp_dir)?;
    if let Err(error) = fs::create_dir(&backup_dir) {
        let _ = remove_exact_if_exists(&temp_dir);
        return Err(error.into());
    }
    let staged_payload = temp_dir.join("payload");
    let staged_baseline = temp_dir.join("baseline.json");
    let staging_result = (|| {
        materialize(&staged_payload, item.kind.clone(), &verified)?;
        write_new_file(
            &staged_baseline,
            &serde_json::to_vec(&BaselineRecord {
                version: 1,
                target: relative_string(&context.relative),
                content_hash: verified.content_hash.clone(),
                files: verified
                    .files
                    .iter()
                    .map(|file| file.entry.clone())
                    .collect(),
            })
            .map_err(|_| InstallError::InvalidJournal)?,
        )?;
        let installed_target_fingerprint =
            recovery_target_fingerprint(&staged_payload)?.ok_or(InstallError::Recovery)?;
        let installed_baseline_fingerprint =
            recovery_target_fingerprint(&staged_baseline)?.ok_or(InstallError::Recovery)?;
        Ok::<_, InstallError>((installed_target_fingerprint, installed_baseline_fingerprint))
    })();
    let (installed_target_fingerprint, installed_baseline_fingerprint) = match staging_result {
        Ok(fingerprints) => fingerprints,
        Err(error) => {
            if remove_exact_if_exists(&temp_dir).is_err()
                || remove_exact_if_exists(&backup_dir).is_err()
            {
                return Err(InstallError::Recovery);
            }
            return Err(error);
        }
    };

    let had_target = path_exists_no_follow(&context.target)?;
    if !had_target {
        write_new_file(&backup_dir.join("payload.absent"), b"")?;
    }
    let baseline_path = context.baseline_path();
    let had_baseline = path_exists_no_follow(&baseline_path)?;
    let mut journal = Journal {
        version: 1,
        operation,
        phase: JournalPhase::Prepared,
        target: relative_string(&context.relative),
        had_target,
        had_baseline,
        installed_target: true,
        installed_target_fingerprint: Some(installed_target_fingerprint),
        installed_baseline: true,
        installed_baseline_fingerprint: Some(installed_baseline_fingerprint),
    };
    if let Err(error) = write_journal(&context.control, &journal) {
        if remove_exact_if_exists(&temp_dir).is_err()
            || remove_exact_if_exists(&backup_dir).is_err()
        {
            return Err(InstallError::Recovery);
        }
        return Err(error);
    }

    let install_result = (|| {
        #[cfg(test)]
        if matches!(fail, FailPoint::ConcurrentEditBeforeReplace) {
            fs::write(&context.target, b"concurrent local edit")?;
        }
        if let Some(expected) = expected_local_hash {
            if inspect_target(&context, item.kind.clone(), baseline.as_ref())?.as_deref()
                != expected
            {
                return Err(InstallError::PreviewStale);
            }
        }
        if let Some(parent) = context.target.parent() {
            let parent_relative = parent
                .strip_prefix(&context.root)
                .map_err(|_| InstallError::UnsafePath)?;
            create_plain_dir_all(&context.root, parent_relative)?;
        }
        if had_target {
            require_safe_tree(&context.target)?;
            fs::rename(&context.target, backup_dir.join("payload"))?;
        }
        if had_baseline {
            require_plain_file(&baseline_path)?;
            fs::rename(&baseline_path, backup_dir.join("baseline.json"))?;
        }
        journal.phase = JournalPhase::BackedUp;
        write_journal(&context.control, &journal)?;
        #[cfg(test)]
        if matches!(fail, FailPoint::AfterBackup) {
            return Err(InstallError::Io(io::Error::other(
                "injected failure after backup",
            )));
        }

        fs::rename(&staged_payload, &context.target)?;
        fs::rename(&staged_baseline, &baseline_path)?;
        journal.phase = JournalPhase::Installed;
        write_journal(&context.control, &journal)?;
        #[cfg(test)]
        if matches!(fail, FailPoint::AfterInstall) {
            return Err(InstallError::Io(io::Error::other(
                "injected failure after install",
            )));
        }
        commit(&InstallReceipt {
            local_hash: verified.content_hash.clone(),
            install_path: context.target.clone(),
            backup: had_target.then(|| backup_dir.join("payload")),
            backup_dir: Some(backup_dir.clone()),
            operation: Some(operation.to_string()),
        })?;
        Ok(())
    })();

    if let Err(error) = install_result {
        if let Err(rollback_error) = rollback_operation(&context.root, &journal) {
            return match rollback_error {
                error @ InstallError::RecoveryConflict(_) => Err(error),
                _ => Err(InstallError::Recovery),
            };
        }
        if remove_operation_journals(&context.control.join("journals"), operation).is_err() {
            return Err(InstallError::Recovery);
        }
        return Err(error);
    }

    if remove_exact_if_exists(&temp_dir).is_err()
        || remove_operation_journals(&context.control.join("journals"), operation).is_err()
    {
        log::warn!(
            "Team install {} committed; deferred cleanup will finish on next startup",
            operation
        );
    }
    if !had_target && !had_baseline {
        if let Err(error) = remove_dir_if_empty(&backup_dir) {
            log::warn!(
                "Team install {} committed; unused backup directory cleanup was deferred: {error}",
                operation
            );
        }
    }
    Ok(InstallReceipt {
        local_hash: verified.content_hash,
        install_path: context.target,
        backup: had_target.then(|| backup_dir.join("payload")),
        backup_dir: Some(backup_dir),
        operation: Some(operation.to_string()),
    })
}

fn remove_dir_if_empty(path: &Path) -> Result<(), InstallError> {
    if !path_exists_no_follow(path)? {
        return Ok(());
    }
    require_plain_dir(path)?;
    if fs::read_dir(path)?.next().is_none() {
        fs::remove_dir(path)?;
    }
    Ok(())
}

fn inspect_target(
    context: &RootContext,
    kind: AssetKind,
    baseline: Option<&BaselineRecord>,
) -> Result<Option<String>, InstallError> {
    if !path_exists_no_follow(&context.target)? {
        return Ok(None);
    }
    require_safe_tree(&context.target)?;
    let metadata = context.target.symlink_metadata()?;
    if kind == AssetKind::Skill {
        if !metadata.is_dir() {
            return Ok(Some(sha256_hex(KIND_MISMATCH_HASH_INPUT)));
        }
        return Ok(Some(hash_local_skill(
            &context.target,
            context.limits,
            baseline.map(|record| record.files.as_slice()),
        )?));
    }
    if !metadata.is_file() {
        return Ok(Some(sha256_hex(KIND_MISMATCH_HASH_INPUT)));
    }
    let body = read_limited(&context.target, context.limits.max_text_bytes as usize)?;
    Ok(Some(sha256_hex(&normalize_text(&body, context.limits)?)))
}

fn read_baseline(context: &RootContext) -> Result<Option<BaselineRecord>, InstallError> {
    let path = context.baseline_path();
    if !path_exists_no_follow(&path)? {
        return Ok(None);
    }
    require_plain_file(&path)?;
    let bytes = read_limited(&path, 4 << 20)?;
    let record: BaselineRecord = match serde_json::from_slice(&bytes) {
        Ok(record) => record,
        Err(_) => return Ok(None),
    };
    if record.version != 1 || record.target != relative_string(&context.relative) {
        return Ok(None);
    }
    Ok(Some(record))
}

fn hash_local_skill(
    root: &Path,
    limits: ContentLimits,
    baseline_files: Option<&[FileEntry]>,
) -> Result<String, InstallError> {
    let modes = baseline_files
        .unwrap_or_default()
        .iter()
        .map(|entry| (entry.path.as_str(), entry.executable))
        .collect::<HashMap<_, _>>();
    let mut entries = Vec::new();
    let mut nodes: HashMap<String, PathNode> = HashMap::new();
    let mut unpacked = 0u64;
    scan_skill_dir(
        root,
        root,
        limits,
        &modes,
        &mut entries,
        &mut nodes,
        &mut unpacked,
    )?;
    if !entries.iter().any(|entry| entry.path == "SKILL.md") {
        return Err(ContentError::Invalid("root SKILL.md is required").into());
    }
    entries.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    Ok(sha256_hex(&canonical_manifest(&entries)?))
}

#[allow(clippy::too_many_arguments)]
fn scan_skill_dir<'a>(
    root: &Path,
    directory: &Path,
    limits: ContentLimits,
    modes: &HashMap<&'a str, bool>,
    entries: &mut Vec<FileEntry>,
    nodes: &mut HashMap<String, PathNode>,
    unpacked: &mut u64,
) -> Result<(), InstallError> {
    let mut children = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let path = child.path();
        let metadata = path.symlink_metadata()?;
        if is_link_or_reparse(&metadata) {
            return Err(InstallError::UnsafeFilesystem);
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| InstallError::UnsafePath)?;
        let name = slash_path(relative)?;
        let portable = portable_path(&name, metadata.is_dir(), limits)?;
        reserve_path(nodes, &portable, metadata.is_dir())?;
        if metadata.is_dir() {
            scan_skill_dir(root, &path, limits, modes, entries, nodes, unpacked)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(InstallError::UnsafeFilesystem);
        }
        if entries.len() >= limits.max_files {
            return Err(ContentError::TooLarge("file count").into());
        }
        let raw_size = metadata.len();
        if raw_size > limits.max_unpacked_bytes.saturating_sub(*unpacked) {
            return Err(ContentError::TooLarge("unpacked file bytes").into());
        }
        if is_text_path(&portable) && raw_size > limits.max_text_bytes {
            return Err(ContentError::TooLarge("text file bytes").into());
        }
        let mut body = read_limited(&path, raw_size as usize)?;
        *unpacked += raw_size;
        if is_text_path(&portable) {
            body = normalize_text(&body, limits)?;
        }
        #[cfg(unix)]
        let executable = metadata_executable(&metadata);
        #[cfg(not(unix))]
        let executable = modes
            .get(portable.as_str())
            .copied()
            .unwrap_or_else(|| metadata_executable(&metadata));
        entries.push(FileEntry {
            path: portable,
            sha256: sha256_hex(&body),
            size: body.len() as u64,
            executable,
        });
    }
    Ok(())
}

fn materialize(
    path: &Path,
    kind: AssetKind,
    verified: &VerifiedContent,
) -> Result<(), InstallError> {
    if kind != AssetKind::Skill {
        write_new_file(path, &verified.body)?;
        return Ok(());
    }
    fs::create_dir(path)?;
    for file in &verified.files {
        let relative = PathBuf::from(file.entry.path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let destination = path.join(&relative);
        if let Some(parent) = destination.parent() {
            let parent_relative = parent
                .strip_prefix(path)
                .map_err(|_| InstallError::UnsafePath)?;
            create_plain_dir_all(path, parent_relative)?;
        }
        write_new_file(&destination, &file.body)?;
        set_executable(&destination, file.entry.executable)?;
    }
    Ok(())
}

fn open_root(root: &Path) -> Result<PathBuf, InstallError> {
    if !root.is_absolute() || !path_exists_no_follow(root)? {
        return Err(InstallError::InvalidRoot);
    }
    require_plain_dir(root)?;
    root.canonicalize().map_err(InstallError::Io)
}

fn validate_managed_relative(relative: &Path, limits: ContentLimits) -> Result<(), InstallError> {
    if relative.as_os_str().is_empty() || relative.is_absolute() {
        return Err(InstallError::UnsafePath);
    }
    let mut components = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(value) => {
                let value = value.to_str().ok_or(InstallError::UnsafePath)?;
                if components.is_empty() && value.eq_ignore_ascii_case(CONTROL_DIR) {
                    return Err(InstallError::UnsafePath);
                }
                components.push(value);
            }
            _ => return Err(InstallError::UnsafePath),
        }
    }
    portable_path(&components.join("/"), false, limits)?;
    Ok(())
}

fn validate_existing_ancestors(root: &Path, relative: &Path) -> Result<(), InstallError> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(InstallError::UnsafePath);
        };
        current.push(component);
        if !path_exists_no_follow(&current)? {
            continue;
        }
        let metadata = current.symlink_metadata()?;
        if is_link_or_reparse(&metadata) {
            return Err(InstallError::UnsafeFilesystem);
        }
    }
    Ok(())
}

fn ensure_beneath(root: &Path, path: &Path) -> Result<(), InstallError> {
    let canonical = path.canonicalize()?;
    if canonical == root || !canonical.starts_with(root) {
        return Err(InstallError::UnsafeFilesystem);
    }
    Ok(())
}

fn create_plain_dir_all(root: &Path, relative: &Path) -> Result<(), InstallError> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(InstallError::UnsafePath);
        };
        current.push(component);
        if path_exists_no_follow(&current)? {
            require_plain_dir(&current)?;
        } else {
            fs::create_dir(&current)?;
            require_plain_dir(&current)?;
        }
        ensure_beneath(root, &current)?;
    }
    Ok(())
}

fn require_plain_dir(path: &Path) -> Result<(), InstallError> {
    let metadata = path
        .symlink_metadata()
        .map_err(|_| InstallError::InvalidRoot)?;
    if !metadata.is_dir() || is_link_or_reparse(&metadata) {
        return Err(InstallError::UnsafeFilesystem);
    }
    Ok(())
}

fn require_plain_file(path: &Path) -> Result<(), InstallError> {
    let metadata = path.symlink_metadata()?;
    if !metadata.is_file() || is_link_or_reparse(&metadata) {
        return Err(InstallError::UnsafeFilesystem);
    }
    Ok(())
}

fn require_safe_tree(path: &Path) -> Result<(), InstallError> {
    let metadata = path.symlink_metadata()?;
    if is_link_or_reparse(&metadata) {
        return Err(InstallError::UnsafeFilesystem);
    }
    if metadata.is_dir() {
        for child in fs::read_dir(path)? {
            require_safe_tree(&child?.path())?;
        }
    } else if !metadata.is_file() {
        return Err(InstallError::UnsafeFilesystem);
    }
    Ok(())
}

fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

fn path_exists_no_follow(path: &Path) -> io::Result<bool> {
    match path.symlink_metadata() {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn metadata_executable(metadata: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return metadata.permissions().mode() & 0o111 != 0;
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        false
    }
}

fn set_executable(path: &Path, executable: bool) -> Result<(), InstallError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
        )?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, executable);
    }
    Ok(())
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), InstallError> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn read_limited(path: &Path, limit: usize) -> Result<Vec<u8>, InstallError> {
    let mut file = File::open(path)?;
    let mut body = Vec::new();
    Read::by_ref(&mut file)
        .take(limit.saturating_add(1) as u64)
        .read_to_end(&mut body)?;
    if body.len() > limit {
        return Err(ContentError::TooLarge("local file bytes").into());
    }
    Ok(body)
}

fn write_journal(control: &Path, journal: &Journal) -> Result<(), InstallError> {
    let path = control.join("journals").join(format!(
        "{}.{}.json",
        journal.operation,
        journal.phase.sequence()
    ));
    let body = serde_json::to_vec(journal).map_err(|_| InstallError::InvalidJournal)?;
    write_new_file(&path, &body)
}

#[derive(Default)]
struct RecoveryFingerprintBudget {
    entries: usize,
    files: usize,
    bytes: u64,
}

fn valid_recovery_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn recovery_target_fingerprint(path: &Path) -> Result<Option<String>, InstallError> {
    if !path_exists_no_follow(path)? {
        return Ok(None);
    }
    let limits = ContentLimits::default();
    let max_entries = limits
        .max_files
        .checked_mul(limits.max_depth + 1)
        .ok_or(ContentError::InvalidLimits)?;
    let mut budget = RecoveryFingerprintBudget::default();
    let mut hasher = Sha256::new();
    hasher.update(b"nexusops-team-recovery-target-v1\0");
    hash_recovery_node(path, &mut hasher, &mut budget, limits, max_entries, 0, 0)?;
    Ok(Some(format!("{:x}", hasher.finalize())))
}

#[allow(clippy::too_many_arguments)]
fn hash_recovery_node(
    path: &Path,
    hasher: &mut Sha256,
    budget: &mut RecoveryFingerprintBudget,
    limits: ContentLimits,
    max_entries: usize,
    depth: usize,
    relative_bytes: usize,
) -> Result<(), InstallError> {
    let metadata = path.symlink_metadata()?;
    if is_link_or_reparse(&metadata) {
        return Err(InstallError::UnsafeFilesystem);
    }
    if depth > limits.max_depth || relative_bytes > limits.max_path_bytes {
        return Err(ContentError::TooLarge("recovery path").into());
    }

    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o777
    };
    #[cfg(not(unix))]
    let mode = u32::from(metadata.permissions().readonly());
    hasher.update(mode.to_le_bytes());

    if metadata.is_file() {
        budget.files = budget
            .files
            .checked_add(1)
            .ok_or(ContentError::InvalidLimits)?;
        if budget.files > limits.max_files
            || metadata.len() > limits.max_unpacked_bytes.saturating_sub(budget.bytes)
        {
            return Err(ContentError::TooLarge("recovery file bytes").into());
        }
        budget.bytes += metadata.len();
        hasher.update(b"file\0");
        hasher.update(metadata.len().to_le_bytes());
        let mut file = File::open(path)?;
        let mut read = 0u64;
        let mut chunk = [0u8; 32 * 1024];
        loop {
            let count = file.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            read = read
                .checked_add(count as u64)
                .ok_or(ContentError::InvalidLimits)?;
            if read > metadata.len() {
                return Err(InstallError::RecoveryConflict(
                    "the managed target changed while recovery inspected it; current target and backup were preserved"
                        .into(),
                ));
            }
            hasher.update(&chunk[..count]);
        }
        if read != metadata.len() {
            return Err(InstallError::RecoveryConflict(
                "the managed target changed while recovery inspected it; current target and backup were preserved"
                    .into(),
            ));
        }
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(InstallError::UnsafeFilesystem);
    }

    hasher.update(b"dir\0");
    let mut children = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        budget.entries = budget
            .entries
            .checked_add(1)
            .ok_or(ContentError::InvalidLimits)?;
        if budget.entries > max_entries {
            return Err(ContentError::TooLarge("recovery entry count").into());
        }
        let name = child.file_name();
        let name_bytes = recovery_os_string_bytes(&name);
        hasher.update((name_bytes.len() as u64).to_le_bytes());
        hasher.update(&name_bytes);
        let next_relative_bytes = relative_bytes
            .checked_add(usize::from(relative_bytes != 0))
            .and_then(|value| value.checked_add(name_bytes.len()))
            .ok_or(ContentError::InvalidLimits)?;
        hash_recovery_node(
            &child.path(),
            hasher,
            budget,
            limits,
            max_entries,
            depth + 1,
            next_relative_bytes,
        )?;
    }
    hasher.update(b"end\0");
    Ok(())
}

fn recovery_os_string_bytes(value: &OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        value.as_bytes().to_vec()
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        value.encode_wide().flat_map(u16::to_le_bytes).collect()
    }
    #[cfg(not(any(unix, windows)))]
    {
        value.to_string_lossy().as_bytes().to_vec()
    }
}

fn ensure_expected_rollback_state(
    path: &Path,
    should_remove: bool,
    installed: bool,
    expected_fingerprint: Option<&str>,
    label: &str,
) -> Result<(), InstallError> {
    if !should_remove {
        return Ok(());
    }
    let actual = recovery_target_fingerprint(path);
    if installed
        && expected_fingerprint.is_some()
        && actual.as_ref().ok().and_then(|value| value.as_deref()) == expected_fingerprint
    {
        return Ok(());
    }
    Err(InstallError::RecoveryConflict(format!(
        "the managed {label} changed after the interrupted operation; current target and backup were preserved"
    )))
}

fn rollback_operation(root: &Path, journal: &Journal) -> Result<(), InstallError> {
    validate_managed_relative(Path::new(&journal.target), ContentLimits::default())?;
    let target = root.join(Path::new(&journal.target));
    let control = root.join(CONTROL_DIR);
    let temp_dir = control.join("tmp").join(journal.operation.to_string());
    let backup_dir = control.join("backups").join(journal.operation.to_string());
    let backup_payload = backup_dir.join("payload");
    let baseline_path = control
        .join("baselines")
        .join(format!("{}.json", sha256_hex(journal.target.as_bytes())));
    let backup_baseline = backup_dir.join("baseline.json");

    let target_exists = path_exists_no_follow(&target)?;
    let backup_payload_exists = path_exists_no_follow(&backup_payload)?;
    let baseline_exists = path_exists_no_follow(&baseline_path)?;
    let backup_baseline_exists = path_exists_no_follow(&backup_baseline)?;
    if journal.phase == JournalPhase::Prepared
        && !backup_payload_exists
        && !backup_baseline_exists
        && (!journal.had_target || target_exists)
        && (!journal.had_baseline || baseline_exists)
    {
        remove_exact_if_exists(&temp_dir)?;
        remove_exact_if_exists(&backup_dir)?;
        return Ok(());
    }
    if backup_payload_exists {
        require_safe_tree(&backup_payload)?;
    }
    if backup_baseline_exists {
        require_plain_file(&backup_baseline)?;
    }
    ensure_expected_rollback_state(
        &baseline_path,
        baseline_exists && (backup_baseline_exists || !journal.had_baseline),
        journal.installed_baseline,
        journal.installed_baseline_fingerprint.as_deref(),
        "baseline",
    )?;
    ensure_expected_rollback_state(
        &target,
        target_exists && (backup_payload_exists || !journal.had_target),
        journal.installed_target,
        journal.installed_target_fingerprint.as_deref(),
        "target",
    )?;

    if target_exists && (backup_payload_exists || !journal.had_target) {
        require_safe_tree(&target)?;
        remove_exact_if_exists(&target)?;
    }
    if backup_payload_exists {
        if let Some(parent) = target.parent() {
            let parent_relative = parent
                .strip_prefix(root)
                .map_err(|_| InstallError::UnsafePath)?;
            create_plain_dir_all(root, parent_relative)?;
        }
        fs::rename(&backup_payload, &target)?;
    } else if journal.had_target && !path_exists_no_follow(&target)? {
        return Err(InstallError::Recovery);
    }

    if baseline_exists && (backup_baseline_exists || !journal.had_baseline) {
        require_plain_file(&baseline_path)?;
        fs::remove_file(&baseline_path)?;
    }
    if backup_baseline_exists {
        fs::rename(&backup_baseline, &baseline_path)?;
    } else if journal.had_baseline && !path_exists_no_follow(&baseline_path)? {
        return Err(InstallError::Recovery);
    }

    remove_exact_if_exists(&temp_dir)?;
    remove_exact_if_exists(&backup_dir)?;
    Ok(())
}

fn remove_operation_journals(journals: &Path, operation: Uuid) -> Result<(), InstallError> {
    for phase in [
        JournalPhase::Prepared,
        JournalPhase::BackedUp,
        JournalPhase::Installed,
    ] {
        let path = journals.join(format!("{}.{}.json", operation, phase.sequence()));
        if path_exists_no_follow(&path)? {
            require_plain_file(&path)?;
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn remove_exact_if_exists(path: &Path) -> Result<(), InstallError> {
    if !path_exists_no_follow(path)? {
        return Ok(());
    }
    let metadata = path.symlink_metadata()?;
    if is_link_or_reparse(&metadata) {
        return Err(InstallError::UnsafeFilesystem);
    }
    if metadata.is_dir() {
        require_safe_tree(path)?;
        fs::remove_dir_all(path)?;
    } else if metadata.is_file() {
        fs::remove_file(path)?;
    } else {
        return Err(InstallError::UnsafeFilesystem);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn leave_interrupted_backup_for_test(
    root: &Path,
    relative: &Path,
) -> Result<(), InstallError> {
    let context = RootContext::open(root, relative, ContentLimits::default())?;
    context.prepare_control()?;
    let operation = Uuid::new_v4();
    let backup_dir = context.control.join("backups").join(operation.to_string());
    fs::create_dir(&backup_dir)?;
    let baseline_path = context.baseline_path();
    let had_target = context.target.exists();
    let had_baseline = baseline_path.exists();
    if had_target {
        fs::rename(&context.target, backup_dir.join("payload"))?;
    }
    if had_baseline {
        fs::rename(&baseline_path, backup_dir.join("baseline.json"))?;
    }
    write_journal(
        &context.control,
        &Journal {
            version: 1,
            operation,
            phase: JournalPhase::BackedUp,
            target: relative_string(relative),
            had_target,
            had_baseline,
            installed_target: true,
            installed_target_fingerprint: None,
            installed_baseline: true,
            installed_baseline_fingerprint: None,
        },
    )
}

fn relative_string(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn slash_path(path: &Path) -> Result<String, InstallError> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(value) = component else {
            return Err(InstallError::UnsafePath);
        };
        parts.push(value.to_str().ok_or(InstallError::UnsafePath)?);
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use std::{io::Write, path::Path};

    use flate2::{write::GzEncoder, Compression};
    use tar::{Builder, EntryType, Header};

    use super::*;

    fn text_item(revision: i64, body: &[u8]) -> ManifestItem {
        let normalized = normalize_text(body, ContentLimits::default()).unwrap();
        ManifestItem {
            asset_id: 7,
            kind: AssetKind::Rule,
            slug: "managed".into(),
            name: "Managed".into(),
            revision,
            content_hash: sha256_hex(&normalized),
            archive_sha256: None,
            download_url: format!("/api/v1/assets/7/revisions/{revision}/download"),
            content_type: "text/plain; charset=utf-8".into(),
            byte_size: body.len() as u64,
            files: Vec::new(),
        }
    }

    fn skill_item(body: &[u8]) -> (ManifestItem, Vec<u8>) {
        let mut plain = Vec::new();
        {
            let mut builder = Builder::new(&mut plain);
            let mut header = Header::new_gnu();
            header.set_entry_type(EntryType::Regular);
            header.set_mode(0o644);
            header.set_size(body.len() as u64);
            header.set_cksum();
            builder.append_data(&mut header, "SKILL.md", body).unwrap();
            builder.finish().unwrap();
        }
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(&plain).unwrap();
        let archive = gzip.finish().unwrap();
        let normalized = normalize_text(body, ContentLimits::default()).unwrap();
        let files = vec![FileEntry {
            path: "SKILL.md".into(),
            sha256: sha256_hex(&normalized),
            size: normalized.len() as u64,
            executable: false,
        }];
        let item = ManifestItem {
            asset_id: 8,
            kind: AssetKind::Skill,
            slug: "managed-skill".into(),
            name: "Managed skill".into(),
            revision: 1,
            content_hash: sha256_hex(&canonical_manifest(&files).unwrap()),
            archive_sha256: Some(sha256_hex(&archive)),
            download_url: "/api/v1/assets/8/revisions/1/download".into(),
            content_type: "application/gzip".into(),
            byte_size: archive.len() as u64,
            files,
        };
        (item, archive)
    }

    #[test]
    fn drift_matrix_is_strict() {
        assert_eq!(classify_drift(None, None, "A"), DriftStatus::NotInstalled);
        assert_eq!(
            classify_drift(None, Some("personal"), "A"),
            DriftStatus::LocalModified
        );
        assert_eq!(classify_drift(Some("A"), Some("A"), "A"), DriftStatus::Same);
        assert_eq!(
            classify_drift(Some("A"), Some("A"), "B"),
            DriftStatus::RemoteUpdate
        );
        assert_eq!(
            classify_drift(Some("A"), Some("local"), "A"),
            DriftStatus::LocalModified
        );
        assert_eq!(
            classify_drift(Some("A"), Some("local"), "B"),
            DriftStatus::BothModified
        );
    }

    #[test]
    fn installs_a_b_and_remote_rollback_a_without_rewriting_same_content() {
        let temp = tempfile::tempdir().unwrap();
        let target = Path::new("rules/managed.md");
        let a = b"A\r\n";
        let b = b"B\r\n";
        let item_a = text_item(1, a);
        let first = install_verified(
            &item_a,
            a,
            temp.path(),
            target,
            None,
            InstallPolicy::default(),
        )
        .unwrap();
        assert_eq!(fs::read(&first.install_path).unwrap(), b"A\n");
        let backups_before = fs::read_dir(temp.path().join(CONTROL_DIR).join("backups"))
            .unwrap()
            .count();
        let same = install_verified(
            &item_a,
            a,
            temp.path(),
            target,
            Some(&first.local_hash),
            InstallPolicy::default(),
        )
        .unwrap();
        assert!(same.backup.is_none());
        assert_eq!(
            fs::read_dir(temp.path().join(CONTROL_DIR).join("backups"))
                .unwrap()
                .count(),
            backups_before
        );

        let item_b = text_item(2, b);
        let second = install_verified(
            &item_b,
            b,
            temp.path(),
            target,
            Some(&first.local_hash),
            InstallPolicy::default(),
        )
        .unwrap();
        assert_eq!(fs::read(&second.install_path).unwrap(), b"B\n");
        let rolled_back = install_verified(
            &item_a,
            a,
            temp.path(),
            target,
            Some(&second.local_hash),
            InstallPolicy::default(),
        )
        .unwrap();
        assert_eq!(fs::read(&rolled_back.install_path).unwrap(), b"A\n");
        assert!(second.backup.unwrap().exists());
        assert!(rolled_back.backup.unwrap().exists());
    }

    #[test]
    fn commit_failure_rolls_back_payload_and_baseline() {
        let temp = tempfile::tempdir().unwrap();
        let target = Path::new("rules/managed.md");
        let item_a = text_item(1, b"A");
        let first = install_verified(
            &item_a,
            b"A",
            temp.path(),
            target,
            None,
            InstallPolicy::default(),
        )
        .unwrap();
        let item_b = text_item(2, b"B");
        let error = install_verified_with_limits_and_commit(
            &item_b,
            b"B",
            temp.path(),
            target,
            Some(&first.local_hash),
            Some(Some(&first.local_hash)),
            InstallPolicy::default(),
            ContentLimits::default(),
            |_| Err(InstallError::Commit("synthetic Team state failure".into())),
        )
        .unwrap_err();
        assert!(matches!(error, InstallError::Commit(_)));
        assert_eq!(fs::read(temp.path().join(target)).unwrap(), b"A");
        assert_eq!(
            inspect_local(&item_a, temp.path(), target).unwrap(),
            Some(first.local_hash)
        );
        assert_eq!(
            fs::read_dir(temp.path().join(CONTROL_DIR).join("journals"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn committed_journal_is_finalized_instead_of_rolled_back() {
        let temp = tempfile::tempdir().unwrap();
        let target = Path::new("rules/managed.md");
        let item = text_item(2, b"B");
        let receipt = install_verified(
            &item,
            b"B",
            temp.path(),
            target,
            None,
            InstallPolicy::default(),
        )
        .unwrap();
        let context = RootContext::open(temp.path(), target, ContentLimits::default()).unwrap();
        let operation = Uuid::new_v4();
        fs::create_dir(context.control.join("tmp").join(operation.to_string())).unwrap();
        let journal = Journal {
            version: 1,
            operation,
            phase: JournalPhase::Installed,
            target: relative_string(target),
            had_target: true,
            had_baseline: true,
            installed_target: true,
            installed_target_fingerprint: None,
            installed_baseline: true,
            installed_baseline_fingerprint: None,
        };
        write_journal(&context.control, &journal).unwrap();
        assert_eq!(
            recover_installs_with_committed(temp.path(), &HashSet::from([operation.to_string()]))
                .unwrap(),
            1
        );
        assert_eq!(fs::read(temp.path().join(target)).unwrap(), b"B");
        assert_eq!(
            inspect_local(&item, temp.path(), target).unwrap(),
            Some(receipt.local_hash)
        );
    }

    #[test]
    fn restore_backup_restores_its_matching_baseline_and_creates_undo() {
        let temp = tempfile::tempdir().unwrap();
        let target = Path::new("rules/managed.md");
        let item_a = text_item(1, b"A");
        let first = install_verified(
            &item_a,
            b"A",
            temp.path(),
            target,
            None,
            InstallPolicy::default(),
        )
        .unwrap();
        let item_b = text_item(2, b"B");
        let second = install_verified(
            &item_b,
            b"B",
            temp.path(),
            target,
            Some(&first.local_hash),
            InstallPolicy::default(),
        )
        .unwrap();
        let restored = restore_backup_with_limits_and_commit(
            temp.path(),
            target,
            second.backup.as_ref().unwrap().parent().unwrap(),
            AssetKind::Rule,
            ContentLimits::default(),
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(
            restored.local_hash.as_deref(),
            Some(first.local_hash.as_str())
        );
        assert_eq!(fs::read(&restored.install_path).unwrap(), b"A");
        assert_eq!(fs::read(restored.displaced_backup.unwrap()).unwrap(), b"B");
        assert_eq!(
            inspect_local(&item_a, temp.path(), target).unwrap(),
            Some(first.local_hash)
        );
    }

    #[test]
    fn restore_round_trip_preserves_a_missing_target_as_an_undo_point() {
        let temp = tempfile::tempdir().unwrap();
        let target = Path::new("rules/managed.md");
        let item_a = text_item(1, b"A");
        let first = install_verified(
            &item_a,
            b"A",
            temp.path(),
            target,
            None,
            InstallPolicy::default(),
        )
        .unwrap();
        let item_b = text_item(2, b"B");
        let second = install_verified(
            &item_b,
            b"B",
            temp.path(),
            target,
            Some(&first.local_hash),
            InstallPolicy::default(),
        )
        .unwrap();
        fs::remove_file(temp.path().join(target)).unwrap();
        let restored = restore_backup_with_limits_and_commit(
            temp.path(),
            target,
            second.backup.as_ref().unwrap().parent().unwrap(),
            AssetKind::Rule,
            ContentLimits::default(),
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(fs::read(temp.path().join(target)).unwrap(), b"A");
        assert!(restored.displaced_backup.is_none());
        assert!(restored.backup_dir.join("payload.absent").exists());

        let undone = restore_backup_with_limits_and_commit(
            temp.path(),
            target,
            &restored.backup_dir,
            AssetKind::Rule,
            ContentLimits::default(),
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(undone.local_hash, None);
        assert!(!temp.path().join(target).exists());
        assert_eq!(fs::read(undone.displaced_backup.unwrap()).unwrap(), b"A");
    }

    #[test]
    fn missing_backup_payload_never_means_restore_to_absence() {
        let temp = tempfile::tempdir().unwrap();
        let target = Path::new("rules/managed.md");
        let first = install_verified(
            &text_item(1, b"A"),
            b"A",
            temp.path(),
            target,
            None,
            InstallPolicy::default(),
        )
        .unwrap();
        let second = install_verified(
            &text_item(2, b"B"),
            b"B",
            temp.path(),
            target,
            Some(&first.local_hash),
            InstallPolicy::default(),
        )
        .unwrap();
        let backup_dir = second.backup.as_ref().unwrap().parent().unwrap();
        fs::remove_file(second.backup.as_ref().unwrap()).unwrap();
        assert!(matches!(
            restore_backup_with_limits_and_commit(
                temp.path(),
                target,
                backup_dir,
                AssetKind::Rule,
                ContentLimits::default(),
                |_| Ok(()),
            ),
            Err(InstallError::InvalidJournal)
        ));
        assert_eq!(fs::read(temp.path().join(target)).unwrap(), b"B");
    }

    #[test]
    fn expected_missing_target_detects_a_file_created_after_preview() {
        let temp = tempfile::tempdir().unwrap();
        let target = Path::new("rules/managed.md");
        let item = text_item(1, b"Team");
        fs::create_dir_all(temp.path().join("rules")).unwrap();
        fs::write(temp.path().join(target), b"created after preview").unwrap();
        let error = install_verified_with_limits(
            &item,
            b"Team",
            temp.path(),
            target,
            None,
            Some(None),
            InstallPolicy {
                overwrite_local: true,
            },
            ContentLimits::default(),
        )
        .unwrap_err();
        assert!(matches!(error, InstallError::PreviewStale));
        assert_eq!(
            fs::read(temp.path().join(target)).unwrap(),
            b"created after preview"
        );
    }

    #[test]
    fn second_cas_preserves_an_edit_made_while_the_replacement_is_staging() {
        let temp = tempfile::tempdir().unwrap();
        let target = Path::new("rules/managed.md");
        let item_a = text_item(1, b"A");
        let first = install_verified(
            &item_a,
            b"A",
            temp.path(),
            target,
            None,
            InstallPolicy::default(),
        )
        .unwrap();
        let item_b = text_item(2, b"B");
        let error = install_inner(
            &item_b,
            b"B",
            temp.path(),
            target,
            Some(&first.local_hash),
            Some(Some(&first.local_hash)),
            InstallPolicy::default(),
            ContentLimits::default(),
            FailPoint::ConcurrentEditBeforeReplace,
            |_| Ok(()),
        )
        .unwrap_err();
        assert!(matches!(error, InstallError::PreviewStale));
        assert_eq!(
            fs::read(temp.path().join(target)).unwrap(),
            b"concurrent local edit"
        );
    }

    #[test]
    fn crlf_only_local_change_is_not_drift() {
        let temp = tempfile::tempdir().unwrap();
        let target = Path::new("managed.md");
        let body = b"same\n";
        let item = text_item(1, body);
        let receipt = install_verified(
            &item,
            body,
            temp.path(),
            target,
            None,
            InstallPolicy::default(),
        )
        .unwrap();
        fs::write(&receipt.install_path, b"same\r\n").unwrap();
        assert_eq!(
            inspect_local(&item, temp.path(), target).unwrap(),
            Some(receipt.local_hash)
        );
    }

    #[test]
    fn local_and_both_modified_require_overwrite_and_always_backup() {
        let temp = tempfile::tempdir().unwrap();
        let target = Path::new("managed.md");
        fs::write(temp.path().join(target), b"personal").unwrap();
        let item_a = text_item(1, b"A");
        let error = install_verified(
            &item_a,
            b"A",
            temp.path(),
            target,
            None,
            InstallPolicy::default(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            InstallError::Conflict(DriftStatus::LocalModified)
        ));
        assert_eq!(fs::read(temp.path().join(target)).unwrap(), b"personal");
        let first = install_verified(
            &item_a,
            b"A",
            temp.path(),
            target,
            None,
            InstallPolicy {
                overwrite_local: true,
            },
        )
        .unwrap();
        assert_eq!(fs::read(first.backup.unwrap()).unwrap(), b"personal");

        fs::write(temp.path().join(target), b"local").unwrap();
        let item_b = text_item(2, b"B");
        assert!(matches!(
            install_verified(
                &item_b,
                b"B",
                temp.path(),
                target,
                Some(&first.local_hash),
                InstallPolicy::default(),
            ),
            Err(InstallError::Conflict(DriftStatus::BothModified))
        ));
        let overwritten = install_verified(
            &item_b,
            b"B",
            temp.path(),
            target,
            Some(&first.local_hash),
            InstallPolicy {
                overwrite_local: true,
            },
        )
        .unwrap();
        assert_eq!(fs::read(overwritten.backup.unwrap()).unwrap(), b"local");
    }

    #[test]
    fn injected_failure_keeps_a_and_recovery_restores_interrupted_backup() {
        let temp = tempfile::tempdir().unwrap();
        let target = Path::new("managed.md");
        let item_a = text_item(1, b"A");
        let first = install_verified(
            &item_a,
            b"A",
            temp.path(),
            target,
            None,
            InstallPolicy::default(),
        )
        .unwrap();
        let item_b = text_item(2, b"B");
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;

            let locked = OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(temp.path().join(target))
                .unwrap();
            let result = install_verified(
                &item_b,
                b"B",
                temp.path(),
                target,
                Some(&first.local_hash),
                InstallPolicy::default(),
            );
            assert!(matches!(result, Err(InstallError::Io(_))));
            drop(locked);
            assert_eq!(fs::read(temp.path().join(target)).unwrap(), b"A");
        }
        let result = install_inner(
            &item_b,
            b"B",
            temp.path(),
            target,
            Some(&first.local_hash),
            None,
            InstallPolicy::default(),
            ContentLimits::default(),
            FailPoint::AfterBackup,
            |_| Ok(()),
        );
        assert!(result.is_err());
        assert_eq!(fs::read(temp.path().join(target)).unwrap(), b"A");

        let result = install_inner(
            &item_b,
            b"B",
            temp.path(),
            target,
            Some(&first.local_hash),
            None,
            InstallPolicy::default(),
            ContentLimits::default(),
            FailPoint::AfterInstall,
            |_| Ok(()),
        );
        assert!(result.is_err());
        assert_eq!(fs::read(temp.path().join(target)).unwrap(), b"A");

        let context = RootContext::open(temp.path(), target, ContentLimits::default()).unwrap();
        context.prepare_control().unwrap();
        let operation = Uuid::new_v4();
        let backup_dir = context.control.join("backups").join(operation.to_string());
        fs::create_dir(&backup_dir).unwrap();
        fs::rename(&context.target, backup_dir.join("payload")).unwrap();
        let baseline_path = context.baseline_path();
        fs::rename(&baseline_path, backup_dir.join("baseline.json")).unwrap();
        let journal = Journal {
            version: 1,
            operation,
            phase: JournalPhase::BackedUp,
            target: relative_string(target),
            had_target: true,
            had_baseline: true,
            installed_target: true,
            installed_target_fingerprint: None,
            installed_baseline: true,
            installed_baseline_fingerprint: None,
        };
        write_journal(&context.control, &journal).unwrap();
        assert_eq!(recover_installs(temp.path()).unwrap(), 1);
        assert_eq!(fs::read(temp.path().join(target)).unwrap(), b"A");
        assert_eq!(
            inspect_local(&item_a, temp.path(), target).unwrap(),
            Some(first.local_hash)
        );
    }

    #[test]
    fn recovery_conflict_preserves_an_edited_installed_target_and_old_backup() {
        let temp = tempfile::tempdir().unwrap();
        let target = Path::new("managed.md");
        let item_a = text_item(1, b"A");
        install_verified(
            &item_a,
            b"A",
            temp.path(),
            target,
            None,
            InstallPolicy::default(),
        )
        .unwrap();

        let context = RootContext::open(temp.path(), target, ContentLimits::default()).unwrap();
        context.prepare_control().unwrap();
        let operation = Uuid::new_v4();
        let backup_dir = context.control.join("backups").join(operation.to_string());
        fs::create_dir(&backup_dir).unwrap();
        fs::rename(&context.target, backup_dir.join("payload")).unwrap();
        let baseline_path = context.baseline_path();
        fs::rename(&baseline_path, backup_dir.join("baseline.json")).unwrap();

        let item_b = text_item(2, b"B");
        fs::write(&context.target, b"B").unwrap();
        write_new_file(
            &baseline_path,
            &serde_json::to_vec(&BaselineRecord {
                version: 1,
                target: relative_string(target),
                content_hash: item_b.content_hash,
                files: Vec::new(),
            })
            .unwrap(),
        )
        .unwrap();
        let journal = Journal {
            version: 1,
            operation,
            phase: JournalPhase::Installed,
            target: relative_string(target),
            had_target: true,
            had_baseline: true,
            installed_target: true,
            installed_target_fingerprint: recovery_target_fingerprint(&context.target).unwrap(),
            installed_baseline: true,
            installed_baseline_fingerprint: recovery_target_fingerprint(&baseline_path).unwrap(),
        };
        write_journal(&context.control, &journal).unwrap();

        fs::write(&context.target, b"C written while the client was down").unwrap();
        let error = recover_installs(temp.path()).unwrap_err();
        assert!(matches!(error, InstallError::RecoveryConflict(_)));
        assert_eq!(
            fs::read(&context.target).unwrap(),
            b"C written while the client was down"
        );
        assert_eq!(fs::read(backup_dir.join("payload")).unwrap(), b"A");
        assert!(backup_dir.join("baseline.json").exists());
        assert!(context
            .control
            .join("journals")
            .join(format!("{}.2.json", operation))
            .exists());
    }

    #[test]
    fn restore_to_absence_recovery_conflict_preserves_a_new_target_and_old_backup() {
        let temp = tempfile::tempdir().unwrap();
        let target = Path::new("managed.md");
        let item_a = text_item(1, b"A");
        install_verified(
            &item_a,
            b"A",
            temp.path(),
            target,
            None,
            InstallPolicy::default(),
        )
        .unwrap();

        let context = RootContext::open(temp.path(), target, ContentLimits::default()).unwrap();
        context.prepare_control().unwrap();
        let operation = Uuid::new_v4();
        let backup_dir = context.control.join("backups").join(operation.to_string());
        fs::create_dir(&backup_dir).unwrap();
        fs::rename(&context.target, backup_dir.join("payload")).unwrap();
        let baseline_path = context.baseline_path();
        fs::rename(&baseline_path, backup_dir.join("baseline.json")).unwrap();
        let journal = Journal {
            version: 1,
            operation,
            phase: JournalPhase::Installed,
            target: relative_string(target),
            had_target: true,
            had_baseline: true,
            installed_target: false,
            installed_target_fingerprint: None,
            installed_baseline: false,
            installed_baseline_fingerprint: None,
        };
        write_journal(&context.control, &journal).unwrap();

        fs::write(
            &context.target,
            b"C created after restore-to-absence crashed",
        )
        .unwrap();
        let error = recover_installs(temp.path()).unwrap_err();
        assert!(matches!(error, InstallError::RecoveryConflict(_)));
        assert_eq!(
            fs::read(&context.target).unwrap(),
            b"C created after restore-to-absence crashed"
        );
        assert_eq!(fs::read(backup_dir.join("payload")).unwrap(), b"A");
        assert!(backup_dir.join("baseline.json").exists());
    }

    #[test]
    fn skill_install_preserves_neighbor_and_unsubscribe_never_deletes() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("personal.txt"), b"keep").unwrap();
        let (item, archive) = skill_item(b"\xef\xbb\xbf# Team\r\n");
        let target = Path::new("skills/team-skill");
        let receipt = install_verified(
            &item,
            &archive,
            temp.path(),
            target,
            None,
            InstallPolicy::default(),
        )
        .unwrap();
        assert_eq!(
            fs::read(receipt.install_path.join("SKILL.md")).unwrap(),
            b"# Team\n"
        );
        assert_eq!(fs::read(temp.path().join("personal.txt")).unwrap(), b"keep");
        assert_eq!(subscription_status(false), SubscriptionStatus::Unsubscribed);
        assert!(receipt.install_path.exists());
    }

    #[test]
    fn rejects_target_traversal_and_symlink_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let item = text_item(1, b"A");
        assert!(matches!(
            install_verified(
                &item,
                b"A",
                temp.path(),
                Path::new("../outside"),
                None,
                InstallPolicy::default(),
            ),
            Err(InstallError::UnsafePath)
        ));

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(temp.path(), temp.path().join("linked")).unwrap();
            assert!(matches!(
                install_verified(
                    &item,
                    b"A",
                    temp.path(),
                    Path::new("linked/outside"),
                    None,
                    InstallPolicy::default(),
                ),
                Err(InstallError::UnsafeFilesystem)
            ));
        }
        #[cfg(windows)]
        {
            let outside = tempfile::tempdir().unwrap();
            let link = temp.path().join("linked");
            let created = std::process::Command::new("cmd.exe")
                .args(["/d", "/c", "mklink", "/J"])
                .arg(&link)
                .arg(outside.path())
                .output()
                .unwrap();
            assert!(created.status.success());
            assert!(matches!(
                install_verified(
                    &item,
                    b"A",
                    temp.path(),
                    Path::new("linked/outside"),
                    None,
                    InstallPolicy::default(),
                ),
                Err(InstallError::UnsafeFilesystem)
            ));
            fs::remove_dir(link).unwrap();
        }
    }
}
