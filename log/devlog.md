# Registro de trabalho

## 2026-10-26 — Base documental

- Criados guia canônico para agentes, arquitetura proposta, ADR inicial, roteiro, lacunas, matriz de paridade, contrato de preservação e plano de testes.
- Consultados documentos públicos do PhotoCraft e guias oficiais do Google; fontes registradas em `docs/reference/sources.md`. Padrões privados da ArtCraft não acessados.
- Decisões confirmadas: Rust, motor próprio, API Google, sem rclone. Toolkit e licença continuam abertos.
- Estado real: somente documentação; sem workspace Cargo, aplicação ou acesso a contas.
- Validação: links relativos verificados por script local; revisão de consistência dos estados. Nenhum teste Rust aplicável.
- Próximo passo: M0 no ROADMAP.md; domínio e planejador puro com snapshots sintéticos. Depois persistência/recuperação, antes dos efeitos reais.

## 2026-10-26 — M0 executável

- Criado workspace Rust 2024, mínimo 1.99, crate sync-core sem dependências, lockfile e lints compartilhados.
- Planejador de três estados com ordem estável, conflitos, bloqueios e precondições; simulador em memória rejeita snapshots desatualizados.
- Exclusões e colisões são bloqueadas; omissão/perda de acesso não são tratadas como exclusão. Identidade e fingerprints são sintéticos; não há operações reais.
- Validação: 11 testes de integração passaram; fmt --check e Clippy com -D warnings passaram; exemplo simulate executado e convergiu para zero decisões.
- Corrigido erro de ownership no teste de ordem durante a validação inicial.
- Limitações: sem bytes, persistência, API, UI ou equivalência observada com Drive oficial. Estados iguais alterados ainda não avançam checkpoint.
- Próximo passo: M1, journal SQLite e recuperação com falhas injetadas; foco atualizado no ROADMAP.md.

## 2026-10-26 — SQLite e journal inicial (M1 parcial)

- Usuário confirmou SQLite. Registrado ADR 0002: metadados/journal no banco; conteúdo e tokens fora.
- Criada state-store com schema v1, migração inicial transacional, confirmed e operations; um objeto só pode ter uma operação ativa.
- Implementadas fases planned/running/observed/committed; confirmação de base e conclusão do journal são atômicas. Running exige inspeção após interrupção.
- Serde adicionado a tipos persistíveis do domínio; rusqlite 0.41 bundled e serde_json com versões resolvidas em Cargo.lock.
- Validação: 18 entradas de teste passaram (11 do domínio, 6 cenários de armazenamento e 1 helper); inclui reabertura por fase, erro SQL entre writes e processo encerrado sem destructors. Fmt e Clippy -D warnings passaram.
- Limites: armazenamento isolado; executor integrado, checkpoint de estados iguais, cancelamento/replanejamento e recuperação de efeitos ainda pendentes. M1 permanece parcial.
- Próxima tarefa detalhada em ROADMAP.md: integrar planejador, journal e executor simulado com falhas em torno dos efeitos.

## 2026-10-26 — Prévia executável de espelhamento 0.1.0

