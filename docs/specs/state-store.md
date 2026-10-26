# SQLite — schema e journal

Implementado em state-store; um banco por perfil/raiz e lock externo por executor. DELETE journal, synchronous FULL e busy timeout de cinco segundos. Nenhum conteúdo ou token no banco.

## Schema

v1 mantém o protótipo: confirmed e operations com as fases planned/running/observed/committed. As APIs e testes M0 continuam disponíveis.

v2 adiciona, por migração transacional sem apagar v1:

- mirror_links: ID remoto e Link confirmado, com estados local/remoto.
- mirror_journal: ID da operação, ID do objeto, Operation serializada e fase planned/running.
- mirror_history: últimas operações concluídas, com descrição e sequência local. Finish limita o histórico a 1000 registros; a consulta retorna os 100 mais recentes. Retire explícito também registra histórico.

v3 identifica entradas de diretório com o campo directory; JSON antigo sem o campo continua sendo arquivo. v4 acrescenta upload_sessions (ID remoto e payload de sessão, snapshot esperado, destino e offset). v5 acrescenta remote_inventory e remote_cursor. Migração transacional aceita v1/v2/v3/v4 e conserva bases/intenções. A elevação de versão impede leitores antigos de tratar diretórios como conteúdo de arquivo ou descartar sessões/cache.

URL de sessão é dado privado e não entra em histórico/relatório. save_upload_session exige intenção running para o objeto. finish remove sessão, atualiza vínculo e conclui a intenção na mesma transação; um rollback conserva os três. retire explícito remove o registro de sessão sem afirmar conclusão remota. Não há tokens OAuth nessa tabela.

O offset de download fica no tamanho do staging privado associado ao ID da intenção; não há coluna específica para essa retomada. `remote_inventory` guarda Entries por ID; `remote_cursor` guarda o cursor da conta. Salvamento completo substitui Entries e cursor numa transação. Quando o feed vem vazio, somente o cursor avança; eventos de arquivos comuns atualizam Entries alterados e cursor na mesma transação; falha não avança o estado. A sessão/offset de upload continua separada, com reinspeção do servidor na retomada.

## Runtime

stage compara a base esperada dentro da transação e persiste a intenção. Índice único impede duas intenções ativas por objeto. mark_running acontece antes do efeito. finish confirma Link (ou remove a associação em exclusão) e retira a intenção na mesma transação; registra histórico. O chamador precisa ter inspecionado o resultado externo antes de concluir.

Após interrupção, engine inspeciona estados reais. Efeito completo é confirmado; estado ainda igual às precondições permite retentativa com o mesmo ID. Divergências bloqueiam. recover pode aposentar a intenção conservando a base anterior, ou associar um ID criado e visível, para evitar recriar o arquivo. IDs sem acesso continuam bloqueados.

Checkpoint também atualiza estados iguais alterados e novas associações por conteúdo. KeepBoth participa do mesmo journal e mantém cópia visível e backup antes da instalação da versão remota.

## Garantias e limites

Transações cobrem SQLite, não Drive e filesystem. Testes exercitam migração preservando v1, reabertura entre fases, rollback entre escrita da base e conclusão do journal, perda de processo e recuperação integrada de efeitos simulados sobre arquivos reais. Energia física e efeitos Google reais ainda precisam de homologação.

Diretórios XDG privados são criados pelo engine; a raiz de sincronização não pode conter configuração, dados ou o arquivo OAuth. Conteúdo recuperável fica fora do banco, em .gdrive-clone-recovery dentro da raiz. Não sincronizar o próprio SQLite. API e fases do protótipo v1 não são o executor ativo do desktop; veja [espelhamento runtime](mirror.md).
