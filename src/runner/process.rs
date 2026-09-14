//! Lancement du processus `bru`, transport du rapport et annulation.
//!
//! Le rapport transite par un pipe anonyme dont l'extrémité écriture devient
//! le fd 3 de `bru` (`--reporter-json /dev/fd/3`) : aucun fichier n'est créé.
//! Chaque exécution tourne dans sa propre tâche tokio et délivre exactement
//! un [`RunEvent`] sur le canal fourni au [`BruRunner`].

use std::ffi::OsString;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::{mpsc, oneshot};

use super::error::{RunError, RunOutcome};
use super::report::Report;
use super::request::RunRequest;

/// Taille maximale conservée de la sortie console de `bru`.
#[cfg_attr(not(unix), allow(dead_code))]
const OUTPUT_TAIL_BYTES: usize = 64 * 1024;

/// Identifiant d'une exécution, unique pour un [`BruRunner`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RunId(pub u64);

/// Issue d'une exécution, émise une seule fois par exécution.
#[derive(Debug)]
pub struct RunEvent {
    pub id: RunId,
    pub outcome: RunOutcome,
}

/// Lance des exécutions `bru run` et publie leurs issues sur un canal.
#[derive(Debug)]
pub struct BruRunner {
    program: OsString,
    next_id: AtomicU64,
    events: mpsc::Sender<RunEvent>,
}

impl BruRunner {
    /// Runner utilisant le programme `bru` du `PATH`.
    pub fn new(events: mpsc::Sender<RunEvent>) -> Self {
        Self::with_program("bru", events)
    }

    /// Runner utilisant un programme explicite (chemin ou nom dans le `PATH`).
    pub fn with_program(program: impl Into<OsString>, events: mpsc::Sender<RunEvent>) -> Self {
        Self {
            program: program.into(),
            next_id: AtomicU64::new(1),
            events,
        }
    }

    /// Lance une exécution et rend la main immédiatement.
    ///
    /// Doit être appelé depuis un runtime tokio. L'issue arrive sur le canal ;
    /// détruire le [`RunHandle`] retourné annule l'exécution.
    pub fn start(&self, request: RunRequest) -> RunHandle {
        let id = RunId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let program = self.program.clone();
        let events = self.events.clone();
        tokio::spawn(async move {
            let outcome = execute(program, request, cancel_rx).await;
            // Récepteur fermé : plus personne n'attend l'issue, rien à faire.
            let _ = events.send(RunEvent { id, outcome }).await;
        });
        RunHandle {
            id,
            cancel: Some(cancel_tx),
        }
    }
}

/// Poignée d'une exécution en cours. Sa destruction annule l'exécution.
#[derive(Debug)]
pub struct RunHandle {
    id: RunId,
    cancel: Option<oneshot::Sender<()>>,
}

impl RunHandle {
    pub fn id(&self) -> RunId {
        self.id
    }

    /// Termine `bru` ; l'issue délivrée sera [`RunOutcome::Cancelled`] si
    /// l'exécution n'était pas déjà terminée.
    pub fn cancel(mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
    }
}

#[cfg(not(unix))]
async fn execute(
    _program: OsString,
    _request: RunRequest,
    _cancel: oneshot::Receiver<()>,
) -> RunOutcome {
    RunOutcome::Failed(RunError::UnsupportedPlatform)
}

#[cfg(unix)]
async fn execute(
    program: OsString,
    request: RunRequest,
    cancel: oneshot::Receiver<()>,
) -> RunOutcome {
    unix::execute(program, request, cancel).await
}

/// Détermine l'issue une fois `bru` terminé et le pipe du rapport fermé.
///
/// Le code de sortie ne décide jamais du succès : `bru` sort en 0 sans
/// rapport hors d'une racine de collection, et en 1 avec un rapport valide
/// quand des tests échouent.
#[cfg_attr(not(unix), allow(dead_code))]
fn classify(exit_code: Option<i32>, report: Vec<u8>, output: String) -> RunOutcome {
    if report.iter().all(u8::is_ascii_whitespace) {
        return RunOutcome::Failed(RunError::NoReport { exit_code, output });
    }
    match serde_json::from_slice::<Report>(&report) {
        Ok(report) => RunOutcome::Completed { report, exit_code },
        Err(error) => RunOutcome::Failed(RunError::invalid_report(&error)),
    }
}

/// Fusionne stdout et stderr en ne gardant que les derniers octets.
#[cfg_attr(not(unix), allow(dead_code))]
fn output_tail(stdout: Vec<u8>, stderr: Vec<u8>) -> String {
    let mut combined = stdout;
    combined.extend_from_slice(&stderr);
    let start = combined.len().saturating_sub(OUTPUT_TAIL_BYTES);
    String::from_utf8_lossy(&combined[start..]).into_owned()
}

#[cfg(unix)]
mod unix {
    use std::ffi::OsString;
    use std::io::{self, Read};
    use std::process::Stdio;
    use std::time::Duration;

    use command_fds::{CommandFdExt, FdMapping};
    use tokio::io::{AsyncRead, AsyncReadExt};
    use tokio::process::{Child, Command};
    use tokio::sync::oneshot;
    use tokio::task::JoinHandle;

