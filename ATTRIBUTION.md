# Origem dos recursos

- `assets/drive-linux.svg`: desenho vetorial original criado para este projeto, sem recursos ou marcas do Google/ArtCraft incorporados. Licença GPL-3.0-only do projeto.
- Interface: fontes padrão incluídas pelo egui/epaint_default_fonts. Os avisos de Hack, Noto Emoji, Ubuntu e emoji-icon-font estão em `packaging/licenses/` e acompanham os pacotes.
- Dependências Rust e SQLite bundled: versões e checksums em Cargo.lock; cada dependência mantém sua própria licença. A lista de licenças declaradas está em `THIRD_PARTY_LICENSES.md`; textos e avisos coletados estão em `THIRD_PARTY_NOTICES.html`.

- `apps/drive-desktop/src/ui.rs`: símbolo do aplicativo, ícones de ajuda/configurações/conta e ilustração de computador desenhados originalmente com primitivas egui; nenhum bitmap, logo ou ícone proprietário incorporado. Referências públicas da organização das preferências e limites de comparação em [contrato visual](docs/specs/desktop-preferences.md). Licença GPL-3.0-only do projeto.
