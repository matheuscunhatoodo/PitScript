use crate::meeting::manager::{ApplicationActivity, MeetingManager};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, RunEvent, WindowEvent,
};

const TRAY_ID: &str = "meeting-recorder";

struct TrayState {
    icon: TrayIcon,
    status: MenuItem<tauri::Wry>,
    stop: MenuItem<tauri::Wry>,
    exit: MenuItem<tauri::Wry>,
    busy: AtomicBool,
    exit_allowed: AtomicBool,
    monitor_stop: AtomicBool,
    monitor: Mutex<Option<JoinHandle<()>>>,
}

fn status_text(activity: &ApplicationActivity) -> String {
    if activity.recording {
        format!(
            "● Gravando{} {:02}:{:02}:{:02}",
            if activity.partial_failure {
                " com falha"
            } else {
                ""
            },
            activity.elapsed_seconds / 3600,
            (activity.elapsed_seconds / 60) % 60,
            activity.elapsed_seconds % 60
        )
    } else if activity.processing {
        "Processando transcrição".into()
    } else {
        "Pronto para gravar".into()
    }
}

fn status_icon(recording: bool, processing: bool) -> Image<'static> {
    let color = if recording {
        [234, 55, 65, 255]
    } else if processing {
        [235, 162, 38, 255]
    } else {
        [40, 139, 150, 255]
    };
    let mut rgba = vec![0; 32 * 32 * 4];
    for y in 0_i32..32 {
        for x in 0_i32..32 {
            if (x - 16).pow(2) + (y - 16).pow(2) <= 13 * 13 {
                let index = ((y * 32 + x) * 4) as usize;
                rgba[index..index + 4].copy_from_slice(&color);
            }
        }
    }
    Image::new_owned(rgba, 32, 32)
}

pub(crate) fn install(app: &AppHandle, manager: Arc<MeetingManager>) -> tauri::Result<()> {
    let status = MenuItem::with_id(
        app,
        "tray-status",
        "Pronto para gravar",
        false,
        None::<&str>,
    )?;
    let open = MenuItem::with_id(app, "tray-open", "Abrir aplicativo", true, None::<&str>)?;
    let view = MenuItem::with_id(app, "tray-view", "Visualizar estado", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "tray-stop", "Finalizar gravação", false, None::<&str>)?;
    let exit = MenuItem::with_id(app, "tray-exit", "Sair", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&status, &separator, &open, &view, &stop, &exit])?;
    let icon = TrayIconBuilder::with_id(TRAY_ID)
        .icon(status_icon(false, false))
        .tooltip("Meeting Recorder — pronto para gravar")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray-open" => restore(app, false),
            "tray-view" => restore(app, true),
            "tray-stop" => stop_recording(app),
            "tray-exit" => request_exit(app),
            _ => {}
        })
        .on_tray_icon_event(|icon, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                restore(icon.app_handle(), false)
            }
        })
        .build(app)?;
    let state = Arc::new(TrayState {
        icon,
        status,
        stop,
        exit,
        busy: AtomicBool::new(false),
        exit_allowed: AtomicBool::new(false),
        monitor_stop: AtomicBool::new(false),
        monitor: Mutex::new(None),
    });
    app.manage(state.clone());
    let weak = Arc::downgrade(&state);
    let handle = app.clone();
    let worker = thread::Builder::new()
        .name("tray-status".into())
        .spawn(move || {
            while let Some(state) = weak.upgrade() {
                if state.monitor_stop.load(Ordering::Acquire) {
                    break;
                }
                if !state.busy.load(Ordering::Acquire) {
                    if let Ok(activity) = manager.activity() {
                        let update = state.clone();
                        let _ = handle.run_on_main_thread(move || {
                            if !update.busy.load(Ordering::Acquire)
                                && !update.monitor_stop.load(Ordering::Acquire)
                            {
                                let _ = update.status.set_text(status_text(&activity));
                                let _ = update.icon.set_tooltip(Some(format!(
                                    "Meeting Recorder — {}",
                                    status_text(&activity)
                                )));
                                let _ = update.icon.set_icon(Some(status_icon(
                                    activity.recording,
                                    activity.processing,
                                )));
                                let _ = update.stop.set_enabled(activity.recording);
                                let _ = update.exit.set_enabled(true);
                            }
                        });
                    }
                }
                drop(state);
                thread::park_timeout(Duration::from_secs(1));
            }
        })?;
    *state.monitor.lock().unwrap() = Some(worker);
    Ok(())
}

fn restore(app: &AppHandle, view_state: bool) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = app.emit("meeting-lifecycle-changed", ());
        if view_state {
            let _ = app.emit("tray-open-state", ());
        }
    }
}

fn begin_action(app: &AppHandle, text: &str) -> Option<Arc<TrayState>> {
    let state = app.state::<Arc<TrayState>>().inner().clone();
    if state
        .busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return None;
    }
    let _ = state.status.set_text(text);
    let _ = state.stop.set_enabled(false);
    let _ = state.exit.set_enabled(false);
    Some(state)
}

