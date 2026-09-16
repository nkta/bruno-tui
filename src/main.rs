//! Point d'entrée du binaire `bruno-tui`.
//!
//! Analyse les arguments, prépare le terminal et le runtime, lance
//! l'interface, puis restaure le terminal quoi qu'il arrive.

use std::io::{self, IsTerminal};
use std::process::ExitCode;
use std::sync::Arc;

use bruno_tui::app::cli::{self, Command, USAGE, VERSION};
use bruno_tui::app::clipboard::SystemClipboard;
use bruno_tui::app::event::{EVENT_BUFFER, spawn_terminal_reader};
use bruno_tui::app::model::Exit;
use bruno_tui::app::run;
use bruno_tui::collection::BruLoader;
use tokio::sync::mpsc;

/// Code de sortie d'une erreur d'usage.
const USAGE_ERROR: u8 = 2;

fn main() -> ExitCode {
    let path = match cli::parse(std::env::args_os().skip(1)) {
        Ok(Command::Run(path)) => path,
        Ok(Command::Help) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Ok(Command::Version) => {
            println!("{VERSION}");
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("bruno-tui : {error}\n\n{USAGE}");
            return ExitCode::from(USAGE_ERROR);
        }
    };

    // Vérifié avant toute modification de l'état du terminal.
    if !io::stdout().is_terminal() {
        eprintln!("bruno-tui : la sortie standard n'est pas un terminal interactif");
        return ExitCode::FAILURE;
    }

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("bruno-tui : impossible de démarrer le runtime : {error}");
            return ExitCode::FAILURE;
        }
    };

    // Installe aussi le hook de panique qui restaure le terminal.
    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(error) => {
            let _ = ratatui::try_restore();
            eprintln!("bruno-tui : impossible d'initialiser le terminal : {error}");
            return ExitCode::FAILURE;
        }
    };

    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let result = match spawn_terminal_reader(sender.clone()) {
        Ok(()) => runtime.block_on(run(
            &mut terminal,
            Arc::new(BruLoader),
            path,
            sender,
            events,
            "bru".into(),
            Arc::new(SystemClipboard),
            Arc::new(bruno_tui::writer::BruWriter),
        )),
        Err(error) => Ok(Exit::TerminalError(error)),
    };

    let restored = ratatui::try_restore();
    // N'attend pas un chargement encore en cours : il est en lecture seule.
    runtime.shutdown_background();

    let code = match result {
        Ok(Exit::Normal) => ExitCode::SUCCESS,
        Ok(Exit::TerminalError(error)) => {
            eprintln!("bruno-tui : terminal inutilisable : {error}");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("bruno-tui : erreur d'affichage : {error}");
            ExitCode::FAILURE
        }
    };
    if let Err(error) = restored {
        eprintln!("bruno-tui : restauration du terminal incomplète : {error}");
        return ExitCode::FAILURE;
    }
    code
}
