# Usar a prévia de espelhamento

Esta prévia opera uma única pasta remota, com arquivos comuns, pastas (inclusive vazias) e referências a documentos Google. O usuário relatou login e leitura de pastas bem-sucedidos; transferências e recuperação reais ainda não foram homologadas. A sincronização não é backup: os mecanismos de recuperação reduzem riscos, mas a homologação deve começar numa pasta de teste.

## Requisitos

Linux com sessão gráfica X11 ou Wayland e OpenGL, navegador, `xdg-open`, Secret Service desbloqueado e o comando `secret-tool` do libsecret. Para compilar: Rust/Cargo 1.99 ou posterior, compilador C e dependências de desenvolvimento X11/Wayland/OpenGL da distribuição. O SQLite é compilado com o binário.

## Primeiro acesso

Abra o aplicativo, que apresenta as preferências de **Google Drive**, e clique em **Fazer login** no cabeçalho. Autorize o acesso na página Google e volte ao aplicativo. Em **Escolher pasta…**, navegue pelo **Meu Drive**, abra subpastas e clique em **Usar esta pasta**. Não é necessário copiar o ID da URL. A trilha de nomes permite voltar às pastas anteriores; IDs ficam disponíveis no tooltip para distinguir nomes repetidos. Sincronizar uma árvore que contém nomes duplicados ainda é bloqueado pelo motor.

Em **Local dos arquivos espelhados**, clique em **Alterar…** para escolher a pasta local. Sem seletor disponível, o caminho absoluto pode ser digitado em **Configurações → Desenvolvimento e configuração manual**. O seletor usa `zenity` ou `kdialog`, quando disponíveis; sem eles, o campo de caminho continua funcional. Comece com pastas de teste dedicadas. Clique em **Salvar** no rodapé; **Cancelar** restaura as opções salvas sem iniciar sincronização. Editar as opções desativa o modo automático e bloqueia novos ciclos da interface até salvar.

**Disponibilidade do login:** a distribuição precisa fornecer um cliente OAuth Desktop próprio do projeto. Esta árvore não inclui credenciais reais. Em uma instalação sem esse cliente nem uma configuração de desenvolvimento anterior, a ação de login encaminha às configurações e informa a limitação. O aplicativo não deve encaminhar o usuário comum para criar um projeto Google Cloud. A configuração manual fica recolhida em **Configurações → Desenvolvimento e configuração manual**.

O login abre o navegador e espera até três minutos. A senha Google não passa pelo aplicativo. O refresh token é guardado pelo `secret-tool` no Secret Service da sessão. Não há fallback para token em arquivo texto. Desconectar apaga a credencial local; para revogar o acesso no Google, use a página de segurança da conta.

A conexão é independente das raízes de sincronização. Escolher outra pasta preserva o login quando o arquivo de cliente OAuth é o mesmo, mas cria um journal independente. A configuração antiga é compatível: sua identidade anterior de chaveiro continua sendo usada. Trocar as raízes com intenções pendentes é bloqueado até recuperá-las; reconfiguração durante um ciclo também é bloqueada.

## Organização das preferências

A navegação lateral separa **Meu computador** e **Google Drive**. **Meu computador** informa que o aplicativo ainda não registra o dispositivo na seção Computadores do site; **Adicionar pasta** fica desativado. Não foi identificada uma operação documentada na API pública para esse registro; veja a [investigação](specs/computers.md). Criar uma pasta com o nome da máquina em Meu Drive não equivale a aparecer naquela seção. Para o espelhamento funcional, use **Google Drive**. A opção **Transferência de arquivos por streaming** também permanece desativada. Essa apresentação não adiciona suporte a esses modos.

O ícone de configurações no cabeçalho reúne conta, exclusões, recuperação e opções de desenvolvimento. **Conta** abre as mesmas configurações. **Ajuda** abre a documentação oficial no navegador. Os desenhos e a identidade do aplicativo são próprios do projeto; a organização visual segue [referências públicas](specs/desktop-preferences.md).

## Primeiro ciclo

Abra **Sincronização** na navegação lateral. Use **Ver prévia** antes de **Sincronizar agora**. Arquivos inicialmente existentes nos dois lados com mesmo caminho e conteúdo são associados; conteúdos diferentes geram conflito. O inventário é completo antes de planejar efeitos. O banco e a configuração podem ser criados durante uma prévia, mas o conteúdo dos arquivos não é alterado e não são reservados IDs remotos.

A propagação de exclusões vem desativada. Ao ativá-la, remoções locais de arquivos já associados enviam os arquivos remotos à lixeira. Exclusão remota só é propagada localmente quando o objeto tem evidência explícita de lixeira. Perda de acesso e movimento para fora da raiz não são tratados como exclusão.

## Conflitos e recuperação

A ação **Preservar ambas** mantém a versão local em `nome.conflict-<id>` e instala a versão remota no caminho correspondente. O próximo ciclo envia a cópia de conflito como novo arquivo. Os originais substituídos também ficam em `.gdrive-clone-recovery`.

Operações interrompidas são inspecionadas antes de repetir. Se o estado mudou, a execução bloqueia para evitar sobrescrever uma edição. Em **Configurações**, **Recuperar pendências** tenta concluir efeitos observados ou aposentar a intenção conservando a base anterior para novo planejamento. Uma criação cujo ID não pode ser consultado permanece bloqueada: reconecte a conta e restaure acesso antes de continuar.

A pasta `.gdrive-clone-recovery` contém arquivos `*-old` (originais preservados), possíveis downloads parciais e snapshots de uploads pendentes. Não é sincronizada. Snapshots de upload concluídos são removidos; originais não expiram automaticamente. Restaure os arquivos desejados copiando-os para um novo nome fora dessa pasta. Não remova arquivos de uma operação ainda pendente.

