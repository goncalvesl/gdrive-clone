# Computadores no Google Drive

Consulta em 2026-10-26. Estado: registro de novo dispositivo não implementado; nenhuma operação pública documentada identificada. Isso é uma lacuna de capacidade de integração, não somente de aparência da interface.

## Resultado da investigação

O usuário quer que este cliente Linux registre a máquina em `https://drive.google.com/drive/computers`, como o cliente oficial. A seção não deve ser confundida com uma pasta comum em Meu Drive ou com drives compartilhados.

Foram consultados a [referência REST v3](https://developers.google.com/workspace/drive/api/reference/rest/v3), o método [files.create](https://developers.google.com/workspace/drive/api/reference/rest/v3/files/create), o guia [de pastas](https://developers.google.com/workspace/drive/api/guides/folder) e o [Discovery público v3](https://www.googleapis.com/discovery/v1/apis/drive/v3/rest), revisão retornada `20261020`.

O Discovery lista os recursos about, accessproposals, approvals, apps, changes, channels, comments, drives, files, operations, permissions, replies, revisions e teamdrives. Não existe recurso de cadastro de computadores/dispositivos nesse contrato. O schema File também não expõe campos folderFeatures, machineRoot, computer ou deviceId. Os métodos documentados permitem criar arquivos e pastas comuns, mas não documentam o registro de uma máquina na seção Computadores.

Conclusão limitada à documentação e ao contrato públicos consultados: **não há integração suportada identificada para criar esse registro com a API oficial adotada pelo projeto**. Não é demonstração de impossibilidade absoluta nem prova sobre APIs internas do Google. Não prometer que completar o backup local fará o dispositivo aparecer nessa página.

O [relato público 172775993](https://issuetracker.google.com/issues/172775993) descreve dificuldades em distinguir itens existentes de Computadores pela listagem. O resultado indexado da busca mostrou o relato, mas a abertura direta exigiu login; não usar esse relato como garantia de comportamento atual nem como contrato de escrita. Ler um objeto existente por ID, quando permitido, também não demonstra capacidade de registrar outro dispositivo.

A página pessoal indicada pelo usuário redirecionou para autenticação na consulta web. Nenhuma conta foi conectada, nenhum arquivo pessoal foi inspecionado e nenhum teste remoto de escrita foi executado.

## Comportamento do cliente

A tela Meu computador informa que o aplicativo não registra o dispositivo nessa seção. Adicionar pasta permanece desativado. Um link permite ao usuário abrir Computadores no próprio navegador. O acesso ao espelhamento existente continua em Google Drive.

Uma alternativa possível é implementar sincronização de várias pastas locais para uma pasta identificada pelo nome da máquina em **Meu Drive**. Essa alternativa ainda não está implementada e não deve ser rotulada como registro em Computadores. Exige seleção explícita, identidades remotas persistidas, tratamento de colisões, precondições, exclusões recuperáveis e testes de falha/retomada antes de habilitar uploads.

Não criar pastas remotas para fingir o cadastro, reutilizar identidade do cliente oficial, nem introduzir endpoints privados sem uma nova decisão explícita e evidência apropriada. O escopo atual continua API pública e motor próprio em Rust.

## Critério para reabrir a integração

Identificar documentação oficial de uma operação de registro, autorização suportada para terceiros e semântica de criação, retomada, renomeação e remoção do dispositivo. Só depois implementar com conta/pasta de teste escolhidas pelo usuário e registrar evidência da aparição no site. Até lá, manter essa linha de paridade bloqueada e avançar nas funcionalidades com contrato público conhecido.
