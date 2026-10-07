# Fase 9 — Player e transcrição

## Implementação

O histórico usa `list_meetings` e o detalhe usa `get_meeting`, preservando os contratos existentes. A tela exibe título, data/hora no fuso local e duração `mm:ss` ou `h:mm:ss`.

O serviço frontend obtém uma URL de reprodução por `get_meeting_audio` e `convertFileSrc`; os componentes não montam caminhos nem acessam arquivos. O backend autoriza somente um WAV local validado por vez, com escopo inicial vazio no protocolo asset. A prioridade é `merged.wav`, depois `microphone.wav` e `system.wav`. A reprodução não altera, mistura ou transcreve os arquivos.

A transcrição vem do SQLite. A busca literal ignora diferenças de maiúsculas/minúsculas, destaca ocorrências e permite navegar entre elas. Copiar envia o texto completo à área de transferência. Não são criados nomes de participantes ou timestamps que não existam no texto.

`export_transcript` abre o diálogo Salvar do Windows, respeita cancelamento e confirmação de sobrescrita, e exporta UTF-8 por um arquivo temporário seguido de rename. O destino deve ser um TXT fora da árvore de dados do aplicativo. O SQLite e o `transcript.txt` original são preservados. `open_meeting_folder` valida a reunião/pasta e usa o Explorador de Arquivos. Ambas as operações executam no backend, fora da thread principal.

Foi habilitado o protocolo asset nativo do Tauri, que acrescenta a dependência indireta `http-range`. Não foram adicionados plugins de filesystem/dialog, serviços externos, identificação de participantes ou recursos da Fase 10.

## Arquivos desta fase

- Backend: `src-tauri/src/meeting/details.rs`, `meeting/mod.rs`, `commands/files.rs`, `commands/mod.rs`, `storage/mod.rs`, `audio/mixer.rs` (reutilização do validador existente e soma sem overflow no cabeçalho WAV), `lib.rs`.
- Configuração: `src-tauri/Cargo.toml`, `Cargo.lock`, `tauri.conf.json`, `package.json`.
- Frontend: `src/app/App.tsx`, `src/pages/HomePage.tsx`, `MeetingPage.tsx`, `NewRecordingPage.tsx`, `src/services/meeting.ts`, `src/types/meeting.ts`, `src/components/StatusBadge.tsx`, `src/styles/app.css`, `src/utils/meeting.ts`.
- Testes/documentação: `tests/meeting-presentation.test.mjs`, este documento e `README.md`.

## Como testar no Windows

Na raiz do projeto, com as dependências já instaladas:

```powershell
# Somente se Cargo não estiver no PATH e esta instalação local existir:
$projectRoot = (Get-Location).Path
$env:CARGO_HOME = Join-Path $projectRoot '.tooling\cargo'
$env:RUSTUP_HOME = Join-Path $projectRoot '.tooling\rustup'
$env:PATH = "$(Join-Path $env:CARGO_HOME 'bin');$env:PATH"

npm run tauri dev
```

1. Abra **Início**, escolha uma reunião finalizada ou grave 15–30 segundos e use **Abrir reunião salva** após finalizar. Espere a transcrição terminar; modelo/whisper.cpp devem estar configurados como descrito em `WHISPER_TESTING.md`.
2. Confira título, data/hora, duração e fontes habilitadas.
3. Reproduza, pause e avance o áudio. Compare com o WAV na pasta. Não deve iniciar uma nova captura ou transcrição ao abrir o detalhe.
4. Busque uma palavra repetida, navegue com **Anterior/Próximo**, experimente maiúsculas e um termo inexistente. Limpe a busca: o texto completo deve reaparecer.
5. Use **Copiar texto** e cole no Bloco de Notas; confirme acentos e que todo o texto foi copiado, sem os destaques da busca.
6. Use **Exportar TXT**, escolha Documentos ou outra pasta fora de `%LOCALAPPDATA%\MeetingRecorder`, abra o TXT e compare seu conteúdo. Repita escolhendo um TXT existente e confirme a substituição; teste também **Cancelar**. O `transcript.txt` original deve permanecer intacto.
7. Use **Abrir pasta da reunião**: o Explorador deve abrir `recordings/{meeting_id}` dessa reunião.
8. Feche e reabra o aplicativo e confira que a mesma reunião continua no histórico e pode ser reproduzida.
9. Em uma reunião descartável, renomeie temporariamente `merged.wav`: uma fonte válida deve continuar disponível. Restaure o nome após testar. Sem WAVs válidos, o detalhe deve informar a ausência e continuar exibindo texto/metadados.

