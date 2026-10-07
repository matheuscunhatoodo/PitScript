# Fase 9.1 — implementação e validação

Escopo: identificação de grupos de locutores por reunião, autorizada em 02/10/2026. Sem nomes reais, cadastro/biometria, embeddings persistentes, Python/PyTorch, API externa, resumo ou Fase 9.2. O roadmap anterior colocava diarização no futuro; a extensão autorizada foi registrada no plano e na seção 31 da arquitetura.

## Pipeline implementado

1. MeetingManager finaliza captura e fecha os WAVs. A exclusão global impede captura e processamento simultâneos.
2. Preparação existente gera `merged.wav`. Whisper salva primeiro a transcrição tradicional no SQLite e em `transcript.txt`.
3. Um único worker Rust prepara cópias temporárias independentes das fontes válidas em PCM mono 16-bit/16 kHz. A conversão verifica cancelamento periodicamente nas duas passagens.
4. Whisper local transcreve cada fonte em JSON com timestamps das palavras/tokens. Não atribuir o texto de `merged.wav` diretamente aos turnos evita confundir fontes simultâneas.
5. Somente a cópia de `system.wav` passa por sherpa-onnx; quantidade de grupos automática. Não há configuração de nome do usuário existente; microfone sempre `Você`.
6. Alinhamento aplica o deslocamento temporal das fontes, coerente com a preparação existente. Cobertura de palavra de pelo menos 60%, confiança heurística de pelo menos 0,6 e ausência de outro grupo cobrindo pelo menos 20% permitem `Participante N`. Demais trechos usam `Participantes`.
7. Migration v3 salva segmentos e conclusão em transação; estado de diarização separado do estado da transcrição. Falha/cancelamento/interrupção preserva o texto tradicional e o áudio.
8. A tela mostra horário, rótulo e texto, pesquisa, cópia, TXT, progresso e cancelamento. `Ver texto tradicional` alterna a versão exibida; cópia e exportação seguem a seleção. Abrir reuniões antigas não dispara inferência; `Identificar participantes` é uma ação explícita.

`transcript.txt` permanece como texto tradicional de segurança. O TXT exportado por padrão é formatado com horários/rótulos quando existem segmentos. O parâmetro Tauri opcional `traditional` seleciona o texto anterior; chamadas existentes sem esse argumento continuam válidas.

Temporários ficam em `recordings/{id}/postprocess-{pid}-{nonce}/` e são removidos em conclusão, erro e cancelamento normal. Nunca são salvos embeddings. Em encerramento abrupto podem sobrar cópias WAV/JSON nesse diretório; o reinício recupera o estado para permitir tentativa explícita, sem apagar originais.

## Dependências e modelos locais

Dependência Rust direta adicionada: `serde_json`, para o JSON nativo do Whisper; já existia transitivamente no lockfile. Sem novas dependências npm ou binding FFI. A CLI oficial isola falhas nativas e permite cancelamento por término/espera do subprocesso.

| Recurso | Versão / origem | Tamanho |
| --- | --- | ---: |
| Whisper existente, Base Multilingual Q5 | whisper.cpp b5130, `ggml-base-q5_1.bin`, idioma padrão `pt` | 59.707.625 bytes |
| Segmentação | pyannote segmentation 3.0, ONNX int8 | 1.540.506 bytes |
| Embeddings genéricos | WeSpeaker VoxCeleb ResNet34 LM, ONNX | 26.530.550 bytes |
| Runtime de diarização | sherpa-onnx v1.13.8 x64 CPU MD + ONNX Runtime 1.28.2 | 17.489.920 bytes, executável e duas DLLs |

Os dois novos modelos somam **28.071.056 bytes** (26,77 MiB). Recursos inclusos no bundle em `resources/diarization/`, sem download em runtime. No máximo quatro threads, ou metade das CPUs disponíveis, usando um processador e CPU; sem inferência durante gravação.

SHA-256:

- Segmentação: `D582F4B4C6B48205DE7E0643C57DF0DF5615A3C176189BE3FC461E9D18827B5D`.
- Embeddings: `E9848563DA86F263117134DFD7AD63C92355B37DE492B55E325400C9D9C39012`.

Fontes: [release sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx/releases/tag/v1.13.8), [modelos de segmentação](https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-segmentation-models), [exportação de embeddings](https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-recongition-models), [modelo original WeSpeaker e licença](https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM). Licenças/atribuição acompanham os recursos: sherpa Apache-2.0, pyannote e ONNX Runtime MIT, WeSpeaker CC BY 4.0. Versão MD requer **Microsoft Visual C++ Runtime x64**. Funcionou nesta máquina; instalador/máquina limpa ainda dependem da fase de distribuição.

