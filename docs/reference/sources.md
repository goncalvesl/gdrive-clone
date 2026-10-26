# Fontes e limites da pesquisa

Consultadas em 2026-10-26. Links para documentação upstream são mutáveis; conferir novamente ao implementar. Nenhum código ou recurso do PhotoCraft foi incorporado.

## Organização da ArtCraft / PhotoCraft

- [AGENTS.md](https://github.com/storytold/photocraft/blob/main/AGENTS.md): porta de entrada para agentes, mapa de leitura e regras de contribuição.
- [Arquitetura](https://github.com/storytold/photocraft/blob/main/docs/architecture.md): separação entre desenho original e estado implementado.
- [Contribuição](https://github.com/storytold/photocraft/blob/main/docs/contributing.md): orientações públicas para mudanças.
- [Controle](https://github.com/storytold/photocraft/blob/main/docs/control-protocol.md): automação do aplicativo.
- [Roteiro](https://github.com/storytold/photocraft/blob/main/ROADMAP.md): avaliação de progresso e evidências.

Essas referências inspiram a organização; não estabelecem o algoritmo deste projeto. Os padrões compartilhados em `craftrules` são descritos pelo upstream como privados e não foram consultados. Não conhecemos o histórico completo de prompts nem inferimos que esses documentos reproduzem todo o processo interno da equipe. Os textos locais são originais.

## Integração Google

- [Sistemas suportados](https://support.google.com/drive/answer/2375082): disponibilidade oficial do cliente desktop.
- [OAuth para desktop](https://developers.google.com/identity/protocols/oauth2/native-app): autenticação instalada, PKCE e callback local.
- [Mudanças incrementais](https://developers.google.com/workspace/drive/api/guides/manage-changes): cursores e consulta de alterações.
- [Uploads](https://developers.google.com/workspace/drive/api/guides/manage-uploads): modalidades de transferência, incluindo retomada.

Antes de M2, revisar escopos e verificação OAuth, limites/quota e detalhes de arquivos compartilhados na documentação oficial. A documentação da API não demonstra equivalência com o cliente oficial; observação comportamental ainda está pendente.

## Dependências da prévia

- [reqwest blocking](https://docs.rs/reqwest/0.13.5/reqwest/blocking/struct.Client.html)
- [rustix renameat_with](https://docs.rs/rustix/1.1.5/rustix/fs/fn.renameat_with.html)
- [eframe App](https://docs.rs/eframe/0.33.3/eframe/trait.App.html)
- [MD5 RustCrypto](https://docs.rs/md-5/0.10.6/md5/)
- [Drive performance e PATCH](https://developers.google.com/workspace/drive/api/guides/performance)

A documentação pública não substitui homologar a semântica de If-Match/ETag nos endpoints de upload usados. Não declarar essa interoperabilidade como verificada com base nos mocks.

## Login e seleção — consulta de 2026-10-26

- [OAuth 2.0 para Desktop](https://developers.google.com/identity/protocols/oauth2/native-app) e [aplicativos instalados](https://developers.google.com/identity/protocols/oauth2/#installed): cliente próprio do aplicativo fornecido pelo desenvolvedor, autorização no navegador, PKCE e callback loopback.
- [Escopos da Drive API](https://developers.google.com/workspace/drive/api/guides/api-specific-auth): acesso amplo de sincronização e requisitos de verificação de OAuth.
- Decisão local: [ADR 0004](../adr/0004-login-e-selecao.md). As fontes são documentação pública; não houve inspeção do código ou recursos proprietários do cliente oficial.

## Preferências visuais — consulta de 2026-10-26

- [Streaming e espelhamento](https://support.google.com/drive/answer/13401938?hl=pt-BR) e [configurações](https://support.google.com/drive/answer/13470231?hl=pt-BR): organização documentada das preferências.
- [Captura pública em discussão da Comunidade Google Drive](https://support.google.com/drive/thread/154970342/configured-to-mirror-but-mirroring-not-happening?hl=en): referência visual consultada sem incorporar a imagem. Plataforma visível Windows, build desconhecido. Não corresponde a observação de uma instalação oficial atual.
- Resultado local e limites: [preferências do desktop](../specs/desktop-preferences.md). Ícones e desenhos próprios, sem novos recursos de terceiros incorporados.

## Registro em Computadores — consulta de 2026-10-26

- [Referência REST v3](https://developers.google.com/workspace/drive/api/reference/rest/v3), [files.create](https://developers.google.com/workspace/drive/api/reference/rest/v3/files/create), [pastas](https://developers.google.com/workspace/drive/api/guides/folder) e [Discovery v3](https://www.googleapis.com/discovery/v1/apis/drive/v3/rest), revisão retornada 20261020: nenhum contrato de registro de computador identificado.
- Limites da inferência, diferença de pastas comuns e critério para retomar a integração: [Computadores](../specs/computers.md). Não foi acessada a conta pessoal nem executado teste de escrita.

## Diretórios, retomada e conta — consulta de 2026-10-26

- [Pastas](https://developers.google.com/workspace/drive/api/guides/folder): metadata de diretório e files.create.
- [Uploads](https://developers.google.com/workspace/drive/api/guides/manage-uploads): blocos, status de sessão, Range e expiração. Contrato local em [retomada](../specs/resumable-upload.md).
- [Downloads](https://developers.google.com/workspace/drive/api/guides/manage-downloads): `files.get?alt=media` e `Range` para conteúdo binário; referências Google não usam download parcial. Contrato local em [retomada](../specs/resumable-download.md).
- [about](https://developers.google.com/workspace/drive/api/reference/rest/v3/about): usuário e armazenamento; uso total pode incluir outros serviços e pool organizacional.
- [Changes](https://developers.google.com/workspace/drive/api/guides/manage-changes) e [changes.list](https://developers.google.com/workspace/drive/api/reference/rest/v3/changes/list): cursor inicial, paginação, remoções e novo cursor. Cache invalidado em qualquer evento nesta etapa; [contrato local](../specs/incremental-inventory.md).

As implementações têm testes locais/HTTP simulado; não representam homologação real desses endpoints.
