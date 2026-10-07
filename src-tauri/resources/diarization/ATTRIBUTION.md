# Recursos locais de diarização

- sherpa-onnx v1.13.8, Xiaomi/k2-fsa, Apache-2.0: https://github.com/k2-fsa/sherpa-onnx/releases/tag/v1.13.8. Executável CPU oficial `win-x64-shared-MD-Release-no-tts`.
- ONNX Runtime 1.28.2, Microsoft, MIT: https://github.com/microsoft/onnxruntime. Licença e avisos de terceiros acompanham as DLLs.
- Segmentação `pyannote/segmentation-3.0`, CNRS/Hervé Bredin, MIT. Conversão ONNX int8 distribuída pelo sherpa-onnx: https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-segmentation-models. Modelo original: https://huggingface.co/pyannote/segmentation-3.0.
- Embeddings genéricos WeSpeaker ResNet34 LM, projeto WeSpeaker, CC BY 4.0: https://huggingface.co/Wespeaker/wespeaker-voxceleb-resnet34-LM. Conversão ONNX oficial distribuída em https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-recongition-models. Licença: https://creativecommons.org/licenses/by/4.0/legalcode. O modelo foi treinado em VoxCeleb2; não foi treinado ou alterado com vozes dos usuários deste aplicativo.

O aplicativo usa os pesos ONNX exportados, sem modificar os pesos. Não realiza cadastro, comparação de identidades entre reuniões ou persistência de embeddings de voz. Executa clustering apenas na memória de um processo local por reunião.

Tamanhos dos pesos: segmentação 1.540.506 bytes; embeddings 26.530.550 bytes. A versão MD do runtime requer Microsoft Visual C++ Runtime; validar distribuição desse pré-requisito ao preparar um instalador.
