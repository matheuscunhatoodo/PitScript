# Fase 14 — Diagnóstico local

## Implementação

Arquivo: `%LOCALAPPDATA%\MeetingRecorder\logs\meeting-recorder.log`. A localização é definida pelo StorageManager e permanece no diretório local do aplicativo ao alterar o diretório de gravações.

O logger em `src-tauri/src/storage/diagnostics.rs` usa somente a biblioteca padrão Rust. Não adiciona dependências, serviços, comandos frontend, configurações ou migrations. As APIs existentes foram preservadas.

Cada linha contém `timestamp_ms` (Unix UTC em milissegundos), nível INFO/WARN/ERROR, evento e pares de campos técnicos. O conteúdo é UTF-8. Exemplo ilustrativo:

```text
timestamp_ms=1790899200000 INFO application_start version=0.1.0 os=windows arch=x86_64
timestamp_ms=1790899200100 ERROR wasapi_microphone_start_failed class=device_unavailable meeting=0x12345678 native_code=0x88890004
```

Campos aceitam strings estáticas restritas, números, booleanos e tokens de correlação. Status dinâmicos são convertidos para um conjunto conhecido. Dispositivos e reuniões usam hash do identificador para permitir correlação sem incluir os identificadores originais. Esse hash não é criptográfico nem pretende anonimizar dados; pode mudar entre versões da implementação Rust.

Mensagens brutas de erros e processos são convertidas em categorias/códigos. Não são registrados títulos de reunião/janela, nomes de dispositivos, caminhos escolhidos, SQL, transcrição, amostras de áudio, frames, embeddings ou payloads de panic. Os códigos numéricos ajudam a diagnosticar WASAPI/Windows sem copiar sua mensagem completa.

## Eventos cobertos

| Área | Diagnóstico |
| --- | --- |
| Aplicativo | Inicialização, versão, sistema/arquitetura, pronto, falhas de recursos/tray e panic interno sem payload |
| Dispositivos | Quantidade de entradas/saídas e padrões, seleção por token, fallback e falhas de enumeração WASAPI |
| Reunião | Início da tentativa com fontes habilitadas/quantidade ativa, fim com status/duração/fontes válidas, falhas de metadata/finalização |
| Áudio | Estado de captura e bytes gravados, falhas de início/captura WASAPI de microfone e loopback, preparação do áudio |
| Vídeo | Início solicitado com fonte opaca/FPS, conclusão com frames/skips/duração/hardware, falhas de arquivo/captura/finalização |
| Transcrição | Falha antes de iniciar, início com limite de threads, fim completed/failed/cancelled, Whisper, persistência de TXT e recuperação de arquivo |
| Diarização existente | Início/fim/falha com transcrição tradicional preservada |
| Arquivos/banco | Criação de pasta, exclusão, exportação TXT, preparação/diálogo, reprodução, abertura de pasta, inicialização/conexão SQLite |
| Encerramento | Erro ao finalizar pelo tray e manter aplicativo aberto |

Os eventos anteriores que escreviam erros brutos em meeting/video/transcription foram substituídos pelo logger central. A gravação de mídia e o fluxo de IA permanecem nos módulos existentes. Não há log por frame/pacote ou atualização contínua de progresso.

## Rotação e falhas do logger

- Limite do arquivo ativo: 1 MiB; rotação antes da escrita que ultrapassaria esse limite.
- Backups: `meeting-recorder.log.1` e `meeting-recorder.log.2`; o mais antigo é removido durante a próxima rotação. O conjunto gerado pelo logger ocupa até 3 MiB.
- Arquivo legado acima do limite é truncado na próxima escrita para limitar imediatamente o tamanho. Arquivos legados menores permanecem até serem descartados pela rotação; entradas históricas não são retroativamente sanitizadas.
- Um mutex compartilhado serializa os produtores do processo, inclusive vídeo e logs reabertos. Esta versão pressupõe um processo gravando os logs; não há coordenação entre instâncias simultâneas do aplicativo.
- Diretórios/links no lugar do arquivo ativo ou dos backups são rejeitados. Falha de rotação não provoca exclusão desses diretórios nem escrita acima do limite.
- Falha de escrita gera uma tentativa de aviso único em stderr contendo somente a categoria. Erro em stderr também é ignorado; falhas de diagnóstico não cancelam a gravação.
- Escrita usa flush, sem fsync em cada evento. Encerramento abrupto/falha do sistema pode perder os últimos registros. Se o diretório local estiver indisponível, não é possível gravar o diagnóstico em disco.

