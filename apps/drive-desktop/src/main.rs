mod dialog;
mod ui;
use anyhow::Result;
use clap::{Parser, Subcommand};
use engine::Paths;
use std::{path::PathBuf, time::Duration};

#[derive(Parser)]
#[command(
    name = "drive-desktop",
    version,
    about = "Cliente independente Google Drive para Linux — espelhamento em Rust"
)]
struct Arguments {
    /// Captura a interface em PNG e encerra, para validação visual.
    #[arg(long)]
    screenshot: Option<PathBuf>,
    /// Página de preferências a capturar; não conecta nem sincroniza.
    #[arg(long, value_enum, requires = "screenshot")]
    screenshot_page: Option<ui::ScreenshotPage>,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    /// Salva a configuração. Não inicia sincronização nem abre login.
    Configure {
        #[arg(long)]
        local: PathBuf,
        #[arg(long)]
        remote: String,
        #[arg(long)]
        oauth: Option<PathBuf>,
        #[arg(long)]
        propagate_deletions: bool,
    },
    /// Abre OAuth no navegador; refresh token fica no Secret Service.
    Login {
        /// Conecta antes de configurar as pastas de sincronização.
        #[arg(long)]
        oauth: Option<PathBuf>,
    },
    /// Abre uma referência .gdoc/.gsheet/.gslides no navegador.
    OpenDocument {
        path: PathBuf,
    },
    Disconnect,
    /// Inspeciona sem alterar arquivos ou fazer uploads.
    Plan,
    /// Executa um ciclo completo de espelhamento.
    Sync,
    /// Executa ciclos periódicos para uso como serviço de usuário.
    Watch,
    Status,
    /// Consulta nome, email e armazenamento informados pelo Google; não retorna tokens.
    Account,
    Pause,
    Resume,
    History,
    /// Inspeciona e replaneja operações interrompidas preservando a base anterior.
    Recover,
    /// Preserva ambas as versões de um conflito, copiando a versão local.
    Resolve {
        path: String,
    },
    /// Lista subpastas remotas para ajudar a escolher a raiz.
    Folders {
        #[arg(default_value = "root")]
        parent: String,
    },
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Erro: {error:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let arguments = Arguments::parse();
    let paths = Paths::discover()?;
    match arguments.command {
        None => ui::launch(
            paths,
            arguments.screenshot,
            arguments.screenshot_page.unwrap_or_default(),
        )?,
        Some(Command::Configure {
            local,
            remote,
            oauth,
            propagate_deletions,
        }) => {
            let oauth = match oauth {
                Some(path) => path,
                None => paths.application_credentials()?,
            };
            paths.configure(&local, &remote, &oauth, propagate_deletions)?;
            println!("Configuração salva. Execute login e plan antes de sync.");
        }
        Some(Command::OpenDocument { path }) => {
            anyhow::ensure!(
                std::fs::metadata(&path)?.len() <= 16384,
                "Referência grande demais"
            );
            let url = drive_api::reference_url(&std::fs::read(path)?)?;
            anyhow::ensure!(
                std::process::Command::new("xdg-open")
                    .arg(url)
                    .status()?
                    .success(),
                "Não foi possível abrir o navegador"
            );
        }
        Some(Command::Login { oauth }) => {
            if let Some(credentials) = oauth {
                paths.connect(&credentials)?;
            } else {
                paths.login()?;
            }
            println!("Conta conectada. Credencial guardada no chaveiro da sessão.");
        }
        Some(Command::Disconnect) => {
            paths.disconnect()?;
            println!(
                "Credencial local removida. Para revogar a permissão, use a segurança da conta Google."
            );
        }
        Some(Command::Plan) => println!("{}", serde_json::to_string_pretty(&paths.cycle(true)?)?),
        Some(Command::Sync) => println!("{}", serde_json::to_string_pretty(&paths.cycle(false)?)?),
        Some(Command::Account) => {
            println!("{}", serde_json::to_string_pretty(&paths.account_info()?)?)
        }
        Some(Command::Status) => {
            println!("{}", serde_json::to_string_pretty(&paths.status_json()?)?)
        }
        Some(Command::Pause) => paths.set_paused(true)?,
        Some(Command::Resume) => paths.set_paused(false)?,
        Some(Command::Recover) => println!("{} operações inspecionadas", paths.replan_pending()?),
        Some(Command::Resolve { path }) => {
            paths.keep_both(&path)?;
            println!("Versões preservadas. Execute sync para enviar a cópia local.");
        }
        Some(Command::History) => println!("{}", serde_json::to_string_pretty(&paths.history()?)?),
        Some(Command::Folders { parent }) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&paths.folders(&parent)?)?
            );
        }
        Some(Command::Watch) => {
            let mut failures = 0_u32;
            let mut watch_root = None;
            let mut watcher = None;
            loop {
                let config = paths.load()?;
                let mut delay = Duration::from_secs(config.interval_seconds.clamp(5, 3600));
                if watch_root.as_deref() != Some(config.local.as_path()) {
                    watch_root = Some(config.local.clone());
                    watcher = match paths.watch_local(&config) {
                        Ok(watcher) => Some(watcher),
                        Err(error) => {
                            eprintln!("Observação local indisponível; polling mantido: {error:#}");
                            None
                        }
                    };
                }
                if !paths.paused()? {
                    match paths.cycle(false) {
                        Ok(report) => {
                            failures = 0;
                            println!("{}", serde_json::to_string(&report)?);
                        }
                        Err(error) => {
                            eprintln!("Ciclo interrompido: {error:#}");
                            if let Some(minimum) = drive_api::retry_minimum(&error) {
                                failures = failures.saturating_add(1);
                                delay = engine::retry_delay(failures, minimum);
                            } else {
                                // Persistent failures require explicit resume; never loop destructive intents blindly.
                                paths.set_paused(true)?;
                                failures = 0;
                            }
                        }
                    }
                }
                if paths.paused()? || failures > 0 {
                    std::thread::sleep(delay);
                } else if let Some(active) = watcher.as_ref() {
                    match active.wait(delay) {
                        Ok(true) => {
                            watcher = match paths.watch_local(&config) {
                                Ok(watcher) => Some(watcher),
                                Err(error) => {
                                    eprintln!(
                                        "Observação local indisponível; polling mantido: {error:#}"
                                    );
                                    None
                                }
                            };
                        }
                        Ok(false) => {}
                        Err(error) => {
                            eprintln!("Observação local falhou; polling mantido: {error:#}");
                            watcher = None;
                            std::thread::sleep(delay);
                        }
                    }
                } else {
                    std::thread::sleep(delay);
                }
            }
        }
    }
    Ok(())
}
