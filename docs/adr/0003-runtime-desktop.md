# ADR 0003 — Runtime de espelhamento e desktop Rust

Data: 2026-10-26. Estado: adotado na implementação da prévia, dentro do pedido de versão utilizável.

Escolhido egui/eframe para manter a UI em Rust, com OpenGL e suporte X11/Wayland. Reqwest blocking roda em worker threads; o domínio continua puro. Um único executável contém UI, CLI e watch, compartilhando engine; não há motor duplicado nem dependência de servidor web.

A primeira entrega usa inventários completos periódicos, arquivos regulares e subpastas necessárias. Isso facilita validar a reconciliação/recuperação antes de acrescentar watcher, cursor incremental e FUSE. Documentos Google são referências JSON de navegador, sem edição offline. Tipos locais incompatíveis e nomes inseguros bloqueiam a raiz inteira para que uma omissão não se torne exclusão.

O adaptador local usa descritores, NOFOLLOW e rename sem sobrescrita via rustix. Substituições guardam a versão anterior em recuperação, e upload usa uma cópia estável privada. Exclusões são opt-in com lixeira/quarentena; nenhum cleanup automático de backups antigos foi adicionado.

Consequências: maior custo de inventário, sem progresso por bytes, sem pastas vazias ou streaming. A versão permanece uma prévia para homologação; esses limites não equivalem à meta de paridade completa. Conta real é necessária para validar OAuth/ETag/transferências. Empacotamento público e escolha de licença permanecem pendentes do proprietário.

## Evolução posterior

Os limites de pastas vazias, progresso e offsets desta decisão inicial foram parcialmente superados pelo [ADR 0005](0005-diretorios-e-transferencias.md). Preservar esta entrada como registro do estado original; o contrato ativo e os limites restantes estão no roteiro.
