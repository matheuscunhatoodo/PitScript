use super::whisper::TranscriptionFailure;
use std::{
    io::{BufRead, BufReader, Read},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

pub fn run(
    mut command: Command,
    cancel: &AtomicBool,
    parse: fn(&str) -> Option<u8>,
    progress: &mut dyn FnMut(u8),
) -> Result<String, TranscriptionFailure> {
    if cancel.load(Ordering::Acquire) {
        return Err(TranscriptionFailure::Cancelled);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|_| {
        TranscriptionFailure::Failed(
            "Não foi possível iniciar o processador local de áudio.".into(),
        )
    })?;
    let mut stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let output = thread::spawn(move || {
        let mut text = String::new();
        (&mut stdout)
            .take(4 * 1024 * 1024 + 1)
            .read_to_string(&mut text)?;
        // Keep draining after the cap so a full pipe cannot deadlock the child.
        std::io::copy(&mut stdout, &mut std::io::sink())?;
        Ok::<_, std::io::Error>(text)
    });
    let (sender, receiver) = mpsc::channel();
    let diagnostics = thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if let Some(value) = parse(&line) {
                if sender.send(value).is_err() {
                    break;
                }
            }
        }
    });
    let result = loop {
        for value in receiver.try_iter() {
            progress(value);
        }
        if cancel.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            break Err(TranscriptionFailure::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break Ok(()),
            Ok(Some(_)) => break Err(TranscriptionFailure::Failed(
                "O processador local de áudio falhou; a transcrição tradicional foi preservada."
                    .into(),
            )),
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(TranscriptionFailure::Failed(
                    "Falha ao aguardar o processador local.".into(),
                ));
            }
        }
    };
    let _ = diagnostics.join();
    let text = output
        .join()
        .map_err(|_| TranscriptionFailure::Failed("Falha na leitura do resultado local.".into()))?
        .map_err(|_| TranscriptionFailure::Failed("Resultado local inválido.".into()))?;
    for value in receiver.try_iter() {
        progress(value);
    }
    result?;
    if text.len() > 4 * 1024 * 1024 {
        return Err(TranscriptionFailure::Failed(
            "Resultado de áudio excede o limite seguro.".into(),
        ));
    }
    Ok(text)
}
