# Desenvolvimento

Rust 2024, mínimo declarado 1.99; validado com rustc/cargo 1.99.1 em Linux. Workspace com cinco bibliotecas e uma aplicação. Cargo.lock fixa as versões resolvidas.

Dependências principais: Serde/serde_json, rusqlite 0.41 bundled, reqwest 0.13, rustix 1, md-5 0.10, sha2, uuid, fs2, clap e egui/eframe 0.33.3 (glow, X11/Wayland). SQLite bundled exige compilador C. A UI requer bibliotecas gráficas da distribuição.

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --locked --release -p drive-desktop
cargo run -p drive-desktop -- --help
cargo run -p drive-desktop
```

`./scripts/check.sh` executa os quatro checks de entrega. Não há xtask. `./scripts/install.sh` compila e instala no usuário; `--no-build` reutiliza o binário de release existente. Não ativa o serviço.

Receitas de AppImage, deb, rpm e Arch e fluxo de publicação: [distribuição](distribution.md). A licença do projeto é GPL-3.0-only; dependências conservam as suas próprias licenças.

O exemplo M0 permanece disponível: `cargo run -p sync-core --example simulate`. Não acessa arquivos/rede de sincronização.

## Testes

Os testes usam diretórios temporários, arquivos sintéticos, bancos SQLite e servidores HTTP de loopback. Um helper encerra propositalmente um subprocesso com exit 23 para testar journal. Não usam o chaveiro nem login real. A permissão de HTTP de upload em loopback existe apenas sob cfg(test); o binário exige HTTPS nos hosts Google permitidos.

## Inspeção visual

```sh
cargo run -p drive-desktop -- --screenshot /tmp/drive-setup.png
cargo run -p drive-desktop -- --screenshot /tmp/drive-computer.png --screenshot-page computer
cargo run -p drive-desktop -- --screenshot /tmp/drive-settings.png --screenshot-page settings
```

Esse modo abre a interface, captura PNG após renderização e encerra. Usar XDG_CONFIG_HOME/XDG_DATA_HOME descartáveis para evitar incluir dados pessoais na imagem. A captura de configuração inspecionada fica em [screenshots/setup.png](screenshots/setup.png).

## Regras de mudanças

Novas crates herdam os lints do workspace: unsafe proibido no código próprio e padrões explícitos de panic negados. Dependências podem internamente conter C/unsafe. Testes permitem unwrap para diagnóstico; produção retorna erros. Não copiar regras da UI para CLI.

Tipos persistidos usam JSON versionado pelo schema SQLite; alterações exigem migração. A migração v1 -> v2 preserva o protótipo M0 e adiciona tabelas runtime numa transação. O perfil é single-writer por lock. Atualizar ADRs, especificações, matriz de paridade e devlog com evidências, sem declarar testes mock como homologação externa.

Configuração real e credenciais: [guia de uso](user-guide.md). Gate de versão estável: [aceitação](acceptance.md).

## Cliente OAuth fornecido pela compilação

O fluxo de produto usa login no navegador; criar o cliente OAuth é tarefa do responsável pelo projeto. A árvore não inclui credenciais reais. Em desenvolvimento, use a seção recolhida da interface ou `drive-desktop login --oauth /caminho/cliente-desktop.json`, sem configurar raízes previamente. Não registrar o conteúdo desse arquivo.

Para preparar uma instalação com cliente do projeto, forneça `GDRIVE_OAUTH_CLIENT_FILE=/caminho/fora/do/repositorio/cliente.json` ao executar `scripts/install.sh`. O instalador copia para `~/.local/share/gdrive-clone/oauth-client.json` com permissão 0600; o binário procura esse arquivo relativo ao seu prefixo. Também aceita arquivo gerenciado no diretório XDG de configuração e configurações anteriores. A variável pode ser usada em execução com um caminho absoluto. Veja a ordem completa no [ADR 0004](adr/0004-login-e-selecao.md).

Seletores locais opcionais usam `zenity` ou `kdialog`; ambos são invocados sem shell e cancelamento preserva os campos. Sem as ferramentas, informe o caminho manualmente. Os checks não abrem o navegador, o chaveiro ou seletores interativos.

As capturas abrem a página indicada sem iniciar login/listagem/sincronização. `--screenshot-page` requer `--screenshot`; valores `drive` (padrão), `computer` e `settings`. Use dados sintéticos em qualquer perfil de captura. Estrutura e limites de comparação com a referência estão no [contrato visual](specs/desktop-preferences.md).
