# ADR 0004 — Login de produto e seleção por nomes

Data: 2026-10-26. Estado: aceito na implementação local; cliente OAuth de distribuição e homologação completa pendentes.

## Contexto

O usuário confirmou login e leitura de pastas numa conta real e apontou que o aplicativo oficial inicia com login no navegador, sem exigir criação de cliente OAuth ou cópia de IDs. A prévia anterior colocava tarefas de desenvolvimento no fluxo de uso e vinculava a identidade do chaveiro às raízes, obrigando novo login ao trocar uma pasta.

## Decisão

O responsável pela compilação fornece um cliente OAuth Desktop próprio do projeto fora do repositório. O usuário autoriza pelo navegador. Não reutilizar identidade do cliente oficial nem inventar client IDs. A seção manual fica recolhida e identificada como desenvolvimento. Sem cliente fornecido, informar indisponibilidade nesta compilação, sem prometer login funcional.

A resolução procura `GDRIVE_OAUTH_CLIENT_FILE`, `../share/gdrive-clone/oauth-client.json` relativo ao diretório do executável, `oauth-client.json` no diretório de configuração e a configuração anterior de desenvolvimento, nessa ordem. A variável contém somente caminho, não JSON. O instalador aceita a mesma variável para copiar o cliente com permissão 0600. Isso não inicia distribuição pública nem verificação OAuth.

Separar conta (`account.json`, perfil de chaveiro e caminho do cliente) de sincronização (`config.json`, raízes e perfil de journal). Nenhum token nesses arquivos. Configurações antigas conservam a identidade de chaveiro anterior. A mudança de raízes conserva a conta para o mesmo caminho do cliente e mantém journals separados; intenções pendentes bloqueiam essa mudança. Login com outro cliente mantém identidade independente. Reconfiguração respeita o lock do executor, e o executor revalida o perfil depois de adquirir o lock.

Pastas remotas são navegadas por nome e trilha, usando IDs internamente. Nomes repetidos permanecem distinguíveis pelo ID em tooltip; isso não habilita projeção de nomes duplicados pelo motor. Erro/inventário incompleto não substitui a lista anterior por uma pasta vazia. Mudanças não salvas na interface desativam o modo automático e bloqueiam novos ciclos até salvar. Seletores locais opcionais usam zenity/kdialog sem shell; o campo manual permanece utilizável sem essas ferramentas.

## Fontes e limites

- [OAuth 2.0 para aplicativos Desktop](https://developers.google.com/identity/protocols/oauth2/native-app): identidade do aplicativo, navegador, PKCE e callback loopback.
- [OAuth para aplicativos instalados](https://developers.google.com/identity/protocols/oauth2/#installed): o cliente é provido pelo desenvolvedor; um aplicativo instalado não consegue guardar client secret como um servidor confidencial.
- [Escopos da Drive API](https://developers.google.com/workspace/drive/api/guides/api-specific-auth): o escopo amplo usado nesta prévia exige tratar os requisitos de verificação antes de distribuição pública.
- [Espelhamento e streaming no cliente oficial](https://support.google.com/drive/answer/13401938): opções de Meu Drive como referência documentada, não observação de uma versão identificada.

Consultados em 2026-10-26. Nenhuma credencial real incorporada nesta mudança. A confirmação do usuário não substitui aceitação de transferências, renovação, falhas ou recuperação. Streaming e demais lacunas continuam no roteiro.
