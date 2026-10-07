# Fase 9.1 — plano e decisões

Especificação vinculante: solicitação do usuário de 02/10/2026, AGENTS.md e arquitetura existente. O roadmap original não tem seção 9.1 e adia diarização; esta solicitação autoriza a extensão limitada. Não inclui 9.2 ou tray.

## Desenho aprovado pela solicitação

- Rust continua orquestrando processamento apenas após reunião finalizada, sob a exclusão entre captura e processamento do MeetingManager.
- Salvar primeiro o Whisper de `merged.wav` em SQLite/TXT. Esse resultado nunca depende do sucesso da diarização.
- Preparar cópias temporárias das fontes em PCM mono 16-bit/16 kHz. Transcrever cada fonte com timestamps/token JSON do whisper.cpp para evitar atribuir áudio remoto ao usuário durante sobreposição.
- Microfone → Você (não existe nome de usuário persistido). Sistema → sherpa-onnx CLI nativa v1.13.8 CPU, segmentação pyannote 3.0 int8 + embeddings genéricos WeSpeaker ResNet34 LM. Embeddings individuais nunca saem da memória do processo auxiliar.
- Alinhar tokens com turnos; rótulos numerados por primeira ocorrência nesta reunião. Baixa cobertura temporal, baixa confiança ou múltiplos locutores no mesmo token → Participantes. Confiança de clustering é uma heurística, não probabilidade calibrada.
- Migration 3 cria `transcript_segments` e `meeting_diarization`, preservando `meetings`. Substituição de segmentos em transação, FK com exclusão em cascata. Reuniões antigas sem segmentos continuam com texto original.
- Progresso/cancelamento reutilizam o worker; comandos aditivos consultam estado/segmentos e permitem processar uma reunião antiga finalizada com fontes disponíveis. Nenhum processamento inicia apenas por abrir a tela.
- Interface conserva player, busca/cópia/TXT/pasta; texto formatado inclui horário/rótulo quando há segmentos, com alternativa para texto tradicional.

## Dependências

`serde_json` direto: ler o JSON nativo do Whisper (já existe transitivamente no lockfile). sherpa-onnx/ONNX Runtime via executável e DLLs oficiais empacotadas, sem binding Rust/FFI adicional. Modelos/ligações/licenças e hashes serão documentados no relatório de teste.

## Tarefas

- [x] Repository/migration: testes de persistência, rollback, cascade e reunião antiga; implementar segmentos/estado sem alterar contratos atuais.
- [x] Alinhamento: testes 1/2/3 remotos, local+remoto, alternância, sobreposição, baixa confiança e tokens sem tempos; implementar rótulos conservadores.
- [x] Runners: JSON Whisper, parsing sherpa/progresso, limites CPU/cancelamento, modelos locais e testes com binários reais.
- [x] Pipeline/MeetingManager: worker único, salvar texto antes dos segmentos, falha/cancelamento/reinício preservam transcrição/áudio; comandos aditivos e testes.
- [x] Frontend: horários/rótulos, progresso, retry explícito, busca/cópia/TXT coerentes, reunião antiga; build/testes e QA renderizado.
- [x] Validação final: fmt/clippy/test/build, revisão independente somente leitura e relatório com limitações.

## Pontos de revisão

Texto/timestamps não podem ser perdidos por erro de diarização. Não usar caminhos arbitrários do banco. Não inferir identidade real. Não rodar durante captura nem deixar processos/temporários após cancelamento. Recuperação de interrupção deve preservar texto anterior; UI antiga e exportação devem funcionar sem segmentos.

## Registro de execução

- Inspeção: não há tabela de segmentos nem nome de usuário persistido; schema atual v2. Runtime oficial executado com amostra pública de 16 s: dois grupos, 2,086 s de inferência (não é benchmark de reuniões reais).
- Escolha: CLI auxiliar mantém isolamento de falhas nativas e permite encerramento no cancelamento; não introduzir PyTorch/FFI.
- Workspace contém baseline sem commits; executar no checkout autorizado, sem criar worktree que omitiria os arquivos existentes. Não há autorização de publicação/commit de todo o baseline.
- Repository e alinhamento: testes RED antes da implementação; schema v3, rollback/cascade, contagem/rótulos 1/2/3, alternância e sobreposição passaram.
- Runners: parsing e JSON RED→GREEN. Modelos reais detectam 1/2/3 grupos. `n/a` é ausência de confiança e pontuação no limite de trecho Whisper não invalida tempos das palavras.
- Pipeline: texto salvo primeiro; falha/cancelamento/recuperação testados. Integração real local+dois remotos passa e persistência é conferida reabrindo SQLite.
- Frontend: 4 testes, lint/build, QA em três viewports com ponte Tauri simulada; erro de consulta conserva texto anterior. Sem Browser plugin, usar Playwright existente.
- Final: revisão independente sem achado crítico; corrigido cancelamento durante conversão — `cancellation_in_both_conversion_passes_preserves_source_and_removes_partial_output` RED→GREEN, suíte 78/78.
- Final: corrigido acesso ao texto tradicional e exportação coerente — teste `structured_export_keeps_timestamps_and_speakers_without_replacing_original_text` e QA seleção/cópia/exportação RED→GREEN, suíte 78/78.
- Final: fmt/check, clippy all-targets com warnings como erro, cargo test 78 passaram/3 manuais ignorados, npm lint/test/build e formatação passaram. Cancelamento do sherpa real após progresso também passou.
- Decisões e custos: CLI CPU/modelos locais requerem Visual C++ Runtime x64 e acrescentam aproximadamente 28 MB de pesos; retranscrição por fonte acrescenta custo pós-reunião. Rótulo neutro sem confiança evita forçar identificação e pode reduzir quantidade de trechos individualizados. Sem commits/worktree por baseline não versionado. Encerramento abrupto pode deixar temporários locais; limpeza automática pós-crash fica pendente. Nenhum achado menor foi adiado pela revisão.
