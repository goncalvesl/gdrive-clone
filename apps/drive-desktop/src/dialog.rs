//! Optional native pickers. No shell, and no effect on the sync engine.
use anyhow::{Context, Result, bail, ensure};
use std::{
    io,
    path::PathBuf,
    process::{Command, Stdio},
};

#[derive(Clone, Copy)]
pub enum Target {
    Local,
    Credentials,
}
pub fn choose(target: Target) -> Result<Option<PathBuf>> {
    let directory = matches!(target, Target::Local);
    let title = if directory {
        "Escolher pasta local"
    } else {
        "Cliente OAuth de desenvolvimento"
    };
    let mut zenity = Command::new("zenity");
    zenity.args(["--file-selection", "--title", title]);
    if directory {
        zenity.arg("--directory");
    } else {
        zenity.arg("--file-filter=JSON | *.json");
    }
    let result = zenity.stdout(Stdio::piped()).stderr(Stdio::null()).output();
    let output = match result {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut kdialog = Command::new("kdialog");
            kdialog.args(["--title", title]);
            if directory {
                kdialog.args(["--getexistingdirectory", "."]);
            } else {
                kdialog.args(["--getopenfilename", ".", "*.json"]);
            }
            kdialog.stdout(Stdio::piped()).stderr(Stdio::null()).output()
                .context("O seletor requer zenity ou kdialog. Você também pode digitar o caminho no campo.")?
        }
        Err(error) => return Err(error.into()),
    };
    if output.status.code() == Some(1) {
        return Ok(None);
    }
    ensure!(
        output.status.success(),
        "Não foi possível abrir o seletor de arquivos"
    );
    let path = String::from_utf8(output.stdout).context("Caminho não UTF-8 não suportado")?;
    let path = path.strip_suffix('\n').unwrap_or(&path);
    if path.is_empty() {
        bail!("O seletor não retornou um caminho");
    }
    let path = PathBuf::from(path);
    ensure!(
        path.is_absolute(),
        "O seletor deve retornar um caminho absoluto"
    );
    Ok(Some(path))
}
