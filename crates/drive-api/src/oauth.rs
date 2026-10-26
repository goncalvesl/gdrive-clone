use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Clone, Deserialize)]
pub struct Credentials {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
}
#[derive(Deserialize)]
struct CredentialFile {
    installed: Credentials,
}
impl Credentials {
    pub fn load(path: &Path) -> Result<Self> {
        ensure!(
            std::fs::metadata(path)?.len() <= 1024 * 1024,
            "Arquivo OAuth grande demais; selecione o JSON Desktop"
        );
        let bytes = std::fs::read(path).context("Não foi possível ler o arquivo OAuth")?;
        let parsed: CredentialFile = serde_json::from_slice(&bytes)
            .context("Escolha um JSON OAuth do tipo Desktop (installed)")?;
        ensure!(!parsed.installed.client_id.is_empty(), "client_id ausente");
        Ok(parsed.installed)
    }
}
fn token_form<'a>(
    credentials: &'a Credentials,
    fields: &[(&'a str, &'a str)],
) -> Vec<(&'a str, &'a str)> {
    let mut form = Vec::with_capacity(fields.len() + 2);
    form.push(("client_id", credentials.client_id.as_str()));
    if !credentials.client_secret.is_empty() {
        form.push(("client_secret", credentials.client_secret.as_str()));
    }
    form.extend_from_slice(fields);
    form
}
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default = "expiry")]
    expires_in: u64,
}
fn expiry() -> u64 {
    3600
}
#[derive(Serialize, Deserialize)]
struct Secret {
    refresh_token: String,
}
fn secret_command(args: &[&str], profile: &str, input: Option<&str>) -> Result<String> {
    let mut command = Command::new("secret-tool");
    command
        .args(args)
        .args(["application", "gdrive-clone", "profile", profile]);
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    if input.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = command
        .spawn()
        .context("Instale libsecret/secret-tool e desbloqueie o chaveiro da sessão")?;
    if let Some(input) = input {
        let mut stdin = child
            .stdin
            .take()
            .context("Canal do chaveiro indisponível")?;
        stdin.write_all(input.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "Chaveiro indisponível ou conta desconectada"
    );
    String::from_utf8(output.stdout).context("Resposta inválida do chaveiro")
}
fn save(profile: &str, refresh_token: String) -> Result<()> {
    let secret = serde_json::to_string(&Secret { refresh_token })?;
    secret_command(
        &["store", "--label=Google Drive para Linux"],
        profile,
        Some(&secret),
    )?;
    Ok(())
}
pub fn connected(profile: &str) -> bool {
    secret_command(&["lookup"], profile, None).is_ok()
}
pub fn login(profile: &str, credentials: &Credentials) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let redirect = format!(
        "http://127.0.0.1:{}/callback",
        listener.local_addr()?.port()
    );
    let state = Uuid::new_v4().simple().to_string();
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut url = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")?;
    url.query_pairs_mut().extend_pairs([
        ("client_id", credentials.client_id.as_str()),
        ("redirect_uri", redirect.as_str()),
        ("response_type", "code"),
        ("scope", "https://www.googleapis.com/auth/drive"),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("state", state.as_str()),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
    ]);
    ensure!(
        Command::new("xdg-open")
            .arg(url.as_str())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success(),
        "Não foi possível abrir o navegador"
    );
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_read_timeout(Some(Duration::from_secs(3)))?;
                let mut request = Vec::new();
                let mut byte = [0u8; 1];
                while request.len() < 8192 {
                    if stream.read(&mut byte)? == 0 {
                        break;
                    }
                    request.push(byte[0]);
                    if request.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                let line = std::str::from_utf8(&request)
                    .context("Callback inválido")?
                    .lines()
                    .next()
                    .unwrap_or("");
                let path = line
                    .strip_prefix("GET ")
                    .and_then(|l| l.split_whitespace().next())
                    .unwrap_or("");
                let parsed = url::Url::parse(&format!("http://127.0.0.1{path}"))?;
                let fields: std::collections::BTreeMap<_, _> =
                    parsed.query_pairs().into_owned().collect();
                let valid = parsed.path() == "/callback" && fields.get("state") == Some(&state);
                let body = if valid {
                    "Login recebido. Volte ao aplicativo."
                } else {
                    "Callback invalido."
                };
                let response = format!(
                    "HTTP/1.1 {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    if valid { "200 OK" } else { "400 Bad Request" },
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
                if !valid {
                    continue;
                }
                let code = fields.get("code").context("Login recusado ou cancelado")?;
                let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
                let response = client
                    .post("https://oauth2.googleapis.com/token")
                    .form(&token_form(
                        credentials,
                        &[
                            ("code", code.as_str()),
                            ("code_verifier", verifier.as_str()),
                            ("redirect_uri", redirect.as_str()),
                            ("grant_type", "authorization_code"),
                        ],
                    ))
                    .send()
                    .map_err(|_| {
                        anyhow::anyhow!("Falha ao trocar autorização; tente conectar novamente")
                    })?;
                ensure!(
                    response.status().is_success(),
                    "Google recusou autorização; revise cliente OAuth e usuário de teste"
                );
                let token: TokenResponse = response.json().context("Resposta OAuth inválida")?;
                save(
                    profile,
                    token.refresh_token.context(
                        "Google não retornou refresh token; reconecte com consentimento",
                    )?,
                )?;
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(e) => return Err(e.into()),
        }
    }
    bail!("Tempo de login esgotado; tente novamente")
}
pub fn disconnect(profile: &str) -> Result<()> {
    // Delete locally even when offline. Google permissions can also be revoked in account settings.
    secret_command(&["clear"], profile, None)?;
    Ok(())
}
pub struct Session {
    profile: String,
    credentials: Credentials,
    token: Option<(String, Instant)>,
}
impl Session {
    pub fn new(profile: String, credentials: Credentials) -> Self {
        Self {
            profile,
            credentials,
            token: None,
        }
    }
    pub fn bearer(&mut self, client: &Client) -> Result<String> {
        if let Some((token, until)) = &self.token
            && Instant::now() < *until
        {
            return Ok(token.clone());
        }
        let secret: Secret =
            serde_json::from_str(&secret_command(&["lookup"], &self.profile, None)?)
                .context("Credencial inválida no chaveiro")?;
        let response = client
            .post("https://oauth2.googleapis.com/token")
            .form(&token_form(
                &self.credentials,
                &[
                    ("refresh_token", secret.refresh_token.as_str()),
                    ("grant_type", "refresh_token"),
                ],
            ))
            .send()
            .map_err(|_| super::ApiFailure::Network("Sem conexão com o serviço de autenticação"))?;
        if response.status().as_u16() == 429 || response.status().is_server_error() {
            return Err(super::ApiFailure::Http {
                status: response.status().as_u16(),
                retry_after: response
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok()),
            }
            .into());
        }
        ensure!(
            response.status().is_success(),
            "Conta desconectada ou autorização expirada; conecte novamente"
        );
        let token: TokenResponse = response.json().context("Resposta OAuth inválida")?;
        if let Some(refresh) = token.refresh_token {
            save(&self.profile, refresh)?;
        }
        self.token = Some((
            token.access_token.clone(),
            Instant::now() + Duration::from_secs(token.expires_in.saturating_sub(60).min(86400)),
        ));
        Ok(token.access_token)
    }
}

