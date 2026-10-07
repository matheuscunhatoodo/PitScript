# Validação manual — Fase 7

O `MeetingManager` prepara `merged.wav` somente depois de parar e finalizar as fontes de áudio. `microphone.wav` e `system.wav` permanecem separados. A origem temporal aproximada de cada fonte vem da hora de criação do respectivo WAV; o mixer insere silêncio antes da fonte iniciada mais tarde e no fim da fonte mais curta. Diferenças de criação superiores a 15 segundos são ignoradas e comunicadas como aviso.

## Arquivo sintético reproduzível

Na pasta `src-tauri`, execute:

```powershell
cargo test audio::mixer::tests::manual_playback_fixture -- --ignored --exact --nocapture
```

O teste imprime o caminho de `merged.wav` sob o diretório temporário. O arquivo contém um tom de 440 Hz na primeira parte, um tom de 660 Hz na última parte e uma região de sobreposição. Ele permanece no disco para reprodução manual.

No Windows PowerShell, substitua o caminho impresso:

```powershell
$wavPath = 'CAMINHO_IMPRESSO\merged.wav'
$player = [System.Media.SoundPlayer]::new($wavPath)
$player.Load()
$player.PlaySync()
$data = [IO.File]::ReadAllBytes($wavPath)
"canais=$([BitConverter]::ToUInt16($data, 22)) taxa=$([BitConverter]::ToUInt32($data, 24)) bits=$([BitConverter]::ToUInt16($data, 34)) bytes_pcm=$([BitConverter]::ToUInt32($data, 40))"
```

Resultado esperado: os dois tons aparecem, o arquivo abre no reprodutor e o cabeçalho indica 1 canal, 16000 Hz e 16 bits. Para validar uma gravação real, grave uma fala no microfone e outra reproduzida no navegador, finalize a reunião, reproduza o `merged.wav` e confira que ambas são audíveis e que o deslocamento temporal é aceitável. Compare também os arquivos originais e a duração do `merged.wav`.

## Resultado nesta execução

O teste sintético gerou um arquivo de 3,011 segundos. O `System.Media.SoundPlayer` carregou e executou `PlaySync()` sem erro. Inspeção independente em PowerShell encontrou PCM mono, 16 kHz, 16 bits, 48.176 frames, pico absoluto 15.347, tom de 440 Hz com nível 7.995 no primeiro segundo e tom de 660 Hz com nível 7.990 no último segundo. A audição humana e uma reunião real com dispositivos físicos ainda requerem verificação local.
