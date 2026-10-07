# Teste manual do microfone — Fase 4

O teste grava somente o microfone selecionado. O arquivo permanece no diretório temporário do usuário para inspeção; nenhum áudio é enviado para fora do computador. Faça o teste em um local onde a gravação seja permitida.

## Pré-requisitos

- Windows 10/11 com um dispositivo de entrada ativo.
- Acesso ao microfone permitido para aplicativos de desktop nas configurações de privacidade do Windows.
- Rust/Cargo configurado conforme o README.

## Gravação mínima

Em PowerShell, na pasta `src-tauri`:

```powershell
cargo test audio::microphone::tests::manual_record_five_seconds -- --ignored --exact --nocapture
```

O teste lista os dispositivos, seleciona o padrão, grava por cinco segundos, interrompe a thread e valida o cabeçalho RIFF, o tamanho PCM e a presença de amostras. O caminho do WAV aparece na saída. O formato esperado é PCM mono, 16-bit, 48 kHz.

Para selecionar outro dispositivo, copie o `id` exibido pelo teste e execute:

```powershell
$env:MEETING_RECORDER_MIC_DEVICE_ID = '<id exibido>'
cargo test audio::microphone::tests::manual_record_five_seconds -- --ignored --exact --nocapture
Remove-Item Env:MEETING_RECORDER_MIC_DEVICE_ID
```

Para alterar a duração, defina `MEETING_RECORDER_MIC_TEST_SECONDS` entre 1 e 1800. O valor padrão é 5. Para testar 30 minutos, use 1800.

## Conferir e reproduzir

Substitua o caminho abaixo pelo WAV indicado na saída do teste:

```powershell
$wav = '<caminho WAV exibido>'
$player = [System.Media.SoundPlayer]::new($wav)
$player.Load()
$player.PlaySync()
$player.Dispose()
```

Confira se o áudio contém o som esperado e se o tempo de reprodução corresponde à duração escolhida.

## Falha ou desconexão

Com um microfone USB, inicie uma gravação de 30 segundos e desconecte-o após alguns segundos. A captura deve terminar com erro, e o WAV já escrito deve permanecer no diretório informado. O `MeetingManager` grava o estado `failed_partial` e o caminho no SQLite quando há áudio válido; use `get_recording_state` ou o evento `recording-status` para observar a falha no aplicativo. Nunca desconecte um dispositivo de que outra tarefa depende.

## Resultado nesta máquina

Em 30/09/2026, a gravação de cinco segundos com o dispositivo padrão produziu 480.926 bytes PCM (5,01 s), em um WAV PCM mono, 16-bit, 48 kHz. `System.Media.SoundPlayer.Load()` aceitou o arquivo. A seleção explícita de outro microfone por ID também passou em uma gravação de um segundo (96.960 bytes PCM). A primeira execução na sandbox retornou `0x80070005` (acesso negado); as execuções com acesso ao dispositivo passaram. A reprodução audível, a desconexão física e uma sessão de 30 minutos ainda exigem execução manual.
