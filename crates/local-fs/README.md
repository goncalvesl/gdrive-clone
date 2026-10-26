# local-fs

Adaptador Linux ancorado em descritor de diretório, com Rust seguro/rustix. Root vincula dev:inode; scan retorna arquivos regulares com caminhos, MD5, tamanho e identidade. A enumeração usa /proc/self/fd; não é um adaptador portátil para outros sistemas.

LocalWatcher usa inotify recursivo como sinal para antecipar ciclos; eventos não são fonte de verdade e a engine ainda faz inventários completos. A pasta de recuperação é ignorada pelo watcher. validate_path bloqueia travessia/reservas. parent/read usam NOFOLLOW para componentes internos. Symlinks, hard links, submontagens, nomes não UTF-8 e arquivos especiais bloqueiam inventário. Downloads binários retomam do staging privado e verificam tamanho/checksum antes da instalação por rename NOREPLACE. Quarentena preserva originais em .gdrive-clone-recovery. Spool cria snapshot privado para upload; cleanup_upload só é chamado após confirmação do resultado.

```sh
cargo test -p local-fs
```

Cópias antigas não expiram automaticamente. Essa retenção deve ter política própria antes de uso em larga escala.
