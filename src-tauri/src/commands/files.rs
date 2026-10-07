use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::{database::Database, meeting::details, storage::StorageManager};

#[tauri::command]
pub async fn get_meeting_audio(
    database: State<'_, Database>,
    storage: State<'_, StorageManager>,
    app: AppHandle,
    id: String,
) -> Result<Option<String>, String> {
    let database = database.inner().clone();
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(path) = details::audio_path(&database, &storage, &id).inspect_err(|error| {
            storage
                .log()
                .failure("playback_file_failed", Some(&id), error)
        })?
        else {
            return Ok(None);
        };
        app.asset_protocol_scope()
            .allow_file(&path)
            .map_err(|error| error.to_string())?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn export_transcript(
    database: State<'_, Database>,
    storage: State<'_, StorageManager>,
    window: WebviewWindow,
    id: String,
    traditional: Option<bool>,
) -> Result<bool, String> {
    let database = database.inner().clone();
    let storage = storage.inner().clone();
    #[cfg(windows)]
    let owner = window.hwnd().map_err(|error| error.to_string())?.0 as isize;
    #[cfg(not(windows))]
    let owner = {
        let _ = window;
        0
    };
    tauri::async_runtime::spawn_blocking(move || {
        let traditional = traditional.unwrap_or(false);
        let prepared =
            details::prepare_txt_export(&database, &id, traditional).inspect_err(|error| {
                storage
                    .log()
                    .failure("export_prepare_failed", Some(&id), error)
            })?;
        let destination = save_destination(owner, &prepared.filename).inspect_err(|error| {
            storage
                .log()
                .failure("export_dialog_failed", Some(&id), error)
        })?;
        prepared.export_to(&storage, destination.as_deref())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn open_meeting_folder(
    database: State<'_, Database>,
    storage: State<'_, StorageManager>,
    id: String,
) -> Result<(), String> {
    let database = database.inner().clone();
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let directory = details::folder_path(&database, &storage, &id).inspect_err(|error| {
            storage
                .log()
                .failure("meeting_folder_failed", Some(&id), error)
        })?;
        open_folder(&directory).inspect_err(|error| {
            storage
                .log()
                .failure("meeting_folder_open_failed", Some(&id), error)
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(windows)]
fn save_destination(
    owner: isize,
    suggested_name: &str,
) -> Result<Option<std::path::PathBuf>, String> {
    use std::os::windows::ffi::OsStringExt;
    use windows::{
        core::{w, PWSTR},
        Win32::{
            Foundation::HWND,
            UI::Controls::Dialogs::{
                CommDlgExtendedError, GetSaveFileNameW, OFN_DONTADDTORECENT, OFN_NOCHANGEDIR,
                OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
            },
        },
    };
    let mut filename = vec![0_u16; 32_768];
    let default = suggested_name.encode_utf16().collect::<Vec<_>>();
    filename[..default.len()].copy_from_slice(&default);
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: HWND(owner as *mut std::ffi::c_void),
        lpstrFilter: w!("Texto UTF-8 (*.txt)\0*.txt\0\0"),
        nFilterIndex: 1,
        lpstrFile: PWSTR(filename.as_mut_ptr()),
        nMaxFile: filename.len() as u32,
        lpstrTitle: w!("Exportar transcrição"),
        lpstrDefExt: w!("txt"),
        Flags: OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_DONTADDTORECENT,
        ..Default::default()
    };
    // The dialog owns no buffers: filename and all constant strings live through this call.
    if !unsafe { GetSaveFileNameW(&mut dialog) }.as_bool() {
        let error = unsafe { CommDlgExtendedError() };
        return if error.0 == 0 {
            Ok(None)
        } else {
            Err(format!(
                "Não foi possível abrir o diálogo de exportação ({:#x}).",
                error.0
            ))
        };
    }
    let length = filename
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(filename.len());
    Ok(Some(
        std::ffi::OsString::from_wide(&filename[..length]).into(),
    ))
}

#[cfg(windows)]
fn shell_path(path: &std::path::Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    // canonicalize uses the extended filesystem prefix, which the Windows Shell
    // does not consistently accept. Strip it only at this Shell boundary.
    let prefix: Vec<u16> = r"\\?\".encode_utf16().collect();
    if !wide.starts_with(&prefix) {
        return wide;
    }
    let remainder = &wide[prefix.len()..];
    let unc: Vec<u16> = r"UNC\".encode_utf16().collect();
    if remainder.starts_with(&unc) {
        r"\\"
            .encode_utf16()
            .chain(remainder[unc.len()..].iter().copied())
            .collect()
    } else {
        remainder.to_vec()
    }
}

#[cfg(windows)]
fn open_folder(path: &std::path::Path) -> Result<(), String> {
    use windows::{
        core::{w, PCWSTR},
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    };
    let path = shell_path(path)
        .into_iter()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(path.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        Err("Não foi possível abrir a pasta no Explorador de Arquivos.".to_owned())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn save_destination(
    _owner: isize,
    _suggested_name: &str,
) -> Result<Option<std::path::PathBuf>, String> {
    Err("Disponível apenas no Windows.".to_owned())
}
#[cfg(not(windows))]
fn open_folder(_path: &std::path::Path) -> Result<(), String> {
    Err("Disponível apenas no Windows.".to_owned())
}

#[cfg(all(test, windows))]
mod tests {
    use super::shell_path;
    use std::path::Path;

    #[test]
    fn shell_paths_preserve_unicode_and_normalize_local_and_unc_prefixes() {
        let decode = |path| String::from_utf16(&shell_path(Path::new(path))).unwrap();
        assert_eq!(decode(r"\\?\C:\Reuniões\ação"), r"C:\Reuniões\ação");
        assert_eq!(
            decode(r"\\?\UNC\server\share\reunião"),
            r"\\server\share\reunião"
        );
        assert_eq!(decode(r"C:\Reuniões\ação"), r"C:\Reuniões\ação");
    }
}