#[cfg(test)]
impl Session {
    pub(crate) fn synthetic() -> Self {
        Self {
            profile: "test".into(),
            credentials: Credentials {
                client_id: "synthetic".into(),
                client_secret: "synthetic".into(),
            },
            token: Some((
                "synthetic-access-token".into(),
                Instant::now() + Duration::from_secs(3600),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CredentialFile, token_form};

    #[test]
    fn desktop_client_without_secret_omits_secret_from_token_forms() -> anyhow::Result<()> {
        let parsed: CredentialFile =
            serde_json::from_str(r#"{"installed":{"client_id":"synthetic-client-id"}}"#)?;
        let fields = [("grant_type", "refresh_token")];
        let form = token_form(&parsed.installed, &fields);
        assert!(form.contains(&("client_id", "synthetic-client-id")));
        assert!(form.contains(&("grant_type", "refresh_token")));
        assert!(!form.iter().any(|(key, _)| *key == "client_secret"));
        Ok(())
    }

    #[test]
    fn existing_desktop_client_keeps_secret_in_token_forms() -> anyhow::Result<()> {
        let parsed: CredentialFile = serde_json::from_str(
            r#"{"installed":{"client_id":"synthetic-client-id","client_secret":"synthetic-secret"}}"#,
        )?;
        assert!(
            token_form(&parsed.installed, &[]).contains(&("client_secret", "synthetic-secret"))
        );
        Ok(())
    }
}