fn stop_recording(app: &AppHandle) {
    let Some(state) = begin_action(app, "Finalizando gravação…") else {
        return;
    };
    let manager = app.state::<Arc<MeetingManager>>().inner().clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = manager.stop_meeting_and_transcribe(None);
        let _ = handle.emit("meeting-lifecycle-changed", ());
        state.busy.store(false, Ordering::Release);
        if let Err(error) = result {
            report_error(&handle, &error);
        }
    });
}

fn stop_monitor(state: &TrayState) {
    state.monitor_stop.store(true, Ordering::Release);
    if let Ok(mut monitor) = state.monitor.lock() {
        if let Some(worker) = monitor.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}

fn request_exit(app: &AppHandle) {
    let Some(state) = begin_action(app, "Aguardando saída segura…") else {
        return;
    };
    let manager = app.state::<Arc<MeetingManager>>().inner().clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result: Result<bool, String> = (|| {
            let activity = manager.activity()?;
            let needs_confirmation = activity.recording || activity.processing;
            if needs_confirmation && !confirm_exit(&handle, activity.recording) {
                return Ok(false);
            }
            manager.shutdown(needs_confirmation)?;
            Ok(true)
        })();
        match result {
            Ok(true) => {
                stop_monitor(&state);
                state.exit_allowed.store(true, Ordering::Release);
                handle.exit(0);
            }
            Ok(false) => state.busy.store(false, Ordering::Release),
            Err(error) => {
                state.busy.store(false, Ordering::Release);
                report_error(&handle, &error);
            }
        }
    });
}

#[cfg(windows)]
fn confirm_exit(app: &AppHandle, recording: bool) -> bool {
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{
                MessageBoxW, IDYES, MB_DEFBUTTON2, MB_ICONWARNING, MB_SETFOREGROUND, MB_YESNO,
            },
        },
    };
    let owner = app
        .get_webview_window("main")
        .and_then(|window| window.hwnd().ok())
        .map(|handle| HWND(handle.0));
    let message = if recording {
        "Há uma gravação ativa. Finalizar e salvar a gravação, cancelar eventual processamento e sair?\n\nEscolha Não para continuar gravando."
    } else {
        "Há processamento em andamento. Cancelar, preservar os arquivos e textos já salvos e sair?"
    };
    let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = "Meeting Recorder — confirmar saída"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe {
        MessageBoxW(
            owner,
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2 | MB_SETFOREGROUND,
        ) == IDYES
    }
}
#[cfg(not(windows))]
fn confirm_exit(_app: &AppHandle, _recording: bool) -> bool {
    false
}

fn report_error(app: &AppHandle, error: &str) {
    app.state::<crate::storage::StorageManager>().log().failure(
        "application_shutdown_failed",
        None,
        error,
    );
    let _ = app.emit(
        "application-error",
        "Não foi possível finalizar com segurança. O aplicativo continua aberto.",
    );
    #[cfg(windows)]
    {
        use windows::{
            core::PCWSTR,
            Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK, MB_SETFOREGROUND},
        };
        let message: Vec<u16> = format!(
            "Não foi possível finalizar com segurança. O aplicativo continua aberto.\n\n{error}"
        )
        .encode_utf16()
        .chain(Some(0))
        .collect();
        let title: Vec<u16> = "Meeting Recorder".encode_utf16().chain(Some(0)).collect();
        unsafe {
            MessageBoxW(
                None,
                PCWSTR(message.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
            );
        }
    }
    #[cfg(not(windows))]
    let _ = error;
}

pub(crate) fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    if window.label() == "main" {
        if let WindowEvent::CloseRequested { api, .. } = event {
            if !window
                .state::<Arc<TrayState>>()
                .exit_allowed
                .load(Ordering::Acquire)
            {
                api.prevent_close();
                let _ = window.hide();
            }
        }
    }
}

pub(crate) fn on_run_event(app: &AppHandle, event: RunEvent) {
    match event {
        RunEvent::ExitRequested { api, code, .. } => {
            if !app
                .state::<Arc<TrayState>>()
                .exit_allowed
                .load(Ordering::Acquire)
            {
                api.prevent_exit();
                if code.is_some() {
                    request_exit(app);
                }
            }
        }
        RunEvent::Exit => stop_monitor(app.state::<Arc<TrayState>>().inner()),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tray_shows_recording_duration_partial_failure_processing_and_idle() {
        let mut activity = ApplicationActivity {
            recording: true,
            elapsed_seconds: 3661,
            ..Default::default()
        };
        assert_eq!(status_text(&activity), "● Gravando 01:01:01");
        activity.partial_failure = true;
        assert_eq!(status_text(&activity), "● Gravando com falha 01:01:01");
        activity.recording = false;
        activity.processing = true;
        assert_eq!(status_text(&activity), "Processando transcrição");
        activity.processing = false;
        assert_eq!(status_text(&activity), "Pronto para gravar");
    }
}