## Testes e resultados

| Caso solicitado | Verificação executada | Resultado / alcance |
| --- | --- | --- |
| Uma pessoa | sherpa real com `jfk.wav`, 11 s; parser e alinhamento | Um grupo detectado. CLI retorna confiança `n/a`; saída remota conservadora é `Participantes`. Microfone continua `Você`. |
| Duas pessoas | sherpa real com amostra pública, 16 s; pipeline Whisper+sherpa | Dois grupos sem contagem forçada; segmentos `Participante 1` e `Participante 2`. |
| Três pessoas | sherpa real, composição pública de 27,5 s | Três grupos sem contagem forçada. É concatenação, não reunião natural de três pessoas. |
| Local + remoto | Pipeline real: JFK no microfone e dois locutores no sistema, fontes adaptadas para 48 kHz | Texto por fonte, `Você` e dois participantes remotos; WAVs/TXT intactos; segmentos após reabrir SQLite. |
| Falas alternadas | Teste determinístico de alinhamento local/remoto/local/remoto | Ordem, rótulos repetidos e deslocamento temporal preservados. |
| Pequena sobreposição | Sobreposição entre dois turnos por 100 ms em palavra de 500 ms; local/remoto simultâneos | Remoto ambíguo `Participantes`; local/remoto mantidos como fontes independentes. Verificação de alinhamento, sem benchmark acústico de vozes sobrepostas. |
| Diarização falhando | Processor simulado falha após salvar TXT; rollback/estado separados | SQLite/TXT tradicional e WAV válido preservados; erro e tentativa explícita. |
| Reunião antiga sem segmentos | Repository/migrations, processamento explícito, QA da tela | Texto tradicional acessível, player/exportação existentes, sem processamento automático ao abrir. |

Adicionais: validação de timestamps/confiança, Unicode e pontuação no limite do trecho Whisper, ordenação, transação/rollback/FK cascade, recuperação de interrupção, cancelamento do worker, cancelamento nas duas passagens de conversão com remoção de parcial, exportação das duas versões sem substituir texto original, limite de threads e recusa de reunião não finalizada. Teste com sherpa real solicita cancelamento após progresso e verifica retorno cancelado depois de encerrar/aguardar o processo.

Validações finais em 02/10/2026:

- `cargo fmt` e `cargo fmt --check`.
- `cargo clippy --all-targets -- -D warnings`.
- `cargo test`: **78 passaram, zero falhas, 3 ignorados** (testes manuais de áudio das fases anteriores). Inclui modelos reais, pipeline por fonte, cancelamento nativo e regressões da revisão.
- `npm run lint`, `npm test` (4 testes), `npm run build`.
- QA Playwright com ponte Tauri simulada: localhost `127.0.0.1:1420`, título Meeting Recorder, tamanhos 1280×800, 800×600 e 390×844; sem tela vazia, overlay, erros de console ou overflow horizontal. Abrir antiga → iniciar → progresso → cancelar → tentar novamente → segmentos; pesquisa literal, clipboard UTF-8, exportação, alternância tradicional/participantes e fallback de erro de consulta. Browser plugin ausente; usado Playwright já instalado, sem dependência adicionada. Diálogos nativos e janela WebView2 não foram automatizados nessa verificação.

A revisão independente encontrou cancelamento ausente na conversão e falta de seleção do texto tradicional. Ambos foram reproduzidos com testes falhando antes das correções, e validados novamente. Não houve achado crítico.

## Reproduzir manualmente em português

1. Com Rust/Node/WebView2 e Visual C++ Runtime instalados, abra com `npm run tauri dev`. Se necessário, configure a instalação Rust `.tooling` conforme README. Desconecte a internet depois de preparar o ambiente; nenhuma inferência precisa de rede.
2. Use fones para evitar que o microfone capte o áudio remoto. Habilite microfone e áudio do computador. Grave uma reunião curta em português com duas pessoas remotas, trechos alternados e breve sobreposição. Anote aproximadamente os horários de troca de voz. Não há integração especial com Meet/Teams.
3. Finalize e observe pós-processamento: transcrição → fontes → participantes → alinhamento. A UI deve continuar respondendo. Tentativa de iniciar nova gravação deve ser recusada enquanto o worker processa.
4. Abra a reunião no histórico. Compare áudio e segmentos: microfone `Você`; remotos numerados por reunião; ambiguidades `Participantes`. Confira busca, copiar, exportar TXT e `Ver texto tradicional`. Não esperar nomes reais ou rótulos estáveis entre reuniões.
5. Feche e reabra o aplicativo: segmentos e timestamps devem persistir. Confira originais e `transcript.txt` na pasta local. SQLite v3 deve conter `transcript_segments` e `meeting_diarization`; nenhuma coluna de embeddings ou BLOB de áudio.
6. Em outra sessão, cancele durante preparação e depois durante identificação: preservar WAVs/texto já salvo, estado `cancelled`, temporários removidos normalmente, tentativa explícita disponível. Para simular falha de modelo, feche o app e renomeie temporariamente um ONNX no ambiente de desenvolvimento, gere uma reunião, confirme fallback e restaure o arquivo. Não testar com a única cópia de gravações importantes.
7. Abra reunião anterior sem segmentos: consultar/copiar/exportar texto deve funcionar. Identificação depende das fontes originais válidas; se não existem, manter texto e apresentar erro.
8. Repita com 5 e 30 minutos, ruído e três vozes remotas naturais. Anote CPU/RAM, tempo, correção de rótulos e desvios dos timestamps. Esses testes de uso real em português/longa duração permanecem pendentes; as amostras automáticas curtas são em inglês.

