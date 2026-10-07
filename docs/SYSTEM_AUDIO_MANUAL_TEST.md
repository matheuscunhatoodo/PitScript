# Teste manual do áudio do sistema — Fase 5

O backend captura o dispositivo de saída selecionado por WASAPI Loopback e grava `system.wav` em PCM mono, 16-bit, 48 kHz. O microfone permanece em `microphone.wav`. Os arquivos ficam no diretório temporário neste teste isolado e na pasta da reunião quando os comandos Tauri são usados. Nenhum conteúdo é enviado para fora do computador.

## Reproduzir

1. No Windows 10/11, reproduza continuamente um som no navegador pela saída escolhida. Para um teste offline, abra uma página local com um botão que inicia um `AudioContext` e um oscilador de 440 Hz. Clique no botão antes da captura.
2. Em PowerShell, dentro de `src-tauri`, execute:

```powershell
$env:MEETING_RECORDER_SYSTEM_TEST_SECONDS = '12'
cargo test audio::loopback::tests::manual_record_system_and_microphone -- --ignored --exact --nocapture
```

O teste lista os dispositivos de saída e entrada, escolhe os padrões, inicia o microfone seguido do loopback, grava simultaneamente, interrompe as duas threads, valida RIFF/tamanhos, mede a duração de ambos os WAVs e verifica amostras não silenciosas e intervalos de silêncio acima de 100 ms após os primeiros 500 ms. O teste imprime os caminhos dos arquivos.

Para selecionar dispositivos específicos, defina `$env:MEETING_RECORDER_OUTPUT_DEVICE_ID` e/ou `$env:MEETING_RECORDER_MIC_DEVICE_ID` com um dos IDs listados. O teste não elimina os WAVs ao terminar, para permitir inspeção e reprodução. Remova-os manualmente quando não forem mais necessários.

No aplicativo, o frontend usa `start_meeting`, `stop_meeting` e `get_recording_state`. O `MeetingManager` inicia o microfone seguido do loopback e sinaliza ambas as fontes antes de aguardar as threads na finalização. `list_output_devices` continua disponível para selecionar a saída. Os estados das fontes são emitidos em `recording-status` e `system-recording-status`. O resultado e os erros de captura são registrados em `%LOCALAPPDATA%\MeetingRecorder\logs\meeting-recorder.log`.

## Resultado nesta máquina

Em 30/09/2026, no Windows com fone de ouvido Fuxi-H3 e microfone Headset Fuxi-H3, uma página local no Microsoft Edge reproduziu um tom contínuo. A captura simultânea de 8 segundos gerou `system.wav` com 768.142 bytes PCM (8,001 s) e `microphone.wav` com 769.886 bytes PCM (8,020 s). A diferença foi de 18 ms. O arquivo do sistema tinha 382.315 quadros não silenciosos, e o maior intervalo silencioso depois dos primeiros 500 ms foi de 11,1 ms. Não houve descontinuidade registrada após a estabilização inicial do endpoint. Os cabeçalhos RIFF e os tamanhos foram validados pelo teste.

A seleção explícita do mesmo dispositivo de saída por ID também passou em uma gravação de 2 segundos: 2,004 s no sistema, 2,020 s no microfone, diferença de 16 ms e maior intervalo silencioso de 3,2 ms.

Uma execução anterior, com um WAV de 1 segundo em repetição, apresentou um aviso de descontinuidade do WASAPI e preservou um arquivo válido. O tom contínuo no Edge removeu essa pausa de reprodução. Uma primeira execução na sandbox retornou `0x80070005` (acesso negado); o teste com acesso ao dispositivo passou.

Google Meet, Microsoft Teams e YouTube ainda precisam de uma sessão manual apropriada e consentida. Também permanecem pendentes testes de troca/desconexão da saída durante a captura, reprodução audível dos WAVs, duração longa e uso com o aplicativo minimizado. Não inferir esses resultados a partir do teste local de 8 segundos.