- Pedido: avançar da base até uma versão próxima da final. Entregue prévia executável de espelhamento; não declarar versão final/beta, pois homologação real e capacidades de paridade continuam abertas.
- Criados local-fs, drive-api, engine e drive-desktop. O domínio ativo está em sync-core::mirror; o protótipo M0 permanece isolado e testado.
- Integrados SQLite v2 e executor: links por ID, intenções antes dos efeitos, inspeção após interrupção, confirmação transacional, checkpoint de estados iguais e replanejamento explícito.
- Operações: uploads/downloads de bytes, renomeações/movimentos, exclusões opt-in em lixeira/quarentena e KeepBoth com cópia visível; fontes de upload estáveis e snapshots removidos após confirmação.
- Adapter Linux usa descritores ancorados, NOFOLLOW, limites de profundidade, checksum e rename NOREPLACE; symlinks/hard links/submontagens/tipos incompatíveis bloqueiam o inventário.
- OAuth Desktop com PKCE, callback loopback, navegador e refresh token no Secret Service via secret-tool. Não foram fornecidas credenciais nem realizado login real. Não há tokens em arquivos ou SQLite.
- Drive v3: inventário recursivo paginado, reserva prévia de IDs, transferências, metadata/ETag e lixeira. Conteúdo nativo vira referência .gdoc/.gsheet/.gslides/.gdrive-link, com handler MIME seguro; mudanças no JSON viram conflito em vez de upload de conteúdo do documento.
- Interface Rust egui/eframe com configuração, prévia, ciclo manual/automático, histórico, recuperação e resolução de conflitos. CLI usa os mesmos comandos; watch e serviço de usuário preparados. A janela foi renderizada/capturada e inspecionada com XDG descartável.
- Instalador de usuário e ícone SVG original; sem publicação ou serviço ativado. Testado em raiz temporária via GDRIVE_INSTALL_HOME, com desktop-file-validate, cache MIME, executável instalado e verificação estática systemd após ajustar o caminho do binário no template temporário. A primeira checagem direta do template indicou o binário ainda não instalado no usuário; o teste isolado resolveu esse contexto sem alterar a sessão real.
- Validação final: scripts/check.sh passou: fmt, Clippy -D warnings, 65 entradas de testes (1 helper de subprocesso), build release otimizado. Binário final target/release/drive-desktop, 16.500.024 bytes neste ambiente. 50 links internos válidos. Corrigidos erros de compilação e lints encontrados durante a integração; a última bateria ficou verde.
- Testes: 15 HTTP/API (loopback, inclusive corpo de upload e If-Match); 20 integração de espelhamento (arquivos/SQLite reais, remoto mock); 4 configuração; 6 filesystem; 9 SQLite/helper; 11 M0. Nunca interpretar isso como homologação Google real.
- Limites restantes: sem FUSE/streaming, múltiplas contas simultâneas, shared drives, pastas vazias, filtros, inventário incremental ou offset persistente de upload. Progresso por ciclo, não por bytes. Homologar semântica real de ETag/If-Match e OAuth antes de uso estável.
- Próximo trabalho: docs/acceptance.md com cliente OAuth e pasta Google de teste escolhidos pelo usuário; corrigir resultados e então avançar nos itens de paridade do ROADMAP.md. Documentação de uso e estado real atualizadas.

## 2026-10-26 — Login de produto e seleção de pastas por nomes

- Usuário confirmou conexão e leitura das pastas numa conta real; evidência é relato nesta conversa, sem reprodução pelo agente nem versão/artefatos de homologação. Não estender essa confirmação a renovação, upload, download ou recuperação. Atualizados roteiro, lacunas, aceitação e matriz de paridade sem alterar o histórico anterior.
- Fluxo principal da interface começa com Fazer login com o navegador. O responsável pela compilação fornece o cliente OAuth do projeto; configuração manual recolhida como desenvolvimento. Sem cliente disponível, informar login indisponível nesta compilação. Nenhuma credencial real adicionada, nenhum cliente oficial reutilizado e nenhuma publicação iniciada.
- Cliente fornecido por caminho na variável GDRIVE_OAUTH_CLIENT_FILE, instalação relativa ao prefixo, arquivo gerenciado no XDG ou configuração anterior. Instalador copia opcionalmente o cliente para o prefixo com modo 0600. Registrado ADR 0004 e fontes oficiais Google.
- Separados perfil da conexão e perfil de journal; account.json contém somente identidade e caminho, sem tokens. Login antecede configuração das raízes; selecionar outra pasta mantém o login do mesmo cliente. Configurações antigas mantêm a identidade original no chaveiro. CLI login --oauth permite desenvolvimento antes das raízes; configure sem --oauth usa o cliente fornecido pela aplicação.
- Navegador remoto por nomes, subpastas e trilha; ID interno e tooltip para distinguir nomes repetidos. Listagem valida acesso/paginação e só substitui a lista anterior após sucesso. Escolha de pastas não inicia sincronização. O motor ainda bloqueia nomes duplicados na árvore sincronizada.
- Seletores locais opcionais zenity/kdialog em worker, sem shell; caminho manual continua disponível. Alterações não salvas desativam modo automático e bloqueiam novos ciclos na GUI. Adicionada ação Recuperar pendências à interface.
- Reconfiguração respeita lock do executor e bloqueia troca de raízes com intenções pendentes. Mantém lock até persistir a seleção e o nome numa única gravação. Executor revalida perfil após adquirir lock para recusar configuração antiga.
- Validação final: scripts/check.sh passou (fmt, Clippy -D warnings, 73 entradas de teste, incluindo um helper, build release). São 17 HTTP, 10 configuração, 20 espelhamento, 6 filesystem, 9 SQLite/helper e 11 domínio. Checks sem conta/chaveiro reais.
- Instalação em prefixo temporário com cliente sintético: modo 0600, configure sem --oauth e desktop-file-validate passaram; sem login, rede ou ativação de serviço. bash -n scripts/install.sh passou. 48 links locais válidos. Interface do binário final capturada com XDG descartável e inspecionada; screenshot atualizado.
- Pendências: cliente OAuth próprio para distribuição, homologação real das transferências/renovação/falhas e serviço. Streaming, projeção de nomes duplicados, pastas vazias, progresso por bytes e demais itens do roteiro permanecem abertos. A entrega continua prévia, não versão final. Próximo passo: fornecer cliente do projeto e executar aceitação com pastas sintéticas escolhidas pelo usuário.