## Limitações e aceite

- Timestamps Whisper e clustering são estimativas. Não há garantia de atribuição correta em cada palavra; confiança é heurística. Uma única voz remota pode usar `Participantes` por confiança indisponível.
- WeSpeaker incluído foi treinado em VoxCeleb2, com modelo identificado como inglês; qualidade com português, sotaques, ruído, fala curta e canais comprimidos precisa de avaliação de uso. Quantidade de grupos pode variar; a ferramenta não descobre identidade real.
- Áudio captado pelo microfone segue a regra `Você`, inclusive outras pessoas fisicamente próximas/eco. Não há separação de fontes, cancelamento de eco ou reconhecimento biométrico.
- Transcrever `merged.wav` e depois cada fonte aumenta tempo/CPU **após** a reunião. A preparação faz streaming, mas Whisper/sherpa carregam áudio para inferência; consumo em reuniões longas não foi medido. JSON de timestamps limitado a 64 MiB e resultado sherpa a 4 MiB; limite/erro conserva o texto tradicional.
- Sincronização usa criação dos WAVs como na Fase 7; arquivos copiados/alterados podem perder a referência temporal. Diferença superior a 15 s faz alinhar primeiros samples. Não há nova compensação de drift.
- Encerramento abrupto pode deixar temporários locais; limpeza pós-crash não foi adicionada. Nenhum embedding é escrito nesses arquivos.
- Critérios funcionais implementados: múltiplos grupos, origem local, timestamps/segmentos persistidos, pós-reunião, UI responsiva/worker, progresso/cancelamento, fallback, reuniões antigas, exportação e privacidade local. Qualidade acústica real em português, conversas naturais longas/ruidosas e instalador em máquina limpa ainda não estão validados.

## Arquivos criados e alterados

- `src-tauri/src/database/{mod.rs,migrations.rs,segments.rs}`: FK, migration v3 e repository de segmentos/estado.
- `src-tauri/src/transcription/{mod.rs,engine.rs,whisper.rs,alignment.rs,json.rs,sherpa.rs,process.rs,pipeline.rs,diarization_job.rs}`: runners nativos, alinhamento, pipeline/worker e testes.
- `src-tauri/src/audio/mixer.rs`, `storage/mod.rs`: offsets/cancelamento da conversão e diretório temporário exclusivo.
- `src-tauri/src/meeting/{manager.rs,details.rs}`, `commands/{meeting.rs,files.rs}`, `lib.rs`: orquestração, comandos aditivos e exportação selecionável.
- `src-tauri/{Cargo.toml,Cargo.lock,tauri.conf.json}`: serde_json direto e recursos nativos no bundle.
- `src-tauri/resources/diarization/`: executável, duas DLLs, dois ONNX, atribuição/licenças/avisos.
- `src-tauri/tests/fixtures/{two-speakers.wav,three-speakers.wav,README.md}`: amostras públicas e procedência; JFK existente preservado.
- `src/pages/{MeetingPage.tsx,NewRecordingPage.tsx}`, `src/services/meeting.ts`, `src/types/meeting.ts`, `src/utils/meeting.ts`, `src/styles/app.css`, `tests/meeting-presentation.test.mjs`: interface, contratos e teste de texto formatado.
- `README.md`, `docs/{ARCHITECTURE.md,IMPLEMENTATION_PLAN.md,DIARIZATION_PLAN.md,DIARIZATION_TESTING.md}`: escopo autorizado, decisões, testes e pendências.

Scripts/evidências temporárias de QA ficam fora do código do produto; downloads/logs locais em `.tooling` são ignorados. Nenhum commit/publicação foi feito: o checkout recebido contém todo o baseline ainda sem commits.
