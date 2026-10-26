# Preferências do desktop

Atualizado em 2026-10-26. Foco autorizado: aproximar a tela de preferências da organização visual pública do Drive para computador, mantendo o motor atual. Não declarar identidade visual exata ou paridade funcional.

## Referências e evidência

- [Ajuda oficial: streaming e espelhamento](https://support.google.com/drive/answer/13401938?hl=pt-BR): navegação para as pastas do Drive e seleção dos modos de sincronização.
- [Ajuda oficial: configurações](https://support.google.com/drive/answer/13470231?hl=pt-BR): preferências, conta e configurações avançadas.
- [Captura pública na Comunidade Google Drive](https://support.google.com/drive/thread/154970342/configured-to-mirror-but-mirroring-not-happening?hl=en), [imagem consultada](https://storage.googleapis.com/support-forums-api/attachment/thread-154970342-15296822923950062903.png): janela clara, cabeçalho compacto, navegação lateral, item selecionado arredondado azul e painel com modos de sincronização. A captura mostra Windows; build do cliente não identificado. Não comprova aparência da versão atual.

Referências consultadas em 2026-10-26. A imagem externa foi apenas inspecionada, não incorporada ao aplicativo nem aos screenshots do projeto. Código, símbolo do aplicativo, ícones de ajuda/configuração/conta e ilustração de computador foram desenhados originalmente com egui. Sem copiar logo, fonte proprietária ou imagem do fornecedor.

## Organização implementada

A janela abre em **Google Drive**, com tema claro, cabeçalho de identidade própria e ações de ajuda, configurações e conta. Sem conexão configurada, o cabeçalho oferece **Fazer login**. A navegação principal apresenta **Meu computador / Pastas do computador** e **Google Drive / Pastas do Google Drive**. Sincronização e atividade ficam como ações secundárias.

O painel central apresenta acesso à pasta local, informação sobre uso do disco, as opções de streaming e espelhamento e os locais remoto/local. Streaming aparece desativado e identificado como indisponível. O espelhamento permanece o único modo funcional. A seleção remota por nomes abre uma janela de navegação; IDs ficam restritos ao tooltip e à configuração manual.

**Meu computador** informa que este cliente ainda não registra dispositivos na seção Computadores do site, com **Adicionar pasta** desativado, link para o site e acesso ao espelhamento existente. A [investigação da API pública](computers.md) não identificou uma operação documentada para esse cadastro. Não converter silenciosamente uma pasta local em backup nem anunciar suporte ao Google Fotos.

**Configurações**, acessível pelo ícone do cabeçalho, reúne conta, propagação de exclusões, recuperação e campos de desenvolvimento. A tela principal não exige manipular credenciais ou IDs. Não inventar email, avatar de usuário real, quota ou estado de conexão: uma conexão configurada não é prova de token válido.

## Salvar e cancelar

O rodapé mantém **Cancelar** e **Salvar**. Alterações de pasta e exclusões permanecem em memória até salvar usando o comando compartilhado da engine. Cancelar restaura as opções salvas sem escrever configuração ou iniciar sincronização. Sem configuração anterior, limpa a seleção de pastas. Login e desconexão são ações explícitas de conta e não são desfeitos pelo cancelamento das opções.

Alterar opções interrompe o agendamento automático de novos ciclos. Os botões de sincronização permanecem bloqueados enquanto houver alterações não salvas. Durante trabalho de rede/disco, a edição é desativada; fechar a janela de escolha não representa conclusão da listagem.

## Resultados verificáveis e limites

- Renderizar as páginas Google Drive, Meu computador e Configurações com XDG descartável, sem navegador, chaveiro ou rede de sincronização.
- Inspecionar contraste, alinhamento, ícones sem glifos ausentes, conteúdo e rodapé na janela padrão de 940 × 720; manter rolagem em janelas menores.
- Navegar entre as duas páginas principais sem alterar o arquivo de configuração nem iniciar worker.
- Cancelar opções editadas sem criar a nova pasta nem substituir a configuração anterior.
- Executar fmt, Clippy, suíte existente e build otimizado. Os testes de UI verificam navegação/cancelamento em egui sem sessão Google.

Capturas locais: [preferências iniciais](../screenshots/setup.png), [espelhamento com dados sintéticos](../screenshots/preferences.png), [Meu computador](../screenshots/computer.png) e [Configurações](../screenshots/settings.png). São evidências da implementação local. Não existe comparação pixel a pixel nem observação de binário oficial identificado. O escopo de sincronização continua uma pasta dedicada; o original permite outros fluxos ainda ausentes no motor.

## Progresso e dados da conta

A GUI recebe telemetria de bytes pelo mesmo ciclo da engine. Upload indica bytes lidos para o transporte; download indica bytes recebidos pelo leitor. Chegar ao tamanho esperado mostra espera por validação/confirmação, nunca conclui o journal por si só. Leituras com erro não produzem progresso fictício. Eventos de UI não alteram precondições nem bloqueiam transferências se a janela deixa de recebê-los.

Dados da conta e armazenamento, quando consultados, vêm de about.get, permanecem em memória e não são inventados a partir da configuração. Limite ausente não deve ser apresentado como um número ou como espaço ilimitado confirmado. Falhar ao consultar esses dados não transforma uma listagem de pastas anterior em vazio. Capturas e testes usam dados sintéticos.

A ação Pausar no cabeçalho permanece disponível durante um ciclo, mesmo com os campos de preferências desativados. Ela persiste a mesma flag da CLI/serviço; o executor confirma o arquivo atual e para antes do próximo efeito. Erros transitórios mantêm agendamento com espera; erros permanentes desativam o modo automático. Em Configurações, Atualizar dados da conta consulta o comando compartilhado account_info; a CLI oferece account.