    use super::{OUTPUT_TAIL_BYTES, classify, output_tail};
    use crate::runner::error::{RunError, RunOutcome};
    use crate::runner::request::{REPORT_FD, RunRequest};

    /// Délai de lecture du rapport après la fin de `bru`, au cas où un
    /// sous-processus aurait hérité du fd 3 et le garderait ouvert.
    const REPORT_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

    struct Running {
        child: Child,
        report: JoinHandle<io::Result<Vec<u8>>>,
        stdout: JoinHandle<Vec<u8>>,
        stderr: JoinHandle<Vec<u8>>,
    }

    pub(super) async fn execute(
        program: OsString,
        request: RunRequest,
        cancel: oneshot::Receiver<()>,
    ) -> RunOutcome {
        let mut running = match spawn(program, &request) {
            Ok(running) => running,
            Err(error) => return RunOutcome::Failed(error),
        };

        // Émetteur envoyé ou détruit : les deux valent annulation.
        let status = tokio::select! {
            status = running.child.wait() => status,
            _ = cancel => {
                // `kill` envoie SIGKILL puis attend : pas de processus zombie.
                let _ = running.child.kill().await;
                return RunOutcome::Cancelled;
            }
        };

        let status = match status {
            Ok(status) => status,
            Err(error) => return RunOutcome::Failed(RunError::Io(error)),
        };

        let report = match tokio::time::timeout(REPORT_DRAIN_TIMEOUT, running.report).await {
            Ok(Ok(Ok(report))) => report,
            Ok(Ok(Err(error))) => return RunOutcome::Failed(RunError::Io(error)),
            Ok(Err(join_error)) => {
                return RunOutcome::Failed(RunError::Io(io::Error::other(join_error)));
            }
            // Le fd 3 est resté ouvert chez un descendant : rapport incomplet.
            Err(_elapsed) => Vec::new(),
        };
        // Même garde pour la sortie console, qu'un descendant peut aussi retenir.
        let (stdout, stderr) =
            tokio::join!(drain_output(running.stdout), drain_output(running.stderr));

        classify(status.code(), report, output_tail(stdout, stderr))
    }

    fn spawn(program: OsString, request: &RunRequest) -> Result<Running, RunError> {
        if !request.collection_root.is_dir() {
            return Err(RunError::Spawn(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "racine de collection introuvable : {}",
                    request.collection_root.display()
                ),
            )));
        }

        let (report_reader, report_writer) = io::pipe().map_err(RunError::Io)?;

        let mut command = Command::new(program);
        command
            .args(request.bru_args())
            .current_dir(&request.collection_root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command
            .fd_mappings(vec![FdMapping {
                parent_fd: report_writer.into(),
                child_fd: REPORT_FD,
            }])
            .map_err(|collision| RunError::Io(io::Error::other(collision)))?;

        let spawned = command.spawn();
        // La commande détient la copie parent de l'extrémité écriture : la
        // détruire est indispensable pour recevoir EOF quand `bru` se termine.
        drop(command);

        let mut child = spawned.map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => RunError::BruNotFound,
            _ => RunError::Spawn(error),
        })?;

        let report = tokio::task::spawn_blocking(move || {
            let mut reader = report_reader;
            let mut report = Vec::new();
            reader.read_to_end(&mut report).map(|_| report)
        });
        let stdout = tokio::spawn(read_tail(child.stdout.take()));
        let stderr = tokio::spawn(read_tail(child.stderr.take()));

        Ok(Running {
            child,
            report,
            stdout,
            stderr,
        })
    }

    async fn drain_output(task: JoinHandle<Vec<u8>>) -> Vec<u8> {
        match tokio::time::timeout(REPORT_DRAIN_TIMEOUT, task).await {
            Ok(Ok(output)) => output,
            Ok(Err(_)) | Err(_) => Vec::new(),
        }
    }

    /// Lit un flux jusqu'à EOF en ne gardant que les derniers octets.
    async fn read_tail<R: AsyncRead + Unpin>(stream: Option<R>) -> Vec<u8> {
        let Some(mut stream) = stream else {
            return Vec::new();
        };
        let mut tail = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match stream.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    tail.extend_from_slice(&chunk[..read]);
                    if tail.len() > 2 * OUTPUT_TAIL_BYTES {
                        tail.drain(..tail.len() - OUTPUT_TAIL_BYTES);
                    }
                }
            }
        }
        tail
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_or_blank_report_is_no_report_whatever_the_exit_code() {
        for (code, report) in [(Some(0), b"".to_vec()), (Some(1), b" \n".to_vec())] {
            let outcome = classify(code, report, "sortie".into());
            assert!(matches!(
                outcome,
                RunOutcome::Failed(RunError::NoReport { exit_code, ref output })
                    if exit_code == code && output == "sortie"
            ));
        }
    }

    #[test]
    fn output_tail_keeps_the_end() {
        let stdout = vec![b'a'; OUTPUT_TAIL_BYTES];
        let tail = output_tail(stdout, b"fin".to_vec());
        assert_eq!(tail.len(), OUTPUT_TAIL_BYTES);
        assert!(tail.ends_with("fin"));
    }
}
