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
use bruno_tui::app::mouse::{MouseCapture, MouseSetup, TerminalMouseCapture};
use bruno_tui::app::run;
use bruno_tui::collection::AutoLoader;
use tokio::sync::mpsc;

/// Code de sortie d'une erreur d'usage.
const USAGE_ERROR: u8 = 2;

fn main() -> ExitCode {
    let (paths, secrets, mouse, watch, history, editor) =
        match cli::parse(std::env::args_os().skip(1)) {
            Ok(Command::Run {
                paths,
                secrets,
                mouse,
                watch,
                history,
                editor,
            }) => (paths, secrets, mouse, watch, history, editor),
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

    // Provisoire : seul le premier chemin est ouvert.
    let path = paths.into_iter().next().unwrap_or_else(|| ".".into());

    // Vérifié avant toute modification de l'état du terminal.
    if !io::stdout().is_terminal() {
        eprintln!("bruno-tui : la sortie standard n'est pas un terminal interactif");
        return ExitCode::FAILURE;
    }

    // Avant tout autre thread : voir `clock::init_local_offset`.
    bruno_tui::app::clock::init_local_offset();

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

    // Le hook de ratatui restaure mode brut et écran alternatif, pas la
    // capture souris : elle est rendue au terminal avant lui.
    let ratatui_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = TerminalMouseCapture.set(false);
        ratatui_hook(info);
    }));

    let mut mouse_setup = MouseSetup::terminal(false);
    if mouse {
        match TerminalMouseCapture.set(true) {
            Ok(()) => mouse_setup.enabled = true,
            Err(error) => mouse_setup.startup_error = Some(error.to_string()),
        }
    }

    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let suspended = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let result = match spawn_terminal_reader(sender.clone(), Arc::clone(&suspended)) {
        Ok(()) => runtime.block_on(run(
            &mut terminal,
            Arc::new(AutoLoader),
            path,
            sender,
            events,
            "bru".into(),
            Arc::new(SystemClipboard::default()),
            Arc::new(bruno_tui::writer::BruWriter),
            secrets,
            mouse_setup,
            suspended,
            watch,
            history,
            editor,
        )),
        Err(error) => Ok(Exit::TerminalError(error)),
    };

    // Sans effet si la capture n'était pas active.
    let _ = TerminalMouseCapture.set(false);
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
