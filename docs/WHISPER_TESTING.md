# Fase 8 — validação da transcrição local

## Recursos incluídos

O aplicativo inclui `whisper-cli.exe` e suas DLLs de CPU da versão `b5130` de `whisper.cpp`, além do modelo multilíngue `ggml-base-q5_1.bin`. O modelo padrão usa idioma `pt`. A captura termina e `merged.wav` é fechado antes de o processo Whisper iniciar. O backend limita o motor a uma transcrição por vez, um processador e no máximo quatro threads (ou metade das CPUs disponíveis, o que for menor). A interface consulta o progresso por comando Tauri a cada segundo; o cancelamento termina o processo. Não há chamada a serviço de transcrição externo.

Origens: [release oficial b5130](https://github.com/ggml-org/whisper.cpp/releases/tag/b5130), [modelo oficial](https://huggingface.co/ggerganov/whisper.cpp/blob/main/ggml-base-q5_1.bin), [opções da CLI](https://github.com/ggml-org/whisper.cpp/blob/master/examples/cli/README.md).

O arquivo de modelo incluído tem SHA-256 `422F1AE452ADE6F30A004D7E5C6A43195E4433BC370BF23FAC9CC591F01A8898` (59.707.625 bytes). O ZIP da release usado para copiar os binários tinha SHA-256 `F9EC6C52A2E949B62AB51FA21D0D497958F9E41C3010C157C4E42932D5316F3C`.

## Testes automatizados curtos

Na pasta `src-tauri`, com Rust configurado, execute `cargo test transcription::`. O teste com execução real usa `tests/fixtures/jfk.wav` (11 segundos, fala em inglês) e verifica que o texto aparece tanto no SQLite quanto em `transcript.txt`. Outros testes verificam idioma padrão `pt`, limite de threads, cancelamento, falha, nova tentativa e recuperação de um processamento interrompido. O fixture em inglês serve para validar a integração real; a qualidade da transcrição em português requer o teste manual abaixo.

## Teste manual em português e gravações maiores

1. Desconecte a internet. Inicie o aplicativo, escolha microfone e/ou áudio do computador, selecione o limite de threads e grave uma reunião curta em português. Termine a gravação. Confirme que a transcrição começa **após** a gravação e que a interface continua respondendo.
2. Repita com cerca de **5 minutos** e depois **30 minutos**. Em cada sessão, inclua um trecho com ruído moderado e outro com duas ou mais vozes. Observe progresso, tempo de processamento e uso de CPU no Gerenciador de Tarefas. A precisão de múltiplas vozes deve ser avaliada no texto; esta fase não identifica falantes.
3. No diretório `%LOCALAPPDATA%\MeetingRecorder\recordings\{meeting_id}\`, compare `microphone.wav`, `system.wav`, `merged.wav` e `transcript.txt`. Confirme que o WAV continua íntegro e que o texto corresponde à gravação. Confira também as colunas `transcription`, `transcription_status`, `transcription_model` e `status` da reunião em `%LOCALAPPDATA%\MeetingRecorder\database\meeting-recorder.db`.
4. Repita uma gravação longa e cancele a transcrição durante o processamento. Confirme `cancelled`, ausência de `transcript.txt` incompleto e preservação dos WAVs. Use **Tentar transcrever novamente** e confira o estado `completed`.
5. Encerre o aplicativo durante uma transcrição e abra-o de novo. O estado `processing` deve ser recuperado como `failed` se não houver TXT completo; a gravação permanece disponível para nova tentativa. Se o TXT já tiver sido salvo antes da interrupção, o texto deve ser recuperado no SQLite como `completed`.

Resultados nesta fase: o teste automatizado de 11 segundos passou com o processo real e o modelo incluído. Os cenários manuais em português, com ruído, várias vozes e duração de 5/30 minutos dependem de gravações reais e ainda precisam ser executados no computador de uso.
