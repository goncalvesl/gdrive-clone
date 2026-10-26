# sync-core

Planejador puro e simulador em memória para uma raiz sintética. Sem acesso a rede/disco. Serde fornece serialização dos tipos persistidos pelo state-store.

API: `Snapshot`, `Inventory`, `Observation`, `FileState`, `plan`, `Plan::decisions` e `simulate`. Veja o [contrato](../../docs/specs/planner.md) para regras e limites.

```sh
cargo test -p sync-core
cargo run -p sync-core --example simulate
```

O exemplo imprime um upload sintético e confirma que não resta transferência após a simulação. Não conecta ao Google Drive.

O módulo `mirror` contém o contrato puro ativo no desktop: Entry, Link, Operation, Kind e reconcile. Ele cobre decisões de transferência, checkpoint, exclusões recuperáveis e conflitos; não acessa rede/disco. Veja [espelhamento runtime](../../docs/specs/mirror.md).
