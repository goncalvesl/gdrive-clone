# ADR 0002 — SQLite para estado e journal

Data: 2026-10-26. Estado: aceito pelo usuário.

## Decisão

Persistir metadados e operações em SQLite, usando rusqlite com SQLite bundled. Manter conteúdo de arquivos fora do banco; tokens não pertencem a este armazenamento. Um futuro cache por hash pode incorporar ideias do Git sem substituir o journal transacional.

A primeira implementação usa rollback journal DELETE e synchronous FULL. WAL fica para quando houver necessidade demonstrada de concorrência de leitores. O aplicativo prevê um único escritor responsável pela raiz. O wrapper Rust não adiciona unsafe próprio; SQLite bundled é uma dependência C.

## Consequências

É possível confirmar a base e concluir uma operação na mesma transação. Isso não inclui efeitos de filesystem ou API: o executor ainda precisa de precondições e inspeção após falhas. O schema é versionado por user_version; versões futuras desconhecidas são recusadas. A versão 1 cria o schema transacionalmente, sem migrar bancos desconhecidos silenciosamente.

## Alternativas

Uma estrutura de objetos imutáveis como a do Git pode ajudar cache/deduplicação, mas exigiria mecanismos adicionais para consultas e atualização consistente do estado operacional. Não manter histórico completo de conteúdo nesta entrega.

Referências consultadas: [rusqlite](https://docs.rs/rusqlite/0.41.0/rusqlite/struct.Connection.html), [PRAGMAs SQLite](https://www.sqlite.org/pragma.html), [Serde derive](https://serde.rs/derive.html) e [serde_json](https://docs.rs/serde_json/latest/serde_json/).
