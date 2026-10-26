# state-store

Base confirmada e journal SQLite para um perfil/raiz de sincronização. Não executa efeitos de sincronização.

API: `Store::open`, `enqueue`, `start`, `observe`, `commit`, `pending`, `confirmed`. Veja o [contrato](../../docs/specs/state-store.md).

```sh
cargo test -p state-store
```

Dependências: sync-core, rusqlite bundled e serde_json. Os testes usam bancos temporários, falhas SQL e um subprocesso de interrupção.

O módulo mirror implementa o journal runtime v5, com diretórios, sessões de upload e cache/cursor remoto, usado pelo engine; as APIs originais do protótipo v1 continuam disponíveis para compatibilidade dos testes.
