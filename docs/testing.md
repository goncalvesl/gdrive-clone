# Validação

Atualizado em 2026-10-26. Suíte automatizada com arquivos sintéticos, SQLite real e HTTP simulado. Login e listagem reais relatados pelo usuário, não reproduzidos pelo agente. Operações de transferência Google reais ainda não executadas. Há um teste helper que encerra subprocesso intencionalmente com código 23.

## Cobertura implementada

- sync-core/tests/reconciliation.rs: 11 testes do protótipo de planejamento e simulação.
- state-store/tests/journal.rs: 12 entradas, incluindo helper; fases, reabertura, processo sem destructors, rollback e migrações até v5, sessões ligadas a intenção running, limpeza atômica e cache/cursor de inventário transacional com migração v4→v5.
- local-fs/tests/safety.rs: 11 testes de contenção de caminhos, symlinks, identidade da raiz, checksum, preservação, spool, staging parcial, watcher inotify recursivo e movimentos de diretórios sem substituir destinos.
- engine/tests/mirror.rs: 33 testes end-to-end locais; criação, edição, conflitos/KeepBoth, rename, exclusões, origem alterada, timeout após aceite remoto, retomada de download após reinício e quarentena e checkpoints, proteção de referências Google, árvores vazias, movimentos de pastas e falhas antes/depois de efeitos, colisões, bloqueio de descendentes e pausa entre arquivos.
- engine/tests/config.rs: 11 testes de privacidade/configuração, identidade do perfil, conexão antes das raízes, compatibilidade do chaveiro antigo, cliente gerenciado, bloqueio de troca com pendências, lock do executor, diretórios proibidos, pausa e política de espera injetável.
- drive-api: 43 testes HTTP/telemetria: paginação, status de acesso/lixeira, nomes inseguros, quotas, IDs reservados, corpo de upload e envio de If-Match, referências nativas, colisão de projeção, URL segura, seleção paginada de pastas por ID e rejeição de listagens incompletas/inacessíveis, pasta com ID reservado/If-Match, usuário/cota, progresso sem bytes fictícios, upload em blocos, retomada com Range autoritativo, expiração, Range inválido, URL de sessão hostil, arquivo vazio, Retry-After e proteção de URL privada no erro da resposta final, paginação de Changes, cache sem alterações, atualização delta de arquivos e subárvores por ID, fallback estrutural/em colisão, rebase após rejeição de cursor e cursor não avançado em inventário falho.

```sh
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Asserções inspecionam bytes, base e intenções, não apenas retorno de sucesso. A permissão de upload HTTP local é compilada somente nos testes; produção exige hosts Google HTTPS permitidos.

- drive-desktop/src/ui/tests.rs: 4 regressões egui sem rede/chaveiro, cobrindo navegação entre as páginas principais sem alterar configuração e cancelamento das opções editadas sem criar nova raiz ou iniciar sincronização, espera de retry e separação entre barra completa e confirmação do ciclo.

## Validação visual e instalação

O desktop foi aberto e capturado com perfil XDG descartável; [captura](screenshots/setup.png). O instalador tem modo sem build e raiz de teste GDRIVE_INSTALL_HOME para verificar arquivos sem instalar na sessão real. O template systemd não é iniciado automaticamente.

## Pendências para versão estável

Executar [aceitação real](acceptance.md), incluindo OAuth, If-Match/ETag, quotas, arquivo grande, token expirado/revogado, corte de conexão e serviço autenticado. Testar também disco cheio real, energia física e distribuições diferentes. MD5 e igualdade de metadados não substituem teste real de interoperabilidade.

Performance ainda sem orçamento medido. Inventários completos são periódicos, então medir volume, hardware e versão antes de anunciar limites de escala. Streaming/FUSE e demais funcionalidades ausentes não entram como testes aprovados por existência de stubs.