## 2026-10-26 — Preferências próximas da organização visual oficial

- Mudança de foco solicitada pelo usuário: aproximar a tela de configuração do aplicativo oficial. Atualizados ROADMAP.md, G05 e P12; contrato visual e referências em docs/specs/desktop-preferences.md.
- Consultadas a ajuda oficial Google e uma captura pública na Comunidade Google Drive. A captura mostra Windows, sem build identificado; não registrar como observação de um binário atual nem alegar paridade pixel a pixel. Imagem externa apenas consultada, sem incorporação ao aplicativo/repositório.
- Substituída a tela escura por preferências claras: cabeçalho compacto, ações de ajuda/configurações/conta, navegação Meu computador/Google Drive, seleção azul arredondada, painel de modos e rodapé Cancelar/Salvar. Símbolo, ícones e ilustração desenhados originalmente em egui, sem novos assets ou dependências.
- Google Drive abre por padrão; seleção remota em janela de navegação e seleção local em Alterar. Configurações de conta, exclusões, recuperação e campos manuais ficam na página acessível pelo ícone do cabeçalho. Sincronização e atividade preservadas na navegação secundária.
- Streaming e Adicionar pasta em Meu computador aparecem desativados e identificados como indisponíveis; não foi implementado backup em Computadores ou FUSE. Não inventar identidade/email/quota de conta. Motor e formatos de persistência preservados.
- Salvar continua usando configure_named da engine; Cancelar restaura opções em memória sem escrever configuração, criar a raiz editada ou iniciar ciclo. Login/desconexão permanecem ações explícitas independentes do cancelamento de opções.
- Acrescentado --screenshot-page drive|computer|settings, restrito ao modo --screenshot. Capturas finais com XDG descartável e dados sintéticos: setup.png, preferences.png, computer.png e settings.png. Inspecionadas as três páginas; corrigido glifo ausente e corte da última linha no painel padrão de 940 × 720. Não houve login, chaveiro ou operação Google nesta tarefa.
- Validação final: scripts/check.sh passou (fmt, Clippy -D warnings, 75 entradas de teste incluindo helper, build release). Duas novas regressões egui verificam navegação sem gravar/iniciar worker e cancelamento sem criar nova raiz. A opção de captura sem --screenshot foi rejeitada como esperado. 58 links locais válidos antes desta entrada; nenhum link quebrado. Corrigidos warnings de literais float durante a implementação; bateria final sem warnings.
- Pendências: comparação com versão identificada do oficial e ajustes a partir de observação do usuário; streaming/backup ausentes e homologação de transferências continuam no roteiro. Próximo passo visual: conferir as capturas com o usuário e refinar detalhes observáveis sem anunciar funcionalidades inexistentes.

