# Planejador M0 — contrato implementado

Implementado em `crates/sync-core`, validado em 2026-10-26 com 11 testes sintéticos. O escopo é uma raiz e arquivos regulares abstratos; não há bytes de arquivos, operações reais ou journal.

## Entrada

`Snapshot` recebe base confirmada e inventários local/remoto. Chaves estáveis ligam objetos entre os três estados; para objetos vinculados, o futuro adaptador deverá usar identidade remota. Um arquivo local novo recebe uma chave sintética, a ser vinculada quando o adaptador criar o objeto remoto. Caminho não é identidade.

`FileState` tem caminho já projetado e fingerprint comparável de conteúdo. Alterar caminho conta como alteração mesmo com fingerprint igual. Não há inferência por timestamp. Paths são dados opacos; validação de caminhos, projeção e tipos não regulares serão responsabilidade explícita dos adaptadores antes de qualquer efeito real.

Observações:

- `Present`: estado conhecido.
- `Absent`: ausência confirmada em destino ainda não criado.
- `Deleted`: evidência explícita de exclusão.
- `Unknown`: falta de conhecimento, incluindo chave omitida do inventário.

`complete` indica uma varredura completa e utilizável. Mesmo assim, omissão não vira exclusão. O adaptador deverá distinguir perda de acesso e exclusão antes de produzir tombstones.

## Saída e regras

`plan` gera decisões ordenadas por chave. As decisões são transferências com precondições dos dois lados, conflitos ou bloqueios tipados. Igualdade dos estados dispensa transferência. Alteração unilateral contra base conhecida produz upload/download. Alterações distintas simultâneas e exclusão versus edição geram conflito. Criações simultâneas diferentes também conflitam.

Inventário incompleto bloqueia todos os efeitos. Estado desconhecido bloqueia o objeto. Exclusões unilaterais ficam bloqueadas por falta de executor recuperável. Um caminho já usado por outro ID em qualquer inventário bloqueia a transferência; não existe resolução automática de colisões. Esta política é conservadora e pode bloquear movimentos válidos, até existir planejamento de dependências.

## Simulação

`simulate` aceita somente o snapshot exato usado para planejar, incluindo base e completude. Se algo mudou, retorna `StaleSnapshot` antes de aplicar qualquer efeito. O plano não é mutável externamente. A simulação clona os dados e aplica transferências, atualizando a base desses objetos; conflitos e bloqueios permanecem intocados. Planos podem conter objetos bloqueados e transferências independentes.

Estados iguais já alterados não exigem transferência; M0 não promove esses estados à base automaticamente. O runtime implementa esse checkpoint em sync-core::mirror e engine; o protótipo M0 preserva seu contrato original. Não interpretar o simulador como uma implementação de recuperação ou execução atômica contra o Drive.

## Limites

Sem conta, credenciais, relógio, rede, filesystem, FUSE, tipos especiais, geração de revisões remotas ou conteúdo real. A validação global de snapshot é deliberadamente conservadora. Integração real exigirá revalidar precondições durante efeitos e tratar falhas parciais. Os testes demonstram comportamento do domínio sintético, não equivalência com o aplicativo oficial.

Aplicação atual: usa [espelhamento runtime](mirror.md), não o simulador M0 para efeitos reais.
