# Arquitetura implementada

Atualizado em 2026-10-26. Prévia de espelhamento; login/listagem reais relatados pelo usuário, transferências reais pendentes. Interfaces e CLI usam os mesmos comandos de `engine`.

| Componente | Implementação | Dependências internas |
|---|---|---|
| `sync-core` | Tipos e planejador M0; contrato ativo em `mirror` para arquivos reais | Nenhuma |
| `state-store` | SQLite v1 do protótipo e migrações até v5 para links/journal/histórico, sessões de upload e cache/cursor remoto | `sync-core` |
| `local-fs` | Inventário, MD5, device/inode, diretórios ancorados, staging, quarentena e watcher inotify | `sync-core` |
| `drive-api` | OAuth PKCE, Secret Service, Drive v3, paginação, metadados, uploads/downloads/lixeira | `sync-core` |
| `engine` | Configuração XDG, lock, ciclos, journal, conflitos, inspeção e recuperação | Os quatro anteriores |
| `drive-desktop` | Interface egui/eframe + subcomandos CLI e `watch` | `engine`, `drive-api` |
| `virtual-fs` | Não implementado; FUSE/streaming permanecem no roteiro | — |

A UI não importa o banco nem replica decisões do planejador. Trabalho de rede/disco ocorre em worker separado; o CLI usa os mesmos métodos. Cada ciclo adquire lock exclusivo por perfil para impedir dois executores simultâneos. `watch` é o processo de segundo plano, com template systemd de usuário. Não há servidor IPC/MCP.

## Reconciliação ativa

`sync-core::mirror::reconcile` compara Link confirmado, entrada local e remota. Entry carrega tipo arquivo/diretório, caminho, hash MD5, tamanho e identidade (dev:inode no Linux, versão no Drive). MD5 é usado para compatibilidade com o checksum dos arquivos binários do Drive, não como garantia criptográfica.

O ciclo faz inventário remoto completo antes de planejar; erro, paginação incompleta ou nomes duplicados/inseguros interrompem. Documentos Google são projetados como referências JSON que abrem no navegador; renomeação modifica metadados, sem upload do JSON como conteúdo do documento. Em seguida inspeciona intenções pendentes, faz inventário local e planeja os objetos. Renomeações locais são associadas por device/inode; editores que substituem o inode são reconhecidos pelo caminho anterior. Hashes e nomes decidem mudanças, não timestamps isolados.

Arquivos iniciais de mesmo caminho só são associados se conteúdo e tamanho coincidirem. IDs de novos arquivos remotos são reservados antes de persistir a intenção; o mesmo ID é consultado após timeout. Pastas vazias são objetos do journal. Criação e movimento precedem os arquivos, com novo inventário após cada efeito estrutural. Pais não são criados implicitamente fora do journal. Exclusão de diretórios e conflitos bloqueiam a árvore.

Ações runtime: Upload, Download, TrashRemote, QuarantineLocal, Checkpoint, Forget e KeepBoth (esta última é um comando explícito de resolução). Conflitos sem resolução não alteram os arquivos. A base avança inclusive quando os dois lados mudaram para um mesmo estado.

## Efeitos e recuperação

A intenção é persistida antes do efeito. Ao reiniciar, o executor consulta os lados antes de repetir. Efeito já observado conclui a intenção; estado ainda igual às precondições permite nova tentativa com o mesmo ID. Divergência bloqueia. O comando recover inspeciona novamente e pode aposentar a intenção conservando a base anterior; um ID criado sem acesso continua bloqueado.

Downloads binários retomam do staging privado por Range e usam checksum antes da instalação. A versão local anterior é movida para recuperação; instalação usa rename sem substituir um destino criado concorrentemente. Uploads usam snapshots privados estáveis, removidos após sucesso. A lixeira do Drive recebe exclusões remotas. Não há limpeza automática de backups locais.

## API e limites

Reqwest blocking em threads dedicadas, TLS e timeout. Login OAuth por navegador, PKCE e callback em 127.0.0.1; refresh token no Secret Service via secret-tool, access token em memória. Nenhum token no SQLite. Updates/lixeira exigem ETag e usam If-Match; a semântica real ainda deve ser homologada, especialmente nas sessões resumable.

O serviço observa eventos locais com inotify e consulta Changes periodicamente. Sem alterações, reutiliza o inventário SQLite; eventos de arquivos atualizam IDs e caminhos; renomeações/movimentos de pastas conhecidas reescrevem caminhos dos descendentes. Criação/remoção de pastas, metadados incompletos e colisões disparam inventário completo. Sessões/offsets de upload são persistidos no SQLite v5 e revalidados no servidor; envio em blocos de 8 MiB. FUSE, integração por ícones no gerenciador de arquivos, múltiplas contas simultâneas, edição offline de documentos Google e shared drives não existem ainda.

Detalhes: [runtime SQLite](specs/state-store.md), [espelhamento](specs/mirror.md), [aceitação](acceptance.md).

## Identidade da conexão e seleção

`Account` guarda perfil de chaveiro e caminho do cliente Desktop em `account.json`. `Config` guarda raízes e perfil de journal, além de `account_profile` opcional e nome remoto de apresentação. Configurações antigas usam o perfil do journal como identidade do chaveiro. Reconfigurar raízes conserva a conta se o caminho do cliente não mudar; journals permanecem independentes. A troca bloqueia com intenções pendentes e mantém o lock do perfil anterior até a persistência. O executor revalida o perfil após adquirir seu lock.

O cliente OAuth é fornecido pelo responsável pela distribuição, fora do repositório, segundo [ADR 0004](adr/0004-login-e-selecao.md). Login pode preceder qualquer raiz de sincronização. UI/CLI chamam os comandos da engine; a navegação usa IDs e só muda a trilha após receber uma listagem completa. Seletores nativos opcionais executam zenity/kdialog sem shell numa thread de trabalho.

## Apresentação das preferências

A UI abre a página Google Drive e organiza as preferências em Meu computador, Google Drive e Configurações. Widgets de funções ausentes permanecem explicitamente desativados. Seleções de pasta ficam em memória até o comando configure_named; cancelamento restaura o snapshot de opções sem escrever configuração. Conta e ações de sincronização mantêm comandos separados da engine. Referências, capturas e limites em [preferências do desktop](specs/desktop-preferences.md).

A engine injeta um UploadSessionStore na API usando uma segunda conexão SQLite do mesmo perfil. Transações do journal são curtas e não permanecem abertas durante HTTP. A gravação de sessão exige intenção running, e finish elimina a sessão junto com a confirmação. O adaptador não depende do banco. ProgressSink transmite somente telemetria por um canal não bloqueante para a UI; 100% não confirma o efeito. about.get fornece usuário/cota, mantidos em memória. Falhas tipadas permitem retry sem serializar URLs de sessão ou headers de autorização. Especificações: [diretórios](specs/directories.md) e [uploads](specs/resumable-upload.md).