## 2026-10-26 — Limite da integração com Computadores

- Usuário pediu retomada das funcionalidades e perguntou se o dispositivo aparecerá na seção Computadores do site. Investigados REST v3, files.create, guia de pastas e Discovery público, revisão retornada 20261020; não identificado recurso/operação de cadastro de máquina nem campos correspondentes no schema File.
- Registrada conclusão limitada ao contrato público: não prometer registro em Computadores com a integração atual; ausência de contrato público não demonstra impossibilidade absoluta. Criar pasta com nome de máquina em Meu Drive não equivale ao cadastro. Acesso por ID a conteúdo existente não comprova criação de novo dispositivo.
- Página pessoal redirecionou para autenticação na consulta web; nenhuma conta ou arquivo pessoal acessado. Relato indexado no Issue Tracker 172775993 apareceu na busca, mas abertura direta exigiu login; não usado como contrato atual de escrita. Não houve testes remotos nem endpoints privados.
- Criado docs/specs/computers.md com fontes, limites, critérios para reabrir integração e alternativa em Meu Drive claramente separada. Acrescentados G11 e P13 bloqueado; roteiro retoma funcionalidades públicas, com pastas vazias como próxima fatia do motor.
- Tela Meu computador agora informa que o cliente não registra o dispositivo nessa seção, mantém Adicionar pasta desativado e oferece link para abrir Computadores no navegador. O espelhamento existente continua disponível, sem efeitos novos.
- Validação: scripts/check.sh passou (fmt, Clippy -D warnings, 75 entradas de teste incluindo helper, build release). Captura computer.png atualizada com perfil sintético e inspecionada. 65 links locais válidos antes desta entrada. Sem novos testes artificiais para texto/link e sem alterações no motor.
- Próximo passo: implementar diretórios vazios com identidades persistidas, precondições e regressões de falha/retomada; não confundir essa entrega com registro de dispositivo no site.

## 2026-10-26 — Diretórios, progresso e transferências retomáveis

