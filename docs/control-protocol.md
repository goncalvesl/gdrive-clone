# Controle por CLI e agentes

Implementado pelo binário drive-desktop; `--help` descreve argumentos. A UI usa os mesmos comandos públicos de engine::Paths.

| CLI | Efeito |
|---|---|
| configure | Configuração XDG e perfil vinculado às raízes |
| login [--oauth PATH] / disconnect | Login pelo navegador antes da configuração de raízes / remoção da credencial local; --oauth é opção de desenvolvimento |
| open-document PATH | Abre referência Google validada no navegador |
| folders [ID_PAI] | Lista IDs/nomes das subpastas remotas em JSON; requer conexão, não requer raízes de sincronização |
| plan | Relatório JSON sem efeitos no conteúdo dos arquivos |
| sync | Um ciclo com relatório JSON |
| watch | Ciclos periódicos para serviço de usuário |
| account | Consulta usuário e armazenamento via about.get; não retorna tokens |
| status / history | Último estado e histórico confirmado em JSON |
| pause / resume | Controla próximos ciclos via sinalizador persistente |
| resolve PATH | Preserva ambas as versões de um conflito |
| recover | Inspeciona e replaneja intenções interrompidas conservadoramente |

Resultados de plan/sync incluem completed, planned, conflicts, blocked, recovered e paused. Erros vão a stderr e o processo retorna status 1. O formato JSON é da prévia 0.1 e ainda não tem promessa de compatibilidade estável. Não fornece tokens.

Cada processo pode solicitar trabalho, mas o lock por perfil permite apenas um ciclo executor. Não existe socket local, RPC ou MCP; o protocolo Unix socket anterior permanece uma possibilidade futura, não um comando disponível. Nenhum endpoint de rede de controle é aberto. OAuth abre temporariamente um callback loopback durante o login.

`--screenshot PATH` captura a interface para inspeção visual; usar perfil descartável. Não é um controle remoto geral da UI.

`watch` antecipa ciclos após eventos de arquivos locais (inotify); Changes API é consultada periodicamente: sem alterações reutiliza cache, arquivos comuns e movimentos de pastas conhecidas atualizam por ID; criação/remoção de pastas e ambiguidades disparam inventário completo. Notificações locais são apenas gatilhos, nunca prova de conteúdo ou exclusão. Usa espera progressiva em falhas transitórias e persiste pausa em erros permanentes; `resume` permite nova inspeção. `pause` é consultado entre operações de um ciclo. Uploads em andamento não são truncados. A telemetria por bytes da GUI usa cycle_with_progress da engine e não confirma a operação.
