# Fase 13 — Exportação TXT

## Escopo e decisões

A Fase 13 do plano prevê TXT para o MVP. SRT, VTT e JSON continuam como evolução futura. Nenhuma dependência, migration ou formato adicional foi adicionado.

O contrato Tauri `export_transcript(id, traditional)` e os serviços/componentes da Fase 9 foram preservados. A exportação continua local e executada em `spawn_blocking` no backend.

Fluxo implementado:

1. `meeting::details` consulta a reunião e prepara o nome sugerido e o texto escolhido: tradicional ou segmentos com timestamps/rótulos. Reuniões antigas sem segmentos usam o texto tradicional.
2. `commands::files` abre o diálogo nativo do Windows com filtro TXT, extensão padrão e confirmação de substituição. O usuário escolhe pasta e nome.
3. `PreparedTxtExport` mantém o texto preparado, mesmo se a transcrição mudar enquanto o diálogo estiver aberto. Cancelar retorna `false` sem escrita.
4. `StorageManager` valida o destino e escreve bytes UTF-8 sem BOM em arquivo temporário, sincroniza e substitui o destino. Em caso de erro, mantém o arquivo anterior e remove o temporário criado pela operação.
5. Erros de diálogo, caminho, permissão e escrita retornam ao fluxo de erro existente na interface.

Essa separação entre preparação do conteúdo, escolha do destino e escrita permite adicionar formatadores futuramente. SRT/VTT deverão usar timestamps disponíveis nos segmentos, sem inventar tempos para reuniões antigas que contenham somente texto. Não há comandos ou serializadores adicionais nesta fase.

## Nomes e proteção dos arquivos

O nome sugerido usa `transcricao-{titulo}.txt`, substitui caracteres inválidos por `_` e limita o título a 90 caracteres Unicode. Títulos vazios usam `transcricao.txt`. Acentos e emojis são preservados. O prefixo evita que títulos como `CON` produzam nomes reservados.

O destino deve ser absoluto, ter extensão TXT e nome válido. A validação rejeita travessia com `..`, controles, caracteres inválidos, streams alternativos (`arquivo.txt:stream.txt`), nomes reservados, espaços/pontos finais e nomes maiores que 255 unidades UTF-16. As regras de nomes reservados seguem a [documentação do Windows](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file).

A proteção existente impede exportar sobre arquivos internos do aplicativo ou dentro das pastas de gravações atuais/anteriores. Diretórios e links no destino também são rejeitados. Áudio, vídeo, SQLite e a transcrição original permanecem preservados quando a exportação falha.

## Testes automatizados

Quatro testes foram adicionados:

- `txt_snapshot_exports_safe_portuguese_filename_and_cancellation_writes_nothing`: nome em português, cancelamento sem escrita, bytes UTF-8 exatos e conteúdo preservado após mudança no banco.
- `generated_names_preserve_unicode_and_fit_windows_limits`: nomes vazios/reservados, acentos, emojis, truncamento e rejeição de nome excessivamente longo; gravação/leitura dos arquivos gerados.
- `text_export_rejects_unsafe_windows_names_without_touching_existing_files`: nomes reservados, streams alternativos, caracteres inválidos, extensão incorreta e travessia; arquivo anterior e quantidade de arquivos preservados.
- `failed_replacement_preserves_previous_export_and_cleans_temporary_file`: arquivo bloqueado pelo Windows, falha na substituição, preservação dos bytes anteriores, limpeza do temporário e sucesso após desbloqueio.

Os testes existentes também cobrem exportação tradicional/segmentada, reunião sem transcrição, texto em português, destinos protegidos e erros de armazenamento.

Validações executadas nesta fase:

```powershell
cargo fmt
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
npm run lint
npm test
npm run build
```

Cargo foi executado com `--offline --manifest-path src-tauri/Cargo.toml`, usando a instalação local em `.tooling/` descrita no README.

Resultados: formatação Rust e Prettier aprovadas; Clippy com `-D warnings` aprovado; 114 testes Rust aprovados, nenhum falhou e 5 testes manuais preexistentes foram ignorados; testes de binário/documentação sem falhas; build Rust aprovado; lint aprovado; 4 testes do frontend aprovados; build TypeScript/Vite aprovado. Os logs estão em `.tooling/phase13-tests.txt`, `.tooling/phase13-clippy.txt` e `.tooling/phase13-build.txt`.

O build Rust emitiu mensagem do linker MSVC sobre a criação da biblioteca/objeto e aviso do ambiente sobre canonicalização do diretório do usuário; terminou com código zero. Clippy não encontrou warnings de código. Os cinco testes ignorados requerem captura/reprodução manual ou MP4 fornecido explicitamente; nenhum teste de exportação foi ignorado.

## Reprodução manual

1. Abra o aplicativo com `npm run tauri dev` e selecione uma reunião já transcrita no histórico.
2. Clique em **Exportar TXT**. Confira o nome sugerido a partir do título, escolha uma pasta externa às gravações e salve.
3. Abra o arquivo no Bloco de Notas. Confira texto e rótulos/timestamps apresentados na tela. Para verificar português, use uma transcrição contendo `Olá! Ação, coração, revisão, São Paulo. Você: amanhã às 10h.`.
4. Repita usando a opção de transcrição tradicional, quando disponível, e confira que o texto corresponde à seleção.
5. Cancele uma nova exportação e confira que nenhum arquivo foi criado.
6. Exporte novamente sobre um TXT existente: aceite a confirmação e confira a substituição; repita cancelando a confirmação e confira que o conteúdo anterior permanece.
7. Experimente um nome com extensão diferente de TXT ou um destino sem permissão de escrita; confira a mensagem de erro e a preservação da transcrição original.

O diálogo visual não foi exercitado manualmente nesta fase. A escrita real no Windows, UTF-8, cancelamento no core e falha de substituição foram exercitados pelos testes automatizados. A revisão do código conferiu o diálogo e a compatibilidade da API existente.

## Arquivos alterados

- `src-tauri/src/commands/files.rs`: nome sugerido e preparação antes do diálogo.
- `src-tauri/src/meeting/details.rs`: conteúdo preparado e teste de exportação/cancelamento.
- `src-tauri/src/storage/mod.rs`: validação/geração de nomes e testes de arquivo.
- `README.md` e este documento: descrição e reprodução da Fase 13.

A Fase 14 permanece fora do escopo.