- Autorização do usuário: implementar as funcionalidades possíveis para aproximar do oficial. Entregues funcionalidades concretas com API pública e motor próprio, sem novas dependências, publicação ou conexão a dados pessoais. A integração com Computadores continua sem contrato público de cadastro identificado.
- Diretórios são Entry explícita, com IDs/revisões e dev:inode, criação/associação inclusive vazios, rename/move de árvores e journal. Pais remotos não são criados implicitamente; diretórios precedem arquivos e efeitos estruturais forçam novo inventário. Conflito/exclusão/perda de acesso bloqueia descendentes; exclusão de árvores fica indisponível para preservar filhos concorrentes. Movimento local usa NOREPLACE e recuperação valida inode, inclusive contra destino ocupado por pasta não relacionada.
- SQLite migra até v4 preservando intenções antigas: campo directory tem default arquivo; upload_sessions guarda sessão/snapshot/offset somente com intenção running. A engine injeta persistência via segunda conexão do perfil, sem transação mantida durante HTTP. Finish limpa sessão e confirma vínculo/journal atomicamente; falha SQL conserva os três. URL de sessão não entra em relatórios, histórico ou mensagens de erro.
- Upload em blocos de até 8 MiB, offset consultado no servidor após reinício e Range validado. Sessão expirada pode ser recriada sob o mesmo ID; snapshot divergente/URL hostil/Range inválido bloqueiam. Resposta final verifica ID, tipo, caminho, tamanho e checksum. Downloads ainda recomeçam. Corrigido diagnóstico de resposta final inválida para não conservar uma URL privada em erro reqwest.
- Progresso de bytes por canal da engine para a GUI, com arquivo/direção e espera por validação após 100%; não confirma journal. about.get fornece nome/email/uso/limite, mantidos em memória; ausência de limite não vira valor inventado. Consulta disponível nas configurações e em account na CLI; cota pode incluir outros serviços ou organização.
- Erros tipados de transporte/429/5xx mantêm modo automático com esperas 30/60/120/240/300s e Retry-After numérico mínimo. Erros permanentes interrompem automação; watch persiste pausa para retomada explícita. Causas são preservadas na propagação do executor. Pausa compartilhada entre UI/CLI/watch é consultada entre operações: arquivo atual termina e o próximo não reserva ID nem inicia.
- Registrados contratos de diretórios/retomada, ADR 0005 e fontes oficiais de pastas/uploads/about. Atualizados roteiro, lacunas, paridade, aceitação, guias e READMEs das crates. Histórico de validação anterior preservado.
- Validação final: scripts/check.sh passou (fmt, Clippy -D warnings, 110 entradas de teste incluindo helper de subprocesso, build release). Distribuição: 33 API/telemetria, 32 espelhamento, 11 configuração, 8 filesystem, 11 SQLite/helper, 11 domínio e 4 UI. Inclui HTTP de upload >8 MiB com reinício/Range desatualizado/expiração, arquivo zero, URL hostil e proteção do diagnóstico; migração e rollback; pausa e retry sem relógio real no planejador.
- Instalação do binário final em prefixo temporário validada com cliente sintético 0600, configure sem --oauth, migração CLI de SQLite v2 para v4 com intenção antiga preservada, history/status/help e desktop-file-validate. Serviço não ativado; sem navegador/chaveiro/rede Google. Quatro capturas refeitas com XDG descartável; preferências e configurações inspecionadas. 77 links internos válidos antes desta entrada.
- Durante o desenvolvimento, a primeira rodada detectou a expectativa antiga de schema v2 e um fixture de migração que ainda continha a tabela v4; ambos corrigidos preservando os cenários. Ajustado lint de padrão de versões para intervalo. As baterias finais acima passaram, sem apresentar essas falhas intermediárias como aprovação.
- Pendências: homologar Google real (ETag/If-Match, sessões, renovação, quotas, falhas e serviço) com cliente e pasta de teste escolhidos pelo usuário. Streaming/FUSE, cursor incremental/watcher, nomes duplicados, múltiplas raízes/contas simultâneas, compartilhamentos e exclusão segura de árvores continuam ausentes. Esta etapa amplia a prévia; não é versão final nem comprova paridade total.

## 2026-10-26 — Retomada segura de downloads binários

- Implementado download parcial por `Range` para arquivos binários em intenção running: tamanho do staging privado define offset local; resposta exige HTTP 206 e `Content-Range` exato antes de anexar bytes. Corpo completo passa por tamanho/MD5 antes de quarentena e rename sem substituição. Nenhuma migração SQLite necessária.
- Falha de leitura da resposta é classificada como transitória com diagnóstico sanitizado e permite retry com espera. Staging parcial permanece; staging completo com checksum divergente é descartado sem alterar a cópia confirmada. Staging por symlink/hard link e tamanho excedente é bloqueado. Referências Google e KeepBoth conservam o fluxo de download integral.
- Testes novos cobrem reinício da engine e retomada no byte correto, respostas Range inválidas, checksum divergente preservando arquivo antigo, symlink/hard link e classificação do erro de transporte. Fonte pública e contrato em `docs/specs/resumable-download.md`; aceitação real adicionada. Atualizados roteiro, lacuna G08, paridade P07, guia e arquitetura.
- Validação: `scripts/check.sh` passou após correções de formatação/lint, com 115 entradas de teste incluindo helper de subprocesso, Clippy sem warnings e build release. Bateria final repetida após o teste de hard link: passou integralmente. Nenhum login, arquivo pessoal ou transferência Google real foi usado.
- Pendente: homologar respostas Range/ETag e retomada no Google real; inventário incremental, nomes duplicados, várias raízes, streaming e registro em Computadores continuam abertos ou limitados conforme o roteiro.

## 2026-10-26 — Watcher local para antecipar ciclos

