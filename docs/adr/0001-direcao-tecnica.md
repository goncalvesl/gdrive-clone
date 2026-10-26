# ADR 0001 — Cliente Linux em Rust com motor próprio

Data: 2026-10-26. Estado: aceito no escopo abaixo, conforme orientação do usuário.

## Contexto

O objetivo é aproximar os fluxos do Drive para computador no Linux. O usuário escolheu uma implementação Rust sem rclone e pediu uma organização de documentação voltada a IA inspirada na ArtCraft.

## Decisão

Usar Rust para o produto, inclusive a interface, e API pública do Google para comunicação. Separar domínio, efeitos, persistência e apresentação. Começar por espelhamento; implementar streaming depois. Registrar instruções em AGENTS.md, contratos em especificações, evidências na matriz de paridade e continuidade no devlog versionado.

## Consequências

O projeto assume a complexidade de conflitos, recuperação e transferências. A interface não pode esconder lacunas do motor. Paridade exata pode ser limitada pelas APIs públicas e precisa ser comprovada por cenário.

Não usar rclone, Electron, Tauri ou UI JavaScript. Crates de infraestrutura Rust continuam permitidas; motor próprio não exige reescrever HTTP, OAuth ou SQLite.

## Alternativas e decisões abertas

Rclone foi descartado por preferência explícita do usuário. Toolkit gráfico, dependências e licença de distribuição permanecem abertos. A opção por devlog versionado adapta a abordagem pública do PhotoCraft para que a continuidade não dependa de arquivos locais ignorados pelo Git.
