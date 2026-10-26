# Guia para agentes e contribuidores

## Contexto e leitura inicial

Construímos um cliente Linux independente e fiel aos fluxos do Google Drive para computador, em Rust, com API oficial e motor próprio. Não usar rclone. A interface é Rust com egui/eframe. Leia nesta ordem:

1. `README.md` e `ROADMAP.md`: estado real e foco atual.
2. `docs/architecture.md` e `docs/adr/0001-direcao-tecnica.md`: limites e decisões.
3. `docs/gaps.md` e `docs/target-app-parity.md`: tarefa prioritária e evidências.
4. `docs/specs/sync-safety.md` e `docs/testing.md`: invariantes e cenários.
5. `docs/specs/mirror.md`, `docs/acceptance.md`, `docs/development.md`, `docs/control-protocol.md` e a última entrada em `log/devlog.md`.

Documentação em português; código, identificadores e comandos em inglês. Esta é a instrução canônica para qualquer ferramenta de IA. `CLAUDE.md` apenas aponta para este arquivo.

## Regras de implementação

- Preserve dados antes de buscar paridade visual. Nunca trate uma falha de listagem, perda de permissão ou raiz desmontada como exclusão de todos os arquivos.
- Identifique objetos remotos por ID, não por caminho. Nomes duplicados e renomeações fazem parte do modelo.
- Toda operação com efeitos deve ter precondições e recuperação documentadas. Persistir uma intenção não significa que a operação remota foi concluída.
- UI, CLI e automação compartilham comandos do motor. Não duplicar regras de sincronização na interface.
- Mantenha o planejador determinístico e sem acesso a rede, disco ou relógio real. Injete snapshots e capacidades.
- Em código de produção, propague erros; não introduza `unwrap`, `expect`, `panic!`, `todo!` ou `unimplemented!`. Funcionalidades ausentes retornam erro explícito. Evite `unsafe`; uma necessidade de integração deve ser isolada e justificada em ADR.
- Credenciais nunca entram no repositório, fixtures, logs ou respostas de automação. Usar dados sintéticos por padrão.
- Reproduzir comportamento a partir de observação e especificações públicas. Não copiar código, marcas, ícones ou recursos proprietários. Registrar origem e licença de recursos incorporados.
- Não anunciar fidelidade verificada sem evidência. Comportamento desejado, documentado pelo fornecedor e observado no aplicativo são categorias distintas.
- Não adicionar dependências, crates vazias, servidores MCP ou infraestrutura que ainda não atendam a uma entrega concreta.

## Ciclo de trabalho

1. Escolha uma lacuna e leia sua especificação. Registre dúvidas como hipóteses; avance no que não depende delas.
2. Defina resultados verificáveis antes de implementar; para sincronização, inclua falha e retomada.
3. Faça uma fatia pequena e funcional. Uma simulação deve ser identificada como tal.
4. Execute os checks disponíveis em `docs/development.md`; reporte limitações sem apresentar comandos não executados como sucesso.
5. Atualize a linha de paridade afetada, a lacuna e o foco do roteiro quando houver evidência nova.
6. Acrescente uma entrada curta em `log/devlog.md`: mudança, validação, pendências e próximo passo.

Documentação simples não exige testes artificiais. Mudanças no motor exigem testes de comportamento e regressões relevantes. Antes de encerrar, não deixe links internos quebrados nem instruções que presumam ferramentas inexistentes.

## Retomada e mudanças de direção

O código e os testes descrevem o comportamento implementado; especificações descrevem o contrato desejado. Em divergência, registre a lacuna em vez de apagar o requisito. `ROADMAP.md` é a fonte única do foco; `docs/gaps.md` prioriza pendências; `docs/target-app-parity.md` guarda evidências. Decisões duradouras usam `docs/adr/`. Não reescreva histórico de validação.

Trabalhe apenas no escopo autorizado. Uma tarefa de documentação não autoriza conectar contas ou operar arquivos pessoais. Não faça testes destrutivos no Drive real. Não inicialize publicação, release ou licença de distribuição por conta própria.