- Acrescentado `LocalWatcher` com inotify recursivo usando Rustix já presente no workspace. Observa eventos de escrita, criação, remoção e movimento em diretórios da raiz; ignora `.gdrive-clone-recovery`. Não usa os eventos como inventário nem inferência de exclusão.
- O comando `watch` acorda um ciclo ao receber evento local e reconstrói a árvore de observação. O intervalo configurado continua fazendo inventários remotos completos como fallback para eventos perdidos, limites de inotify e mudanças feitas no site. Erros ao iniciar/usar inotify mantêm polling. Retry e pausa não são antecipados por evento.
- Teste local cobre evento em subdiretório e ausência de despertar por staging de recuperação. Documentação atualizada em lacuna, roteiro, guia, arquitetura e protocolo. Ativada feature `event` do Rustix existente; sem crate/dependência nova.
- Validação final: `scripts/check.sh` passou após formatação, Clippy, 116 entradas de teste incluindo helper de subprocesso e build release. 82 links internos conferidos sem ausências. Nenhum login ou arquivo pessoal acessado.
- Limites: o inventário remoto permanece completo e periódico; observação local depende do limite de watches do kernel. A falha de observação recua para polling sem alterar a política de reconciliação.

## 2026-10-26 — Cache por cursor Changes

- Implementada a primeira etapa do inventário incremental: `getStartPageToken` antes do inventário inicial; `changes.list` completo/paginado no ciclo seguinte; ciclos sem eventos reutilizam Entries persistidas e avançam cursor sem varrer toda a árvore remota. Qualquer evento invalida o cache e refaz inventário completo, preservando a regra atual contra exclusões inferidas. Eventos de fora da raiz também causam nova varredura.
- SQLite migra transacionalmente v4→v5 com `remote_inventory` por ID e `remote_cursor`. Atualização após mudanças grava snapshot/cursor na mesma transação; cache ausente inicia baseline. Cursor 400/410 é refeito somente após novo cursor e inventário completo; falha de varredura mantém o snapshot/cursor anterior. Nenhuma dependência nova.
- Engine injeta a persistência da mesma raiz/perfil ao Drive API. Testes simulam token inicial, feed vazio, paginação, alteração com inventário, cursor rejeitado e erro de inventário sem avanço; state-store verifica round-trip, migração e rollback atômico.
- Atualizados README, roteiro, G08/P07, arquitetura, guia de aceitação, fontes oficiais e especificações. Limite declarado: evento individual ainda não atualiza objeto isolado; qualquer mudança faz listagem recursiva completa. Mudanças fora da raiz também invalidam cache.
- Validação: `scripts/check.sh` passou: 122 entradas de teste incluindo helper, Clippy sem warnings e build release. Verificador de links encontrou 0 ausências. Sem login ou chamadas à conta real.
- Próximos passos: mapear eventos por ID/ancestral para evitar inventário completo após qualquer mudança, medir quotas/custos; homologar API Changes real. Nomes duplicados e múltiplas raízes continuam pendentes.

## 2026-10-26 — Aplicação incremental de mudanças por ID

- A Changes API agora entrega mudanças de arquivo ao cache sem varredura recursiva quando a árvore estrutural permanece válida. Atualiza conteúdo, nome e parent para IDs existentes/conhecidos; eventos de remoção retiram o ID do cache e continuam sujeitos à consulta de status/lixeira da engine. Eventos de pastas, mudanças para parent desconhecido em objeto conhecido, metadata insuficiente e colisões acionam inventário completo.
- O armazenamento SQLite aplica delta de Entries e cursor na mesma transação. Mudanças sem impacto na pasta avançam o cursor sem regravar inventário. Falha de SQLite não devolve estado parcial ao ciclo e conserva cursor/cache anteriores. Nenhuma dependência nova.
- Testes HTTP cobrem edição/rename/move em arquivo dentro de pasta conhecida sem chamar files.list, remoção, fallback estrutural e fallback após colisão. Teste de state-store injeta falha no update de cursor e confirma rollback das Entries do delta.
- Documentação revisada: contrato Changes, schema v5, guia, roteiro, G08/P07, aceitação e fonte oficial. Ficam fora: feed push, atualizar IDs individuais de subárvores quando uma pasta muda e homologação real.
- Validação final: `scripts/check.sh` passou com 124 entradas de teste incluindo helper, Clippy sem warnings e build release. Sem conta real/rede Google.

