# Roteiro

Atualizado em 2026-10-26. **Prévia executável de espelhamento**, com login/listagem reais relatados pelo usuário; transferências ainda não homologadas. Sem alegação de versão final ou paridade completa.

## Foco atual

Avançar da organização visual para as funcionalidades faltantes com suporte na API pública. A estrutura das preferências está implementada conforme [contrato visual](docs/specs/desktop-preferences.md). A investigação solicitada sobre [Computadores](docs/specs/computers.md) não identificou operação pública para registrar uma máquina nessa seção; não tratar essa capacidade como uma implementação comum pendente.

Entregues nesta etapa: pastas vazias, movimentos de diretórios, progresso por bytes, usuário/cota, upload em blocos com sessão persistida, retomada de downloads binários, retry com espera e pausa entre operações. Evidência local nas especificações de [diretórios](docs/specs/directories.md), [uploads](docs/specs/resumable-upload.md) e [downloads](docs/specs/resumable-download.md). Watcher local e atualização por ID de mudanças em arquivos comuns foram entregues conforme [contrato do inventário](docs/specs/incremental-inventory.md). Renomes/movimentos de pastas conhecidas já atualizam subárvores por ID; criação/remoção e pais desconhecidos ainda provocam varredura completa. O seletor distingue pastas homônimas pelo ID, mas o espelhamento de nomes duplicados ainda bloqueia o inventário até existir projeção segura. Próximos passos são política de nomes duplicados e várias pastas de sincronização. Exclusão de árvores continua bloqueada até existir garantia adequada para filhos concorrentes. Uma alternativa de várias pastas locais em Meu Drive precisa de contrato próprio e não equivale a aparecer em Computadores.

O fluxo de login no navegador e seleção por nomes permanece implementado. Login/listagem reais foram relatados pelo usuário, não reproduzidos pelo agente. Cliente OAuth de distribuição e homologação de transferências, renovação, arquivos grandes e interrupções continuam pendentes.

| Marco | Estado e evidência |
|---|---|
| M0 — domínio e simulação | Implementado; 11 testes do protótipo |
| M1 — journal e recuperação | Integrado ao executor; transações, migração, retomada e perda de processo testadas localmente; energia física/falhas reais Google pendentes |
| M2 — OAuth e Drive API | Implementado: PKCE, Secret Service, inventário e transferências; HTTP simulado testado; login/listagem reais relatados; transferências e renovação pendentes |
| M3 — espelhamento | Implementado para arquivos comuns e criação/movimento de pastas: criar/editar/renomear/mover/excluir e conflitos; 32 testes de integração com filesystem e SQLite, remoto simulado |
| M4 — desktop/segundo plano | UI Rust com login, navegador de pastas, seletores, progresso, dados da conta e pausa; CLI e watch implementados; instalador e template systemd preparados; serviço autenticado ainda não exercitado |
| M5 — streaming FUSE | Não implementado |
| M6 — distribuição e paridade ampliada | Prévia pública [v0.1.3](https://github.com/goncalvesl/gdrive-clone/releases/tag/v0.1.3) sob GPL-3.0-only com ID público do OAuth Desktop nos pacotes AppImage, deb, rpm e Arch x86_64. Workflow passou; os quatro artefatos foram baixados e inspecionados. Instalação em outras distribuições, verificação OAuth do Google, assinatura e Flatpak pendentes |

## Antes da versão estável

Executar [aceitação](docs/acceptance.md), corrigir resultados e medir inventários/transferências. Homologar referências Google e completar política de nomes duplicados e pastas vazias. Implementar streaming quando o espelhamento estiver homologado. Licença GPL-3.0-only autorizada pelo proprietário; conferir avisos e artefatos antes de cada publicação.

Métricas de código e testes não equivalem a fidelidade observada com o Drive oficial. A observação de uma versão identificada do cliente oficial permanece pendente.
