# Diretórios no espelhamento

Contrato da entrega em 2026-10-26: criação/associação de pastas, inclusive vazias, e renomeação/movimento dentro da raiz com ID remoto estável. Não implementar exclusão recursiva como consequência de um nome ausente.

## Identidade e planejamento

Entry possui directory explícito; registros antigos sem o campo são arquivos. Diretórios usam hash vazio e tamanho zero, mas sua igualdade nunca coincide com um arquivo vazio. A identidade continua dev:inode local e revisão remota. Colisão de tipos é conflito. SQLite v3/v4 impede que um binário antigo interprete diretórios como arquivos; migração preserva bases e intenções anteriores.

Diretórios são reconciliados dos pais para filhos antes dos arquivos. Depois de criar ou mover uma pasta, inventariar novamente antes de planejar descendentes. Conflito, exclusão não suportada ou falta de acesso a uma pasta bloqueia seus descendentes. Uma prévia lista ações sem criar pastas ou reservar IDs. Pastas da recuperação não entram no inventário.

## Efeitos e retomada

Criar pasta remota reserva e persiste o mesmo ID antes do efeito e usa files.create com MIME de pasta, conforme [API oficial](https://developers.google.com/workspace/drive/api/guides/folder). Atualização de pasta existente revalida tipo, caminho e revisão e exige ETag/If-Match. Não criar pais implicitamente fora do journal. Timeout após aceite remoto exige consultar o mesmo ID antes de repetir.

Criar pasta local usa descritores ancorados, NOFOLLOW e mkdirat; renomear usa NOREPLACE e preserva inode e conteúdo da árvore. Não substituir arquivo, diretório concorrente, symlink ou destino ocupado. Após crash, uma pasta movida só é reconhecida como conclusão se o destino preserva a identidade local anterior. Não usar checksum de arquivo para verificar diretório.

Excluir pastas no remoto ou mover árvore local para recuperação permanece bloqueado nesta entrega, inclusive se a listagem mostrou pasta vazia: uma criação concorrente de filho não é protegida por uma simples checagem anterior. Ausência de pasta não autoriza excluir os filhos. Remoção já realizada externamente nos dois lados pode aposentar vínculo apenas com evidência explícita de lixeira remota. KeepBoth automático permanece limitado a arquivos.

## Aceitação local

Criar/adotar pastas vazias nos dois sentidos e árvores aninhadas; mover diretórios com arquivos sem mudar IDs; preservar arquivo que ocupa destino de pasta; interromper antes/depois da criação remota e retomar com o mesmo ID; simular rename local já aplicado antes da confirmação; confirmar que prévia não cria nada e que exclusão/permissão perdida bloqueia a árvore. Reabrir SQLite v2 com intenções antigas sem convertê-las em diretórios. Testes HTTP devem verificar MIME/ID e precondições de metadata. Homologação Google real permanece separada.

A implementação atual refaz o inventário após cada efeito de pasta e limita o ciclo a 1024 efeitos estruturais; o próximo ciclo continua o trabalho. A pausa é consultada entre operações. Não há otimização incremental nesta entrega.
