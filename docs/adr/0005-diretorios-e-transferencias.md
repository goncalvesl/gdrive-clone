# ADR 0005 — Diretórios e transferências recuperáveis

Data: 2026-10-26. Estado: implementado/testado localmente; homologação Google pendente.

## Contexto

O usuário autorizou avançar nas funcionalidades para aproximar o cliente do oficial. A investigação de Computadores não identificou cadastro pela API pública. As lacunas diretamente implementáveis nesta etapa eram pastas vazias, movimentos de árvores, progresso, dados da conta, retomada de upload e controles de execução.

## Decisão

Incluir diretórios como entradas explícitas, com identidade e journal, e reconciliá-los antes dos arquivos. Refazer inventários depois de efeitos estruturais. Pais remotos deixam de ser criados implicitamente. Excluir árvores continua bloqueado: listar uma pasta vazia antes de enviar à lixeira não impede inclusão concorrente de filhos. Colisão e perda de acesso bloqueiam descendentes.

Elevar SQLite até v4, preservando v1/v2/v3: tipos explícitos e tabela upload_sessions. A engine injeta persistência através de UploadSessionStore; drive-api não depende de SQLite. Uma segunda conexão do perfil, sob o mesmo lock externo e sem transações longas durante HTTP, grava sessão somente para intenção running. finish limpa sessão/vínculo/intenção atomicamente. Não copiar tokens para o banco e nunca incluir URL de sessão em relatórios ou erros.

Enviar blocos de até 8 MiB e reconsultar Range do servidor após interrupção. Offset local não prova bytes recebidos. Revalidar expectativa/destino e host de sessão recarregada. Expiração permite nova sessão sob o mesmo ID; confirmação ainda exige metadados e conteúdo esperados. Downloads continuam recomeçando.

Usar ProgressSink opcional para telemetria, sem confirmar o efeito ao chegar a 100%. Nome, email e quota via about.get permanecem em memória e são opcionais na apresentação. A consulta pode incluir uso de outros serviços ou pool organizacional.

Classificar transporte/429/5xx para retry com espera e Retry-After numérico; erros permanentes interrompem automação. Preservar a causa ao propagar erros pelo executor. Consultar a flag de pausa entre efeitos, sem interromper substituição de arquivo em andamento. UI, CLI e watch usam os mesmos comandos.

## Consequências e limites

Sem novas dependências, FUSE, watchers, inventário incremental, várias raízes simultâneas, projeção de nomes duplicados ou cadastro em Computadores. Diretórios causam novas varreduras completas; desempenho ainda precisa de medição. ETag/If-Match, sessão expirada, renovação, quota, energia e serviço reais ainda exigem aceitação. Testes HTTP/SQLite/filesystem não equivalem a essa homologação.

Contratos: [diretórios](../specs/directories.md), [retomada](../specs/resumable-upload.md) e [SQLite](../specs/state-store.md). Fontes: [pastas](https://developers.google.com/workspace/drive/api/guides/folder), [uploads](https://developers.google.com/workspace/drive/api/guides/manage-uploads) e [about](https://developers.google.com/workspace/drive/api/reference/rest/v3/about).