## 2026-10-26 — Reescrita incremental de subárvores conhecidas

- Eventos de renomeação/movimento de diretórios já cacheados e dentro de pais conhecidos agora reescrevem caminhos dos descendentes no cache por ID, inclusive sequências de movimento do pai e depois da pasta filha. O delta grava todos os Entries afetados e cursor na mesma transação.
- Criação/remoção/lixeira de pasta, caminho que sai da raiz, parent desconhecido, ciclo potencial, metadados insuficientes ou colisão continuam exigindo inventário completo. Isso é necessário para descobrir descendentes que possam entrar/sair e preservar a política de acesso/exclusão.
- Teste HTTP valida movimento de pai e subpasta com arquivos descendentes sem chamada a files.list; cobertura mantém fallback estrutural e rollback de delta/cursor.
- Validação: `scripts/check.sh` passou com 125 entradas de teste incluindo helper, Clippy e build release. Sem acesso a conta Google. Mudanças de diretório ainda precisam de homologação da API Changes real.

## 2026-10-26 — Seleção de pastas homônimas

- O seletor remoto agora mostra o ID completo junto ao nome quando há pastas com nomes iguais na mesma lista. A trilha, a configuração e o ID usado para navegar continuam separados do rótulo visual.
- Teste de UI cobre rótulos distintos para homônimos e nome sem alteração para pasta única. Isso permite selecionar raízes ambíguas sem mudar o contrato do motor.
- Limite: nomes duplicados no inventário de sincronização continuam bloqueados; ainda é necessária uma projeção que preserve o nome remoto em operações posteriores.
- Validação: `scripts/check.sh` passou (fmt, Clippy, 126 entradas de teste incluindo helper e build release). Nenhuma conta real foi usada.
# 2026-10-26 — Preparação da distribuição pública GPL

- Proprietário autorizou publicação pública no GitHub sob GPL-3 e pacotes AppImage, deb, rpm e Arch. Adicionada GPL-3.0-only, inventário de licenças das crates e avisos das fontes embarcadas; nenhum cliente OAuth ou token foi incluído.
- Criadas receitas de empacotamento, desktop/MIME/serviço para instalação de sistema, AppDir e workflow de prerelease. O login público continua indisponível até o responsável providenciar cliente OAuth Desktop e a verificação necessária; release identificado como prévia.
- Validação local: `scripts/check.sh` passou (fmt, Clippy, testes e build release); `bash -n` e `desktop-file-validate` passaram; deb, rpm e AppImage gerados; conteúdo dos pacotes inspecionado; AppImage respondeu a `--help`. Arch e workflow GitHub ainda pendentes de tag pública.
- Próximo passo: publicar o repositório e tag, verificar os quatro artefatos produzidos pelo workflow e homologar instalação nas distribuições com dados sintéticos.
- O primeiro workflow da tag v0.1.0 falhou antes do empacotamento Ubuntu porque rustfmt não estava instalado no toolchain do runner. Adicionados rustfmt e Clippy explicitamente ao setup; versão da prévia ajustada para 0.1.1. `scripts/check.sh` passou novamente localmente. A falha da primeira tentativa fica registrada, sem tratá-la como release publicado.

## 2026-10-26 — Prévia pública v0.1.2

- Repositório público e release v0.1.2 publicados: AppImage, deb, rpm, pacote Arch e símbolos de depuração Arch. GitHub reconheceu GPLv3. Nenhuma credencial OAuth ou token foi incluído.
- O CI v0.1.1 produziu os pacotes Ubuntu, mas o Arch falhou na linkedição de aws-lc-sys por LTO do makepkg. PKGBUILD v0.1.2 desativa LTO; o workflow completo passou. Avisos gerados por cargo-about e licenças das fontes foram incluídos nos pacotes.
- Baixados os artefatos do release: conteúdo deb/rpm/Arch inspecionado; AppImage executou `--help` após marcar o download como executável. `scripts/check.sh` passou localmente para v0.1.2; instalação/GUI em distribuições diferentes e operações Google reais não foram testadas.
- Próximo passo: fornecer cliente OAuth Desktop do projeto e homologar login, transferências, recuperação e serviço com dados sintéticos em distribuições alvo.

