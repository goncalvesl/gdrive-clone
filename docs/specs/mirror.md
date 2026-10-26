# Contrato do espelhamento runtime

Atualizado em 2026-10-26. Código: sync-core/src/mirror.rs e engine/src/lib.rs. O planejador M0 e seu simulador são preservados para testes/exemplo; o aplicativo usa o contrato runtime abaixo.

## Precondições

Um perfil de sincronização seleciona uma conexão OAuth, uma raiz remota e uma raiz local vinculada a dev:inode. A identidade do chaveiro é independente do perfil de journal. Trocar raízes conserva o login do mesmo cliente, cria journal independente e é bloqueado se há intenções pendentes ou executor ativo. Diretórios de dados/configuração não podem estar dentro da raiz sincronizada. Inventários devem ser completos, sem symlinks, hard links, submontagens, tipos especiais ou nomes incompatíveis. Ausência remota sem evidência de lixeira é desconhecida.

Para cada operação persistida, são guardados ID estável, tipo, estados local/remoto planejados e base. Antes do efeito, o executor inspeciona os arquivos e verifica as precondições. Identidade local detecta renomeações; mudanças de inode no mesmo caminho podem ser edições por substituição atômica.

## Resultados

- Criação unilateral: upload/download; uma criação remota usa ID previamente reservado.
- Conteúdo e caminho iguais: checkpoint, incluindo estados iguais alterados nos dois lados.
- Mudança unilateral: transferência do lado alterado, preservando ID remoto.
- Mudanças distintas dos dois lados, inclusive editar/excluir: conflito sem sobrescrita.
- Remoção local de arquivo com remoto inalterado: lixeira remota, se propagação habilitada.
- Lixeira remota explícita de arquivo com local inalterado: quarentena local, se habilitada.
- Falha de acesso, remoção da visão ou movimento remoto para fora da raiz: bloqueio.

A primeira associação por caminho exige conteúdo igual. Nomes remotos duplicados bloqueiam a raiz. Collisions no destino local são preservadas pelo rename NOREPLACE e bloqueiam o efeito. Links ainda desconhecidos não são apagados do estado por erro de inventário.

## KeepBoth

Comando explícito resolve um conflito de dois arquivos existentes. Cria uma cópia local visível `nome.conflict-<operation_id>`, baixa a versão remota e confirma sua associação. No ciclo seguinte, a cópia é enviada como arquivo novo. Não existe fusão textual automática.

## Journal runtime

`planned -> running -> conclusão transacional em links/histórico`. O registro ativo é removido na mesma transação em que a base é atualizada. Não reutilizar o ID de operação de um novo efeito.

Running é inspecionado antes de repetição. Se um upload já está presente com conteúdo/caminho esperados, a criação não é repetida. Download já instalado é confirmado. Um download interrompido após quarentena pode prosseguir verificando a cópia anterior. Origem ou destino divergente bloqueiam; recover é uma ação explícita, não um descarte automático do journal.

## Limites de atomicidade

SQLite confirma apenas o estado local do journal; não coordena uma transação com Drive/filesystem. O controle de concorrência externo depende de revalidação, IDs reservados, cópias recuperáveis e If-Match. A homologação real desses endpoints e casos de crash durante requisições é pendente. Uploads usam blocos de 8 MiB e sessão/offset privados persistidos; após interrupção, consultam o servidor antes de continuar. Sessões expiradas podem reiniciar sob o mesmo ID. A GUI recebe progresso de leitura/transporte, separado da confirmação. Downloads binários retomam de staging privado após validar o Range HTTP e o checksum completo; referências Google e KeepBoth recomeçam. Veja [retomada de download](resumable-download.md). Não há execução paralela de arquivos. Veja [retomada de upload](resumable-upload.md).

## Documentos Google

Arquivos nativos são referências JSON estáveis por ID: .gdoc/.gsheet/.gslides e .gdrive-link para demais tipos. Entry usa MD5/tamanho desses bytes e versão remota como identidade. Edições no navegador atualizam o checkpoint de metadados sem exportar conteúdo. Modificar o JSON local não altera o documento: a edição vira conflito, sem criar uma intenção de upload do conteúdo do documento. Rename/move preserva a extensão e usa PATCH de metadados. Colisões entre nome projetado e arquivo binário real bloqueiam o inventário.

## Diretórios e pausa

Pastas, inclusive vazias, participam do journal com directory explícito e identidades próprias. São criadas/associadas e renomeadas/movidas antes dos arquivos, com novo inventário após efeitos. Conflito, perda de acesso ou exclusão de árvore bloqueia descendentes; exclusão de diretórios permanece indisponível. Veja [diretórios](directories.md).

UI, CLI pause/resume e watch usam a mesma flag. O executor consulta a pausa entre efeitos e retorna Report.paused; o arquivo em andamento termina sem ser truncado, e o próximo não reserva ID nem começa. Uma prévia continua permitida enquanto pausado. Falhas transitórias conservam a intenção e permitem retry com espera; falhas permanentes interrompem o modo automático. A política de espera recebe contagem e mínimo do servidor, sem relógio no planejador.
