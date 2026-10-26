# drive-desktop

Aplicação nativa egui/eframe sobre OpenGL, com CLI clap e modo watch para segundo plano. Regras de sincronização pertencem ao engine.

```sh
cargo run -p drive-desktop
cargo run -p drive-desktop -- --help
```

A UI executa efeitos em worker thread; o timer inicia ciclos a cada 30s enquanto habilitado. watch é um processo independente do desktop. Ambos compartilham lock e sinalizador de pausa, sem IPC. Evitar habilitar os dois timers simultaneamente; o lock protege o estado, mas um cliente pode retornar ocupado.

Configuração e aceitação: [guia de uso](../../docs/user-guide.md). Captura visual: --screenshot CAMINHO.png. Não incluir dados pessoais nas capturas.