## 2026-10-26 — Política de Privacidade pública

- Preparadas página inicial e Política de Privacidade em HTML estático no diretório `docs/`, com descrição do escopo Drive, dados acessados, finalidade, armazenamento local, chaveiro, exclusão e contato. Links no README e na distribuição.
- Conteúdo conferido contra OAuth, engine e estado local; links relativos das duas páginas válidos. Sem login, conta Google ou arquivo pessoal acessado nesta tarefa.
- GitHub Pages ativado a partir de `main/docs`: página inicial e política responderam HTTP 200 sem login, com HTTPS e links válidos. URL pública: `https://goncalvesl.github.io/gdrive-clone/privacy/`. Para verificação OAuth pública, ainda é necessário domínio próprio verificável por DNS conforme exigência atual do Google.

## 2026-10-26 — Termos de Serviço públicos

- Adicionada página HTML de Termos de Serviço da prévia, sem impor restrições adicionais à GPL. Página inicial, política, mapa da documentação e README apontam para a URL pública.
- GitHub Pages publicou `https://goncalvesl.github.io/gdrive-clone/terms/`; página inicial, política e termos responderam HTTP 200 sem login. Links internos locais conferidos. Sem login ou dados pessoais acessados.
- Pendente para verificação OAuth pública: hospedar as páginas em domínio próprio comprovado por DNS, conforme exigência atual do Google.

## 2026-10-26 — Logo original para o aplicativo e OAuth

- Substituído o ícone SVG por uma pasta com setas de sincronização em cores próprias, sem marcas Google. Gerado PNG quadrado de 120 × 120 px (2.956 bytes) para o campo de logo OAuth; a página inicial usa os mesmos bytes. Origem e licença registradas em ATTRIBUTION.
- SVG analisado, PNG inspecionado visualmente; formato, dimensão e tamanho conferidos. GitHub Pages publicou a imagem com HTTP 200 e `image/png`; download público idêntico ao arquivo do projeto. Nenhuma conta Google foi acessada.
- Pendente: enviar manualmente o PNG na configuração de Branding do projeto Google e publicar nova versão dos pacotes para incorporar o ícone atualizado; a release v0.1.2 mantém o ícone anterior.

## 2026-10-26 — Cliente OAuth fora do repositório

- Confirmado apenas o formato do arquivo externo `/home/goncalvesl/cliente.json`: cliente OAuth Desktop com campos `client_id` e `client_secret`; valores não foram impressos nem copiados.
- `.gitignore` agora cobre `cliente.json` e `oauth-client.json` na raiz. `git check-ignore` confirmou ambas as regras.
- O cliente continua fora do Git; fornecê-lo à instalação por caminho externo permanece o fluxo documentado. Publicação OAuth ainda depende das exigências de verificação do Google.

## 2026-10-26 — Cliente OAuth público nos pacotes v0.1.3

- Cliente Desktop agora aceita JSON apenas com `installed.client_id`; login e renovação omitem `client_secret` vazio. JSON antigo com segredo continua compatível.
- Receitas geram JSON mínimo para AppImage/deb/rpm/Arch a partir de ID ou arquivo externo. O workflow exige `GDRIVE_OAUTH_CLIENT_ID`, configurado como variável pública do repositório sem imprimir o ID.
- Validação: `scripts/check.sh` passou; workflow publicou AppImage, deb, rpm e Arch. Baixados os quatro formatos e confirmado que carregam o ID configurado sem `client_secret`; AppImage executa `--help`.
- O HTML de verificação Google também foi publicado na raiz de `goncalvesl.github.io`; a URL exata respondeu HTTP 200 com o conteúdo esperado.
- Pendente: concluir verificação OAuth e homologar login/transferências públicas; nenhuma conta Google foi acessada nesta tarefa.