## Testes desta fase

Dez testes novos exercitam arquivos/SQLite reais com dados sintéticos:

1. Append após reabrir, timestamps e código HRESULT, sem mensagem/caminho/texto/identificador original.
2. Rotação com tamanho reduzido no teste, máximo de três arquivos e limite em cada arquivo.
3. Oito produtores concorrentes: 160 linhas completas.
4. Destino de log indisponível, operação best effort e recuperação posterior.
5. Registro maior que o limite rejeitado antes de criar arquivo.
6. Exportação com pasta inexistente e erro simulado de permissão, sem registrar conteúdo privado.
7. Log legado acima do limite truncado, arquivo de mídia preservado.
8. Diretório ocupando o nome de backup preservado durante falha de rotação.
9. Microfone desconectado e vídeo falhando em reunião com áudio de sistema válido: eventos/códigos presentes, status parcial e nenhum título/caminho/nome privado.
10. Transcrição concluída, falha simulada e cancelamento: início/fim/status presentes e nenhum texto da transcrição ou mensagem bruta de erro.

A revisão identificou que o aviso com `eprintln!` poderia gerar panic se stderr também falhasse. Um probe Windows em `.tooling/phase14-stderr-probe.rs` criou um pipe sem leitor e confirmou `eprintln_panicked=true` e `fallible_returned_error=true`. O fallback foi corrigido para `writeln!` com o resultado ignorado.

Validações:

```powershell
cargo fmt
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
npm run lint
npm test
npm run format:check
npm run build
```

Cargo usa `--offline --manifest-path src-tauri/Cargo.toml` e a instalação `.tooling/` descrita no README. Logs das validações: `.tooling/phase14-clippy.txt`, `.tooling/phase14-tests.txt` e `.tooling/phase14-build.txt`. Testes de captura manual preexistentes continuam opt-in; esta fase usa falhas simuladas e não inicia captura real.

Resultados finais após a correção do fallback: fmt/check aprovados; Clippy com `-D warnings` aprovado; 124 testes Rust aprovados (10 novos), zero falhas e 5 testes manuais preexistentes ignorados; testes de binário/documentação sem falhas; build Rust aprovado; lint e Prettier aprovados; 4 testes frontend aprovados; build TypeScript/Vite aprovado. A revisão final confirmou a correção do único achado e não encontrou outros problemas relevantes.

O ambiente Rust emitiu aviso de canonicalização do diretório do usuário e o linker MSVC informou a criação de biblioteca/objeto. Os comandos terminaram com código zero; Clippy não encontrou warnings de código.

## Reprodução manual opcional

1. Abra o aplicativo com `npm run tauri dev` e confirme `application_start`/`application_ready` no arquivo local de log.
2. Liste dispositivos e inicie/finalize uma reunião. Confira eventos de enumeração, seleção, gravação e transcrição após finalizar.
3. Cancele a transcrição pelo aplicativo; confira `transcription_finished status=cancelled`.
4. Tente exportar para um destino indisponível/sem permissão e confira `export_file_failed`, mantendo os dados da reunião.
5. Para verificar privacidade, compare o texto exportado com os logs: o texto, título e caminhos privados não devem aparecer em novos registros.

Os testes automatizados comprovam os cenários simuláveis desta fase. A interação manual com o aplicativo não foi repetida para logging.

## Arquivos

Novo: `src-tauri/src/storage/diagnostics.rs` e este documento.

Alterados: `src-tauri/src/lib.rs`, `storage/mod.rs`, `database/mod.rs`, `meeting/manager.rs`, `transcription/engine.rs`, `transcription/diarization_job.rs`, `video/mod.rs`, `commands/files.rs`, `tray.rs`, `README.md` e `docs/ARCHITECTURE.md`.

A Fase 15 permanece fora do escopo.
