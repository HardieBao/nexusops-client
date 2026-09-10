//! Fixed TeamAI resource worker. It never receives member credentials or writes tool files.
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

use super::api::Cancellation;

const TIMEOUT: Duration = Duration::from_secs(15);
const OUTPUT_LIMIT: usize = 4 << 20;
const DIAGNOSTIC_LIMIT: usize = 8 << 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum WorkerError {
    #[error("invalid_input")]
    InvalidInput,
    #[error("unsafe_path")]
    UnsafePath,
    #[error("too_large")]
    TooLarge,
    #[error("cancelled")]
    Cancelled,
    #[error("timeout")]
    Timeout,
    #[error("worker_unavailable")]
    WorkerUnavailable,
    #[error("invalid_response")]
    InvalidResponse,
    #[error("unsupported_operation")]
    UnsupportedOperation,
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Version,
    InspectSkill {
        root: PathBuf,
    },
    ConvertRule {
        root: PathBuf,
        entry: String,
        target: RuleTarget,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleTarget {
    Codex,
    ClaudeCode,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    schema_version: u8,
    request_id: String,
    ok: bool,
    data: Option<Value>,
    error: Option<Failure>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Failure {
    code: String,
    message: String,
}

pub struct Worker {
    directory: PathBuf,
}

impl Worker {
    /// The caller supplies the installed resource directory, never a renderer-selected executable.
    pub fn open(directory: &Path) -> Result<Self, WorkerError> {
        let directory =
            dunce::canonicalize(directory).map_err(|_| WorkerError::WorkerUnavailable)?;
        let manifest: Value = serde_json::from_slice(
            &std::fs::read(directory.join("manifest.json"))
                .map_err(|_| WorkerError::WorkerUnavailable)?,
        )
        .map_err(|_| WorkerError::WorkerUnavailable)?;
        let runtime: Value =
            serde_json::from_str(include_str!("../../../../scripts/teamai/runtime.json"))
                .map_err(|_| WorkerError::WorkerUnavailable)?;
        if manifest["upstream_commit"] != "6ae0619d067b1699bb2c6e435abf3ffe11a21d71" {
            return Err(WorkerError::WorkerUnavailable);
        }
        for (file, expected) in [
            ("node.exe", &runtime["executable"]["sha256"]),
            ("worker.mjs", &manifest["worker_sha256"]),
        ] {
            let metadata = std::fs::symlink_metadata(directory.join(file))
                .map_err(|_| WorkerError::WorkerUnavailable)?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() > 128 << 20
            {
                return Err(WorkerError::WorkerUnavailable);
            }
            let bytes =
                std::fs::read(directory.join(file)).map_err(|_| WorkerError::WorkerUnavailable)?;
            if expected.as_str() != Some(format!("{:x}", Sha256::digest(bytes)).as_str()) {
                return Err(WorkerError::WorkerUnavailable);
            }
        }
        Ok(Self { directory })
    }

    pub async fn run(
        &self,
        operation: Operation,
        cancellation: &Cancellation,
    ) -> Result<Value, WorkerError> {
        if cancellation.is_cancelled() {
            return Err(WorkerError::Cancelled);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let mut request = json!({"schema_version":1,"request_id":id,"operation":"version"});
        let mut root = None;
        match operation {
            Operation::Version => {}
            Operation::InspectSkill { root: selected } => {
                root = Some(checked_root(&selected)?);
                request["operation"] = json!("inspect_skill");
                request["input"] = json!({"root":root,"entry":"SKILL.md"});
            }
            Operation::ConvertRule {
                root: selected,
                entry,
                target,
            } => {
                root = Some(checked_root(&selected)?);
                request["operation"] = json!("convert_rule");
                request["input"] = json!({"root":root,"entry":entry,"target":match target {RuleTarget::Codex=>"codex",RuleTarget::ClaudeCode=>"claude-code"}});
            }
        }
        let input = serde_json::to_vec(&request).map_err(|_| WorkerError::InvalidInput)?;
        if input.len() > 1 << 20 {
            return Err(WorkerError::TooLarge);
        }
        let mut command = Command::new(self.directory.join("node.exe"));
        command
            .env_clear()
            .current_dir(&self.directory)
            .arg("--permission")
            .arg(format!("--allow-fs-read={}", self.directory.display()));
        if let Some(root) = root {
            command.arg(format!("--allow-fs-read={}", root.display()));
        }
        if let Some(system_root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", system_root);
        }
        #[cfg(windows)]
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        command
            .arg(self.directory.join("worker.mjs"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        run_process(command, input, &id, cancellation, TIMEOUT).await
    }
}

fn checked_root(selected: &Path) -> Result<PathBuf, WorkerError> {
    let metadata = std::fs::symlink_metadata(selected).map_err(|_| WorkerError::UnsafePath)?;
    if !selected.is_absolute() || !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(WorkerError::UnsafePath);
    }
    let root = dunce::canonicalize(selected).map_err(|_| WorkerError::UnsafePath)?;
    // Node's permission path syntax treats '*' as a wildcard.
    if root.to_str().is_none_or(|path| path.contains('*')) {
        return Err(WorkerError::UnsafePath);
    }
    Ok(root)
}

async fn read_bounded(
    reader: impl AsyncRead + Unpin,
    limit: usize,
) -> Result<Vec<u8>, WorkerError> {
    let mut bytes = Vec::new();
    reader
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| WorkerError::InvalidResponse)?;
    if bytes.len() > limit {
        return Err(WorkerError::TooLarge);
    }
    Ok(bytes)
}

async fn run_process(
    mut command: Command,
    input: Vec<u8>,
    id: &str,
    cancellation: &Cancellation,
    timeout: Duration,
) -> Result<Value, WorkerError> {
    let mut child = command
        .spawn()
        .map_err(|_| WorkerError::WorkerUnavailable)?;
    let mut stdin = child.stdin.take().ok_or(WorkerError::WorkerUnavailable)?;
    let stdout = child.stdout.take().ok_or(WorkerError::WorkerUnavailable)?;
    let stderr = child.stderr.take().ok_or(WorkerError::WorkerUnavailable)?;
    let result = {
        let communicate = async {
            let (_, output, _diagnostics, status) = tokio::try_join!(
                async move {
                    stdin
                        .write_all(&input)
                        .await
                        .map_err(|_| WorkerError::InvalidResponse)?;
                    drop(stdin);
                    Ok::<(), WorkerError>(())
                },
                read_bounded(stdout, OUTPUT_LIMIT),
                read_bounded(stderr, DIAGNOSTIC_LIMIT),
                async { child.wait().await.map_err(|_| WorkerError::InvalidResponse) }
            )?;
            decode(&output, id, status.code())
        };
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(WorkerError::Cancelled),
            _ = tokio::time::sleep(timeout) => Err(WorkerError::Timeout),
            result = communicate => result,
        }
    };
    if result.is_err() {
        // Kill and wait before returning; dropping pipe futures also closes their handles.
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    result
}

fn decode(bytes: &[u8], id: &str, exit: Option<i32>) -> Result<Value, WorkerError> {
    let response: Response =
        serde_json::from_slice(bytes).map_err(|_| WorkerError::InvalidResponse)?;
    if response.schema_version != 1 || response.request_id != id {
        return Err(WorkerError::InvalidResponse);
    }
    if response.ok {
        if exit != Some(0) || response.error.is_some() {
            return Err(WorkerError::InvalidResponse);
        }
        return response
            .data
            .filter(Value::is_object)
            .ok_or(WorkerError::InvalidResponse);
    }
    if exit != Some(1) || response.data.is_some() {
        return Err(WorkerError::InvalidResponse);
    }
    let failure = response.error.ok_or(WorkerError::InvalidResponse)?;
    if failure.message != failure.code {
        return Err(WorkerError::InvalidResponse);
    }
    Err(match failure.code.as_str() {
        "invalid_input" => WorkerError::InvalidInput,
        "unsafe_path" => WorkerError::UnsafePath,
        "too_large" => WorkerError::TooLarge,
        "unsupported_operation" => WorkerError::UnsupportedOperation,
        _ => WorkerError::InvalidResponse,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_mismatched_protocol_and_exit_status_without_echoing_content() {
        let valid = json!({"schema_version":1,"request_id":"test","ok":true,"data":{}});
        assert_eq!(
            decode(&serde_json::to_vec(&valid).unwrap(), "test", Some(0)),
            Ok(json!({}))
        );
        assert_eq!(
            decode(&serde_json::to_vec(&valid).unwrap(), "other", Some(0)),
            Err(WorkerError::InvalidResponse)
        );
        assert_eq!(
            decode(&serde_json::to_vec(&valid).unwrap(), "test", Some(1)),
            Err(WorkerError::InvalidResponse)
        );
        assert_eq!(
            decode(b"private invalid response", "test", Some(0)),
            Err(WorkerError::InvalidResponse)
        );
        let unsafe_message = json!({"schema_version":1,"request_id":"test","ok":false,"error":{"code":"invalid_input","message":"private path"}});
        assert_eq!(
            decode(
                &serde_json::to_vec(&unsafe_message).unwrap(),
                "test",
                Some(1)
            ),
            Err(WorkerError::InvalidResponse)
        );
    }

    #[tokio::test]
    async fn rejects_oversized_output() {
        assert_eq!(
            read_bounded(&b"12345"[..], 4).await,
            Err(WorkerError::TooLarge)
        );
        assert_eq!(read_bounded(&b"1234"[..], 4).await.unwrap(), b"1234");
    }

    #[cfg(windows)]
    fn bundle() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../.teamai-build")
    }

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "requires pnpm teamai:prepare; run explicitly for Windows runtime acceptance"]
    async fn bundled_worker_runs_from_rust_without_global_node() {
        let worker = Worker::open(&bundle()).unwrap();
        let result = worker
            .run(Operation::Version, &Cancellation::default())
            .await
            .unwrap();
        assert_eq!(
            result["upstream_commit"],
            "6ae0619d067b1699bb2c6e435abf3ffe11a21d71"
        );
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("中文 资源");
        std::fs::create_dir(&root).unwrap();
        let bytes = b"---\nname: test\n---\nExample\n";
        std::fs::write(root.join("SKILL.md"), bytes).unwrap();
        let result = worker
            .run(
                Operation::InspectSkill { root: root.clone() },
                &Cancellation::default(),
            )
            .await
            .unwrap();
        assert_eq!(result["name"], "test");
        assert_eq!(std::fs::read(root.join("SKILL.md")).unwrap(), bytes);
    }

    #[cfg(windows)]
    fn probe(script: &str) -> Command {
        let mut command = Command::new(bundle().join("node.exe"));
        command
            .env_clear()
            .arg("-e")
            .arg(script)
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Windows crypto initialization needs this OS variable, just like the production launcher.
        if let Some(system_root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", system_root);
        }
        command
    }

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "requires pnpm teamai:prepare; run explicitly for Windows runtime acceptance"]
    async fn timed_out_worker_is_reaped_before_returning() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid");
        let mut command = probe("require('fs').writeFileSync(process.argv[1],String(process.pid));setInterval(()=>{},1000)");
        command.arg(&pid_file);
        assert_eq!(
            run_process(
                command,
                vec![],
                "test",
                &Cancellation::default(),
                Duration::from_secs(1)
            )
            .await,
            Err(WorkerError::Timeout)
        );
        let pid = std::fs::read_to_string(pid_file).unwrap();
        let result = probe(&format!("try{{process.kill({pid},0);process.exit(2)}}catch(e){{if(e.code!=='ESRCH')process.exit(3)}}")).output().await.unwrap();
        assert!(result.status.success());
    }

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "requires pnpm teamai:prepare; run explicitly for Windows runtime acceptance"]
    async fn cancellation_and_excessive_diagnostics_end_the_worker() {
        let cancellation = Cancellation::default();
        let other = cancellation.clone();
        let trigger = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            other.cancel();
        });
        assert_eq!(
            run_process(
                probe("setInterval(()=>{},1000)"),
                vec![],
                "test",
                &cancellation,
                TIMEOUT
            )
            .await,
            Err(WorkerError::Cancelled)
        );
        trigger.await.unwrap();
        assert_eq!(
            run_process(
                probe("process.stderr.write('x'.repeat(9000));setInterval(()=>{},1000)"),
                vec![],
                "test",
                &Cancellation::default(),
                TIMEOUT
            )
            .await,
            Err(WorkerError::TooLarge)
        );
    }
}