## CLI

```sh
drive-desktop login
drive-desktop folders
drive-desktop configure --local /home/voce/Drive --remote ID_DA_PASTA
drive-desktop plan
drive-desktop sync
drive-desktop status
drive-desktop history
```

Em desenvolvimento, `login --oauth /caminho/cliente.json` prepara a conexão antes de configurar pastas; `configure --oauth` permanece disponível. Sem esse argumento, o cliente fornecido pela aplicação ou a configuração anterior é usado.

`configure --propagate-deletions` ativa exclusões recuperáveis. `folders [ID_PAI]` lista subpastas; `resolve 'pasta/arquivo.txt'` preserva ambas as versões; `recover` inspeciona pendências. Caminhos usados por `resolve` são relativos à raiz, como aparecem no relatório.

## Segundo plano

O instalador copia o executável para `~/.local/bin`, a entrada do menu e um serviço de usuário. Para continuar sincronizando após fechar a janela:

```sh
systemctl --user daemon-reload
systemctl --user enable --now gdrive-clone.service
journalctl --user -u gdrive-clone.service -f
```

O serviço `watch` inicia ciclos após eventos locais e consulta Changes a cada 30 segundos. Mudanças em arquivos comuns e renomes/movimentos de pastas conhecidas atualizam o cache por ID; criação/remoção de pastas e ambiguidades disparam inventário completo. Requer configuração e chaveiro acessíveis nessa sessão. Escolha entre serviço e modo automático da janela; o lock impede execução simultânea, mas clientes concorrentes podem mostrar uma mensagem de ocupado.

```sh
drive-desktop pause
drive-desktop resume
systemctl --user disable --now gdrive-clone.service
```

A pausa é consultada entre operações, inclusive dentro de um ciclo; o arquivo em andamento termina e é confirmado antes de parar. Durante a transferência, **Pausar** no cabeçalho solicita essa pausa. O modo automático da GUI funciona enquanto a janela estiver aberta. O serviço é opcional e não é ativado pelo instalador.

## Dados e limitações

Conexão em `$XDG_CONFIG_HOME/gdrive-clone/account.json` (somente perfil e caminho do cliente, sem tokens). Configuração em `$XDG_CONFIG_HOME/gdrive-clone/config.json` e dados em `$XDG_DATA_HOME/gdrive-clone/<perfil>/state.sqlite`; defaults `~/.config` e `~/.local/share`. A raiz local é vinculada ao dispositivo/inode para detectar substituição ou desmontagem. Alterar as raízes ou cliente OAuth cria um perfil independente e preserva o anterior.

Links simbólicos, hard links, submontagens, nomes não UTF-8, arquivos especiais e nomes remotos duplicados bloqueiam o inventário. Não há FUSE/streaming, filtros, compartilhamentos, múltiplas contas simultâneas nem exclusão de árvores de pastas. Uploads dependem de ETag para atualizações concorrentes; sua disponibilidade/semântica ainda precisa de homologação real. Uploads usam blocos de até 8 MiB. A sessão é persistida com a intenção; depois de interrupção, o cliente consulta quantos bytes o servidor recebeu e continua desse ponto. Uma sessão expirada pode reiniciar sob o mesmo ID. A homologação real continua pendente. Downloads binários interrompidos retomam do staging privado após validar o intervalo remoto e o checksum final; referências Google e KeepBoth recomeçam.

Antes de considerar uma versão estável, executar a [aceitação real](acceptance.md).

## Documentos Google

Docs, Sheets e Slides são representados por arquivos locais .gdoc, .gsheet e .gslides. Outros tipos Google, incluindo atalhos, usam .gdrive-link. São referências JSON ao documento remoto, não exportações nem cópias editáveis offline. O instalador registra um handler MIME para abrir no navegador; também é possível usar `drive-desktop open-document /caminho/Documento.gdoc`.

Não altere o JSON: a aplicação apresenta conflito em vez de enviar seu conteúdo como atualização do documento. A ação Preservar ambas restaura a referência e guarda a edição local numa cópia. Renomear/mover a referência, mantendo a extensão, atualiza somente os metadados remotos. Excluir a referência de um documento associado pode enviar o documento real à lixeira se a propagação estiver habilitada. Arquivos locais de referência sem vínculo não são enviados como documentos binários novos. A URL é validada pelo handler; endereços externos e IDs malformados são recusados.

## Pastas e progresso

Pastas vazias agora são criadas e associadas nos dois sentidos. Renomear ou mover uma pasta dentro da raiz preserva o ID remoto e seu conteúdo. Conflito ou perda de acesso bloqueia a pasta e seus descendentes. Excluir uma árvore de pastas não é propagado nesta versão; a pasta permanece bloqueada para evitar atingir filhos adicionados concorrentemente. Consulte [diretórios](specs/directories.md).

O rodapé mostra o arquivo e bytes enviados/recebidos. O fim da barra significa fim do transporte, seguido por validação e confirmação: não significa que o journal já concluiu. No modo automático, falhas transitórias de rede/HTTP 429/5xx fazem nova tentativa após espera progressiva, respeitando Retry-After numérico. Falhas permanentes interrompem o modo automático da janela; watch persiste pausa até resume. Uma espera não descarta a intenção nem o snapshot pendente.

Após a primeira listagem, o aplicativo consulta os dados da conta; também é possível usar **Configurações → Atualizar dados da conta** ou `drive-desktop account`. Nome, e-mail e armazenamento são informados pelo Google e mantidos em memória. O uso pode incluir outros serviços Google ou cota organizacional. Se não houver limite informado, o aplicativo não inventa um valor. Nenhum token é exibido.
