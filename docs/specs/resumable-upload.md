# Upload por blocos e retomada

Contrato local de 2026-10-26, baseado na [documentação oficial](https://developers.google.com/workspace/drive/api/guides/manage-uploads). Homologação Google real separada dos testes HTTP.

A intenção em execução e o snapshot privado antecedem o envio. SQLite v5 guarda URL de sessão, expectativa, destino e offset confirmado. A URL é dado privado: nunca incluí-la em histórico, relatório ou mensagem de erro. Validar host HTTPS Google também ao recarregar do banco; não seguir redirects.

Enviar blocos de 8 MiB, limitando memória; consultar a sessão após reinício com PUT vazio. Range do servidor determina a retomada, nunca a quantidade que o cliente tentou enviar. Offset salvo é uma dica, não prova. Uma sessão expirada reinicia sob o mesmo ID após inspeção. Expectativa/destino diferentes bloqueiam reutilização. Não usar tokens ou conteúdo do arquivo no banco.

Falha conserva intenção, sessão e snapshot. Resposta final só confirma após validar ID/tipo/checksum/tamanho. Finish remove a sessão atomicamente com a confirmação do vínculo. Recuperação que já observou o arquivo final também remove o registro. Dados de progresso não substituem confirmação.

Testar interrupção antes/depois de aceite parcial, reinício com offset local desatualizado, sessão expirada, Range inválido, destino hostil e resposta final divergente. Medir concorrência e If-Match no Google real antes de anunciar homologação.

Falhas transitórias de transporte, HTTP 429 e 5xx permitem nova tentativa automática com esperas de 30, 60, 120, 240 e até 300 segundos, respeitando um Retry-After numérico maior. Erros de autenticação, permissão, checksum e precondição interrompem o modo automático. Nenhum retry dispensa reinspeção do journal. A política recebe contagem e duração mínima, sem relógio real no domínio.
