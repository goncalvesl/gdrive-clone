# engine

Comandos compartilhados por UI, CLI e watch. Paths gerencia configuração XDG, perfil, lock, status, login, sincronização, conflitos e recuperação. synchronize é o ciclo injetável, com Root real, Store real e Remote substituível.

Planeja por sync-core::mirror. Recupera intenções antes de planejar novos efeitos. Links usam IDs remotos; device/inode detecta renomeações locais. resolve_both preserva uma cópia de conflito visível antes de instalar a versão remota. replan_pending é uma ação explícita para inspecionar pendências divergentes conservando a base.

```sh
cargo test -p engine
```

Os testes não usam conta real: remoto simulado, arquivos locais e SQLite temporários. O desktop também não tem FUSE ou inventário incremental; ver o roteiro.

Diretórios são reconciliados antes dos arquivos, com criação/movimento e bloqueio de exclusão de árvores. synchronize_with_pause consulta pausa entre efeitos. cycle_with_progress compartilha telemetria e runtime com a GUI; retry_delay recebe contagem e mínimo do servidor.
