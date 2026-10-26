# Mapa da documentação

| Documento | Fonte de verdade para |
|---|---|
| [Guia de uso](user-guide.md) | Configuração, instalação, recuperação e operação |
| [Distribuição](distribution.md) | Pacotes, prévia pública, licença e limites do login |
| [Política de Privacidade pública](privacy/index.html) | Uso, armazenamento e controle dos dados do Google Drive |
| [Aceitação](acceptance.md) | Homologação pendente antes de versão estável |
| [Espelhamento runtime](specs/mirror.md) | Contrato ativo no desktop e CLI |
| [Arquitetura](architecture.md) | Componentes, dependências e estado implementado |
| [Desenvolvimento](development.md) | Ambiente e comandos realmente disponíveis |
| [Contribuição](contributing.md) | Como preparar uma mudança revisável |
| [Paridade](target-app-parity.md) | Contratos do produto e evidências |
| [Lacunas](gaps.md) | Pendências priorizadas |
| [Estado SQLite](specs/state-store.md) | Journal persistente, transações e limites |
| [ADR runtime](adr/0003-runtime-desktop.md) | Toolkit, adapters e limites da prévia |
| [ADR SQLite](adr/0002-sqlite.md) | Escolha de armazenamento |
| [Planejador M0](specs/planner.md) | Contrato implementado e limites do simulador |
| [Testes](testing.md) | Cenários e critérios de aceitação |
| [Segurança da sincronização](specs/sync-safety.md) | Invariantes de preservação e recuperação |
| [Controle](control-protocol.md) | Contrato proposto para CLI e automação |
| [Decisão inicial](adr/0001-direcao-tecnica.md) | Direção técnica aceita e decisões abertas |
| [Fontes](reference/sources.md) | Pesquisa, links e limites de conhecimento |
| [Registro de trabalho](../log/devlog.md) | Passagem de contexto entre sessões |

Ao alterar um contrato, atualize sua fonte de verdade e os links dependentes. Não copie o mesmo estado em vários roteiros. Propostas devem permanecer identificadas até a implementação e sua validação.
