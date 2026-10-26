# Distribuição da prévia

O código do projeto e os recursos originais são distribuídos sob **GPL-3.0-only**; veja [LICENSE](../LICENSE). As dependências mantêm suas próprias licenças, listadas em [THIRD_PARTY_LICENSES.md](../THIRD_PARTY_LICENSES.md), [THIRD_PARTY_NOTICES.html](../THIRD_PARTY_NOTICES.html) e [ATTRIBUTION.md](../ATTRIBUTION.md). Os quatro formatos são para Linux x86_64.

A [página pública do aplicativo](https://goncalvesl.github.io/gdrive-clone/) contém a [Política de Privacidade](https://goncalvesl.github.io/gdrive-clone/privacy/) exigida para apresentar o fluxo OAuth aos usuários. Para uma verificação OAuth pública, o Google também pode exigir domínio próprio verificado por DNS; a hospedagem padrão em `github.io` não comprova essa propriedade.

Os [Termos de Serviço públicos](https://goncalvesl.github.io/gdrive-clone/terms/) estão vinculados à página inicial e à política.

## Instalar

Baixe os arquivos da [versão de prévia no GitHub](https://github.com/goncalvesl/gdrive-clone/releases). Os pacotes `.deb`, `.rpm` e `.pkg.tar.zst` instalam `drive-desktop`, entradas de menu, handler de referências Google, ícone, MIME e uma unidade systemd de usuário. A unidade não é ativada automaticamente. O AppImage pode ser executado após `chmod +x` e contém apenas a aplicação e avisos: integração com menu, MIME e serviço fica a cargo do usuário.

Um cliente OAuth Desktop do projeto **não está incluído** na publicação. Sem ele, o login não está disponível. O responsável pela distribuição precisa providenciar esse cliente e a verificação OAuth exigida antes de oferecer login público. Quem já dispõe de um cliente para desenvolvimento pode fornecer o caminho via `GDRIVE_OAUTH_CLIENT_FILE` ou colocar `oauth-client.json` no diretório XDG de configuração do aplicativo, com permissão 0600. Não adicione credenciais ao repositório ou a issues. Consulte [ADR 0004](adr/0004-login-e-selecao.md).

O binário chama `secret-tool` para armazenar o refresh token no Secret Service. Instale a ferramenta e tenha um serviço de chaveiro funcional no desktop. Em alguns ambientes, seletores de pasta nativos requerem `zenity` ou `kdialog`; o campo manual continua disponível.

Esta é uma prévia de espelhamento. Login/listagem reais foram relatados pelo usuário; transferências, retomada e recuperação ainda não foram homologadas no Drive real. Faça primeiro a [aceitação com dados sintéticos](acceptance.md) em uma pasta dedicada.

## Produzir pacotes

O workflow [release.yml](../.github/workflows/release.yml) compila no Ubuntu 22.04 para `.deb`, `.rpm` e AppImage; gera o pacote Arch em container Arch Linux. Uma tag de versão como `v0.1.2` dispara os quatro artefatos e publica um prerelease. `workflow_dispatch` permite testar os três builds Ubuntu sem publicar.

Localmente, com Rust 1.99.1 e `nfpm` 2.48.0, `./scripts/package.sh deb` ou `rpm` grava em `dist/`. Com `appimagetool`, `./scripts/package.sh appimage` faz o AppImage. No Arch, `makepkg -p packaging/PKGBUILD` requer a tag pública correspondente. `./scripts/check.sh` executa os checks do projeto. Os pacotes gerados e o AppImage não incluem cliente OAuth nem tokens.

As receitas nativas usam `/usr/bin/drive-desktop`. O instalador pessoal `scripts/install.sh` usa `~/.local/bin` e sua própria unidade systemd. Não ative simultaneamente os dois serviços para o mesmo perfil.
