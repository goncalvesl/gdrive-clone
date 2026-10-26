# Paridade com o Drive para computador

Atualizado em 2026-10-26. Referência desejada: fluxos do cliente oficial de espelhamento e streaming. Nenhuma versão do binário oficial foi observada nesta sessão; o usuário relatou conexão e leitura de pastas numa conta real; o agente não reproduziu esse cenário. Não há percentual de paridade nem declaração de versão final.

Estados: planejado, parcial, verificado, bloqueado. Verificado exige evidência no ambiente real do cenário anunciado. Testes mock comprovam o contrato local, não equivalência com o Drive oficial.

| ID | Capacidade | Estado | Evidência / limite |
|---|---|---|---|
| P01 | Conectar, renovar e desconectar conta | parcial | Login no navegador + PKCE/chaveiro; login real relatado pelo usuário, renovação/desconexão não homologadas. Cliente OAuth deve ser fornecido pela distribuição, não criado pelo usuário comum |
| P02 | Selecionar pasta para espelhamento | parcial | Navegação remota por nomes e trilha de subpastas; IDs internos; pastas homônimas na mesma lista são identificadas pelo ID no seletor. Seletores locais opcionais zenity/kdialog. Leitura real relatada pelo usuário; fluxo testado por HTTP simulado e renderização local |
| P03 | Criação/edição bidirecional preserva conteúdo | parcial | Testes com bytes reais, SQLite e remoto simulado; endpoints HTTP testados localmente; falta Google real |
| P04 | Renomear/mover preserva identidade | parcial | Testes nos dois sentidos; pastas vazias e movimentos de árvores com IDs estáveis testados localmente; exclusão de diretórios e movimentos para fora da raiz bloqueados |
| P05 | Exclusão recuperável; acesso perdido não apaga cópia | parcial | Lixeira/quarentena implementadas e testadas com remoto simulado; desligadas por padrão |
| P06 | Edições concorrentes preservam ambas as versões | parcial | Conflitos bloqueiam sobrescrita; KeepBoth testado com cópia local e próximo upload |
| P07 | Falhas/reinício convergem sem perda | parcial | Journal, cache/cursor Changes transacional, sessões de upload, retomada de download binário por Range, expiração, retry, rollback e efeitos antes/depois da resposta testados localmente; sem energia/falhas reais Google |
| P08 | Pausa, progresso e erros no desktop | parcial | Progresso por bytes separado da confirmação, usuário/cota via about, pausa entre arquivos e espera de retry; serviço autenticado real ainda não homologado |
| P09 | Streaming e fixação offline | planejado | Sem FUSE |
| P10 | Documentos Google, atalhos, nomes duplicados | parcial | Referências de navegador e handler MIME implementados/testados; sem edição offline; colisões e nomes duplicados bloqueiam o inventário |
| P11 | Múltiplas contas/shared drives | planejado | Um perfil ativo por vez; sem suporte simultâneo |
| P12 | Organização visual das preferências | parcial | Tema claro, cabeçalho, navegação Meu computador/Google Drive, opções centrais e rodapé Salvar/Cancelar; captura pública consultada, build oficial desconhecido. [Contrato e capturas locais](specs/desktop-preferences.md); sem comparação pixel a pixel |
| P13 | Registrar a máquina na seção Computadores do site | bloqueado | [REST/Discovery públicos consultados](specs/computers.md) não documentam cadastro de dispositivos. Nenhum teste de registro real; pastas comuns em Meu Drive não equivalem a esse recurso |
| P14 | Instalação Linux em formatos públicos | parcial | Receitas para AppImage, deb, rpm e Arch em [distribuição](distribution.md); artefatos e execução nas distribuições ainda precisam ser verificados. Sem cliente OAuth de distribuição, login público indisponível |

Evidência automatizada: suítes em crates/*/tests e drive-api/src/lib.rs, com contagem no devlog. Evidência visual: [configuração](screenshots/setup.png). Protocolo de aceitação real: [aceitação](acceptance.md). Para medir paridade, registrar plataforma/versão do aplicativo oficial, passos, resultado observado e artefatos sintéticos, e executar o mesmo cenário no cliente.
