# Contrato de preservação e recuperação

Estado: contrato desejado. O executor runtime integra journal, efeitos locais e API Drive, com validação por testes locais e remoto simulado. A homologação com conta Google real permanece pendente. O protótipo M0 continua como exemplo isolado. Veja [planejador M0](planner.md). Estes requisitos são específicos deste cliente, não alegações sobre o algoritmo interno do Google.

## Invariantes

- Um inventário incompleto não autoriza deduzir exclusões por ausência.
- Uma alteração simultânea em ambos os lados preserva as duas versões; timestamps isolados não elegem vencedor.
- Downloads escrevem em temporário no mesmo filesystem, verificam o conteúdo disponível e só então publicam o destino. Nunca truncar a última versão confirmada antes de obter a substituta.
- Uploads verificam se a origem mudou durante a leitura. Uma transferência concluída não autoriza confirmar conteúdo diferente do snapshot planejado.
- Exclusões intencionais usam lixeira/quarentena conforme a capacidade do lado afetado. Política de retenção deve ser definida antes de habilitar limpeza permanente automática.
- Falta de permissão, erro HTTP, quota, disco cheio, token inválido e raiz indisponível são estados de erro, não estados de diretório vazio.
- Um cursor de mudanças só avança com o estado correspondente persistido de modo recuperável.
- Caminhos remotos não escapam da raiz local. Symlinks não são seguidos por padrão; política explícita e testes são necessários antes de suportá-los.

## Journal e evolução

Protótipo v1: `planned -> running -> observed -> committed`. Runtime v2: planned/running seguido de confirmação transacional em links/histórico, retirando a intenção ativa. Ver [contrato do armazenamento](state-store.md). Ainda propostos: `retryable`, `conflict` e `blocked` como resultados que exigem tratamento. Cada registro inclui ID estável, operação, objetos envolvidos, precondições e dados necessários à recuperação. Tokens e conteúdo de arquivos não entram no journal.

Após crash em `running`, consultar o estado dos lados antes de repetir. Em especial, criar arquivo remoto não deve ser repetido cegamente após timeout: a API pode ter aceitado a primeira tentativa. O runtime reserva IDs antes da intenção e inspeciona o mesmo ID depois de falhas; ver [espelhamento](mirror.md).

## Tabela mínima de reconciliação

| Local desde a base | Remoto desde a base | Resultado |
|---|---|---|
| Igual | Igual | Nenhuma transferência |
| Alterado | Igual | Upload com precondições |
| Igual | Alterado | Download com precondições |
| Alterado | Alterado | Conflito; preservar ambos |
| Apagado | Alterado | Conflito; preservar edição |
| Alterado | Apagado | Conflito; preservar edição |
| Ausente em inventário incompleto | Qualquer | Revarrer/bloquear dedução de exclusão |

Criações simultâneas com mesmo nome não provam identidade. Renomeações precisam considerar IDs e colisões. A tabela é mínima, não algoritmo completo.

## Diretórios e sessões de transferência

Entradas de diretório têm tipo explícito, sem equivalência com arquivos de tamanho zero. Conflito, acesso perdido ou exclusão de uma pasta bloqueia descendentes; esta prévia não propaga exclusão de árvores. Movimentos locais não substituem destinos e a recuperação verifica inode da pasta movida. Depois de efeito estrutural, refazer inventários antes de tratar filhos.

Sessão de upload e offset pertencem à intenção running. O offset do servidor é autoritativo após interrupção, e o fim da transmissão não confirma o journal. Remover a sessão junto com a confirmação do vínculo, sob transação. Nunca registrar URL de sessão ou autorização em diagnósticos. A pausa só para entre efeitos; retry exige reinspeção, independentemente da espera.

Download binário interrompido conserva staging privado identificado pela intenção. Seu tamanho determina o próximo offset, mas nenhum byte parcial é publicado antes de conferir tipo, Range, tamanho e checksum final. Resposta HTTP 200 a um pedido parcial não autoriza anexar o corpo. Um staging completo corrompido é descartado sem tocar o arquivo confirmado. Veja [retomada de download](resumable-download.md).
