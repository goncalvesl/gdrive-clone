# Aceitação antes de uma versão estável

Estado em 2026-10-26: testes automatizados e execução visual local realizados. O usuário relatou login e leitura de pastas numa conta real nesta conversa. O agente não reproduziu esses passos nem registrou versão/artefatos; isso não aprova renovação, transferências ou recuperação. Esses cenários reais continuam **não executados**.

## Procedimento reproduzível

Usar pasta local descartável e uma pasta remota dedicada com arquivos sintéticos, selecionadas pelo usuário. Registrar versão do binário, distribuição, modo X11/Wayland e data; não registrar credenciais.

1. Usar uma instalação com cliente OAuth do projeto fornecido pelo responsável pela compilação. Sem preencher credenciais, clicar em Fazer login com o navegador, autorizar, navegar por nomes/subpastas, escolher a pasta de teste e confirmar armazenamento no chaveiro. Trocar a seleção deve preservar o login e bloquear troca com journal pendente.
2. Prévia, upload e download inicial; comparar hashes e tamanhos.
3. Editar unilateralmente e simultaneamente; verificar ação Preservar ambas.
4. Renomear/mover nos dois lados e conferir que o ID remoto não mudou.
5. Excluir com propagação desativada e depois ativada; conferir lixeira e recuperação local.
6. Cortar conexão antes, durante e após a resposta de upload maior que 8 MiB; reiniciar e conferir sessão/Range autoritativo, ausência de duplicação de IDs, progresso e limpeza transacional da sessão. Exercitar também expiração de sessão.
7. Forçar edição concorrente remota durante uma atualização; verificar comportamento real de If-Match/ETag nas sessões resumable.
8. Testar arquivo vazio, arquivo grande, quota/rate limit, renovação e revogação de token.
9. Executar como serviço, pausar/retomar e abrir a GUI; conferir bloqueio de concorrência e acesso ao chaveiro.
10. Testar perda de permissão, raiz substituída, nomes incompatíveis e pasta contendo documento Google; verificar bloqueio dos nomes inseguros e referências abertas no navegador sem perda de arquivos.
11. Criar/adotar pastas vazias nos dois sentidos, mover árvores com arquivos e confirmar IDs estáveis. Colisão/perda de acesso/exclusão de pasta deve bloquear descendentes sem apagar conteúdo. Pausar durante um lote deve confirmar o arquivo atual e deixar o próximo sem iniciar.
12. Consultar nome/e-mail/cota e conferir valores no Google, incluindo conta organizacional. Simular falha transitória de rede e conferir espera/retry; permissão/autenticação/precondição deve interromper o modo automático, preservando intenção e arquivos.

13. Interromper download binário grande após receber parte do corpo; reiniciar e confirmar solicitação Range do offset local, Content-Range correto, checksum final e preservação da versão antiga até a instalação. Repetir com arquivo remoto alterado e resposta sem suporte a Range; ambos devem bloquear sem substituir a cópia local.

14. Com cursor Changes inicial, testar ciclo sem alterações, mudança em arquivo/pasta, remoção/perda de acesso, paginação e cursor rejeitado. Confirmar cache reutilizado sem mudanças, atualização por ID de arquivos e movimentos de pastas conhecidas, inventário completo para criação/remoção de pastas e colisões e que falha durante atualização não avança cursor nem Entries.

Guardar resultados sanitizados e evidências por cenário. Falhas reabrem as lacunas correspondentes. Nenhum cenário real acima deve ser marcado como aprovado apenas com testes mock.

## Gate atual

Ainda não passa o gate de beta: há integração externa sem homologação e lacunas relevantes de paridade. O artefato local é uma prévia de espelhamento para homologação. Streaming/FUSE e demais capacidades não implementadas estão fora do escopo executável atual e continuam no roteiro.
