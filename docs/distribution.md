# Distribuição da prévia

O código do projeto e os recursos originais são distribuídos sob **GPL-3.0-only**; veja [LICENSE](../LICENSE). As dependências mantêm suas próprias licenças, listadas em [THIRD_PARTY_LICENSES.md](../THIRD_PARTY_LICENSES.md), [THIRD_PARTY_NOTICES.html](../THIRD_PARTY_NOTICES.html) e [ATTRIBUTION.md](../ATTRIBUTION.md). Os quatro formatos são para Linux x86_64.

A [página pública do aplicativo](https://goncalvesl.github.io/gdrive-clone/) contém a [Política de Privacidade](https://goncalvesl.github.io/gdrive-clone/privacy/) exigida para apresentar o fluxo OAuth aos usuários. Para uma verificação OAuth pública, o Google também pode exigir domínio próprio verificado por DNS; a hospedagem padrão em `github.io` não comprova essa propriedade.

Os [Termos de Serviço públicos](https://goncalvesl.github.io/gdrive-clone/terms/) estão vinculados à página inicial e à política.

O ícone original está em `assets/drive-linux.svg`; o PNG quadrado de 120 × 120 px para o campo **Logo do app** na configuração OAuth está em [`assets/gdrive-clone-oauth-120.png`](../assets/gdrive-clone-oauth-120.png). O mesmo desenho aparece na página inicial.

## Instalar

Baixe os arquivos da [versão de prévia no GitHub](https://github.com/goncalvesl/gdrive-clone/releases). Os pacotes `.deb`, `.rpm` e `.pkg.tar.zst` instalam `drive-desktop`, entradas de menu, handler de referências Google, ícone, MIME e uma unidade systemd de usuário. A unidade não é ativada automaticamente. O AppImage pode ser executado após `chmod +x` e contém apenas a aplicação e avisos: integração com menu, MIME e serviço fica a cargo do usuário.

A prévia v0.1.2 não inclui cliente OAuth. A v0.1.3 inclui somente o `client_id` público do projeto em um JSON mínimo. Esse ID pode ser extraído dos pacotes; o `client_secret` do arquivo original, os tokens e o arquivo original não são incluídos. O login ainda depende da configuração e verificação OAuth do Google. Para desenvolvimento, `GDRIVE_OAUTH_CLIENT_FILE` em tempo de execução ou `oauth-client.json` no diretório XDG de configuração continuam disponíveis. Não adicione credenciais ao repositório ou a issues. Consulte [ADR 0004](adr/0004-login-e-selecao.md).

O binário chama `secret-tool` para armazenar o refresh token no Secret Service. Instale a ferramenta e tenha um serviço de chaveiro funcional no desktop. Em alguns ambientes, seletores de pasta nativos requerem `zenity` ou `kdialog`; o campo manual continua disponível.

Esta é uma prévia de espelhamento. Login/listagem reais foram relatados pelo usuário; transferências, retomada e recuperação ainda não foram homologadas no Drive real. Faça primeiro a [aceitação com dados sintéticos](acceptance.md) em uma pasta dedicada.

## Produzir pacotes

O workflow [release.yml](../.github/workflows/release.yml) compila no Ubuntu 22.04 para `.deb`, `.rpm` e AppImage; gera o pacote Arch em container Arch Linux. Uma tag de versão dispara os quatro artefatos e publica um prerelease. `workflow_dispatch` permite testar os três builds Ubuntu sem publicar. Antes de qualquer nova execução, configure a variável de repositório `GDRIVE_OAUTH_CLIENT_ID` com o ID do cliente OAuth Desktop. O workflow falha se ela estiver ausente.

Localmente, com Rust 1.99.1, Python 3 e `nfpm` 2.48.0, `./scripts/package.sh deb` ou `rpm` grava em `dist/`. Com `appimagetool`, `./scripts/package.sh appimage` faz o AppImage. No Arch, `makepkg -p packaging/PKGBUILD` requer a tag pública correspondente. Defina `GDRIVE_OAUTH_CLIENT_ID` ou `GDRIVE_OAUTH_CLIENT_FILE` no ambiente antes de executar a receita. Se receber o arquivo original, a receita lê somente `installed.client_id` e escreve um novo JSON mínimo em `/usr/share/gdrive-clone/oauth-client.json`; o arquivo original permanece fora do repositório e dos artefatos. Sem essas variáveis, builds locais são gerados sem login configurado. `./scripts/check.sh` executa os checks do projeto. O AppImage contém o JSON mínimo no mesmo caminho relativo ao binário. Nenhum pacote inclui tokens.

As receitas nativas usam `/usr/bin/drive-desktop`. O instalador pessoal `scripts/install.sh` usa `~/.local/bin` e sua própria unidade systemd. Não ative simultaneamente os dois serviços para o mesmo perfil.
