# Amostras públicas de teste

- `jfk.wav`: amostra de 11 s já usada na Fase 8, originada de [whisper.cpp](https://github.com/ggml-org/whisper.cpp/blob/b5130/samples/jfk.wav). Um locutor, inglês.
- `two-speakers.wav`: 16 s, dois locutores em inglês, arquivo público oficial [1-two-speakers-en.wav](https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/1-two-speakers-en.wav). SHA-256 `F1C877DC01595E28BE7147BF2FE38E5268147A868BF3FDB5C37B97F5940E21F3`.
- `three-speakers.wav`: composição de `two-speakers.wav`, 0,5 s de silêncio e o PCM de `jfk.wav`; 27,5 s, mono 16 kHz PCM 16-bit. Cabeçalho RIFF recalculado, chunks de metadados ignorados. SHA-256 `57388CD15EE759F9D9746F8837DC80C5B3A00D77F1DE79E019D5E5FB764E6232`. Verifica três grupos sem forçar a contagem; não representa conversa natural de três pessoas.

Nenhuma amostra contém dados privados de usuários. Os testes reais são de integração com pesos/binários locais; não são métricas de qualidade DER ou de desempenho de reuniões longas em português.

## Fixture de reprodução de vídeo

`playback.mp4` é um vídeo sintético sem áudio: pixels em tons de cinza gerados localmente com o encoder H.264 do Media Foundation. Não contém captura de tela, pessoas nem dados de reuniões. Perfil: 1280×720, 15 FPS, 90 frames, duração decodificada de 5999 ms, 312789 bytes. SHA-256: `33862c8cb36f934a41bd9576dfa30968f285826a1a50d85364c8905573b34570`.

Usado nos testes de autorização e integridade da reprodução e na validação do player. Pode ser decodificado sem capturar dispositivos usando o teste opt-in existente `decode_existing_app_mp4` e a variável `MEETING_RECORDER_VIDEO_DECODE_PATH` apontando para este arquivo.