`npm run dev` sozinho abre apenas a interface no navegador; o histórico informa que é necessário abrir o aplicativo desktop.

## Validações automatizadas

```powershell
npm test
npm run lint
npm run build
cd src-tauri
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

Os testes JavaScript usam o runner nativo do Node com suporte a TypeScript (Node 22.6+; validado com Node 25.8.1), sem nova dependência de testes.

- Rust: seleção/fallback de WAV, recusa de reunião em gravação/inexistente, caminhos externos no banco, fonte inválida junto de fonte válida, cabeçalho WAV com tamanho excessivo sem panic, pasta ausente, normalização de prefixos Windows/UNC com Unicode para o Shell, exportação UTF-8, sobrescrita e preservação dos dados gerenciados.
- JavaScript: busca literal com símbolos, Unicode, caixa, múltiplas ocorrências e ausência de resultados; duração acima de uma hora; datas válidas/inválidas.
- QA renderizado: Playwright/Chromium em `http://127.0.0.1:1420`, com registros sintéticos e bridge Tauri simulada. Viewports 1280×800, 800×600 (janela padrão) e 390×844. Validou título/data/duração, player WAV real com play/pause/seek, busca/destaques/navegação, clipboard com acentos, chamadas de exportação/cancelamento/abertura de pasta, atualização após processamento e estados de ausência/erro. Sem erros de console ou overlay Vite; screenshots revisadas. O plugin Browser não estava disponível.

## Limites da validação

Resultado final em 01/10/2026: `cargo fmt` executado; `cargo clippy --all-targets -- -D warnings` aprovado; `cargo test` com **63 aprovados, 0 falhas, 3 ignorados**; `npm test` com **3 aprovados**; `npm run lint`, `npm run build` e Prettier nos arquivos alterados aprovados. O Cargo emitiu avisos do ambiente sobre canonicalização da pasta do usuário e uma mensagem informativa do linker, sem impedir as validações.

| Critério da Fase 9 | Implementação e evidência |
| --- | --- |
| Abrir reunião já gravada | Histórico SQLite e botão pós-gravação; QA de navegação aprovado |
| Título | Registro real, validado no QA |
| Data | Timestamp persistido no fuso local, testado e validado no QA |
| Duração | Metadado persistido, testes para minutos/horas e QA |
| Reproduzir áudio | Player WAV, play/pause/seek aprovado no navegador; protocolo asset no WebView2 requer roteiro manual |
| Visualizar transcrição | Texto SQLite, QA inclusive atualização após processamento |
| Pesquisar na transcrição | Busca literal, destaques, navegação e ausência de resultado, testes e QA |
| Copiar texto | Texto completo e acentos validados na área de transferência |
| Exportar TXT | UTF-8/sobrescrita/proteção testados no Rust; integração frontend/cancelamento com bridge simulada; diálogo nativo requer roteiro manual |
| Abrir pasta local | Validação e normalização do caminho testadas; chamada frontend validada; abertura efetiva do Explorador requer roteiro manual |
| Preservar APIs e privacidade | APIs existentes mantidas; comandos aditivos, sem serviços externos ou acesso direto a arquivos pelos componentes |
| Limite de escopo | Sem identificação avançada de participantes e sem Fase 10 |

O QA de navegador simula a ponte Tauri e os retornos dos comandos nativos. Não confirma a interação humana com o diálogo Salvar, o Explorador ou a reprodução pelo protocolo asset dentro do WebView2. O roteiro acima cobre essas verificações no desktop e a reabertura real da aplicação. Os três testes Rust ignorados continuam sendo testes manuais de captura/reprodução das fases anteriores.

Não foram gerados um novo instalador ou mudanças na Fase 10; instaladores de fases anteriores não contêm esta tela atualizada. Para este teste use `npm run tauri dev`.
