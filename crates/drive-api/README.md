# drive-api

Adaptador Drive v3 e OAuth Desktop. Remote é a interface usada pelo engine e pelos testes; Drive é sua implementação HTTP.

Inventário recursivo paginado, cache com cursor Changes, consulta por ID, reserva de IDs, download parcial por Range, upload por sessão resumable e envio à lixeira. Tipos Google nativos viram referências de navegador; nomes duplicados/inseguros ou colisões da projeção bloqueiam o inventário. HTTP 403/404 de um item é Unknown, nunca lixeira presumida.

OAuth usa PKCE, callback loopback e secret-tool. O refresh token não é serializado no banco. Falhas de autenticação/rede não incluem tokens no diagnóstico. Updates precisam de ETag e usam If-Match; falta homologação real.

```sh
cargo test -p drive-api
```

Testes usam HTTP de loopback com credencial sintética, permitido apenas sob cfg(test). Em produção, uploads aceitam somente os hosts HTTPS Google explicitamente permitidos. Sessões/offsets podem ser persistidos via UploadSessionStore injetado pela engine; blocos de 8 MiB e Range revalidado na retomada.

reference_url valida referências locais; URLs externas são recusadas. Renomeação de nativos atualiza metadados; o conteúdo do JSON não é enviado para substituir documentos.

about.get fornece usuário/cota. Operações de pasta usam IDs reservados e metadata/If-Match. ProgressSink informa bytes sem confirmar o journal. Falhas HTTP/rede tipadas conservam retry mínimo sem expor URLs de sessão.
