# Lacunas priorizadas

Atualizado em 2026-10-26. Evidência local: suíte automatizada, UI renderizada e binário executável. Login e listagem reais relatados pelo usuário; não reproduzidos pelo agente. Transferências reais pendentes.

| ID | Prioridade | Estado / falta |
|---|---|---|
| G01 | 0 | Concluída no escopo M0: planejador e simulação |
| G02 | 0 | Journal integrado e recuperação local testados; falta exercitar falhas reais de transferência e energia |
| G03 | 0 | Login/listagem reais relatados pelo usuário. Login via navegador preparado para cliente OAuth fornecido pela distribuição, ainda ausente na árvore; falta homologar renovação/transferências, incluindo ETag no resumable |
| G04 | 1 | Política conservadora bloqueia tipos/nomes incompatíveis; documentos Google/atalhos têm referências de navegador; pastas vazias e movimentos de diretórios implementados/testados; faltam exclusão segura de árvores, edição offline, nomes duplicados e filtros |
| G05 | 1 | Preferências reorganizadas com base em captura pública sem build identificado e documentação oficial; tema claro e navegação Meu computador/Google Drive. Falta comparação visual com versão identificada do cliente oficial e caracterização por cenário |
| G06 | 1 | Desktop/CLI/watch implementados; seleção remota por nomes/subpastas e local por zenity/kdialog, com campo manual de fallback. Progresso por bytes, usuário/cota e pausa entre operações implementados. Falta serviço autenticado real e integração ao gerenciador de arquivos |
| G07 | 2 | Streaming/FUSE, cache sob demanda e fixação offline ausentes |
| G08 | 1 | Upload em blocos com sessão/offset persistidos, download binário parcial por Range e retry com espera implementados/testados localmente. Cursor Changes atualiza arquivos por ID e renomeações/movimentos de pastas conhecidas com a subárvore cacheada; criação/remoção, parent desconhecido e colisões usam inventário completo. Watcher local antecipa ciclos. Falta homologação real de mudanças e retomadas |
| G09 | 2 | Múltiplas contas simultâneas/shared drives ausentes |
| G10 | 1 | GPL-3.0-only e receitas AppImage/deb/rpm/Arch preparadas; falta validar artefatos publicados, avisos de dependências e homologação entre distribuições |
| G11 | 1 | Registro do dispositivo na seção Computadores: sem operação pública documentada identificada na REST/Discovery v3 consultados. [Investigação e limites](specs/computers.md). Alternativa em Meu Drive não equivale a esse registro |

Não fechar G03 com testes HTTP locais. A prévia atual é utilizável para homologação de uma pasta dedicada de arquivos comuns, não uma substituição final do Drive oficial.
