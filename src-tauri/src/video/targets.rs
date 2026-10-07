use serde::Serialize;
use windows::{
    core::BOOL,
    Win32::{
        Foundation::{HWND, LPARAM, RECT},
        Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW},
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowLongW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
            IsWindowVisible, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
        },
    },
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoSource {
    pub id: String,
    pub title: String,
    pub kind: String,
}
pub(crate) fn list_sources() -> Result<Vec<VideoSource>, String> {
    let mut sources = Vec::<VideoSource>::new();
    unsafe {
        EnumWindows(
            Some(window_callback),
            LPARAM(&mut sources as *mut _ as isize),
        )
        .map_err(|e| e.to_string())?;
        if !EnumDisplayMonitors(
            None,
            None,
            Some(monitor_callback),
            LPARAM(&mut sources as *mut _ as isize),
        )
        .as_bool()
        {
            return Err("Não foi possível listar monitores.".into());
        }
    }
    sources.sort_by(|a, b| (&a.kind, &a.title).cmp(&(&b.kind, &b.title)));
    Ok(sources)
}
pub(crate) fn select_source(id: &str) -> Result<VideoSource, String> {
    list_sources()?
        .into_iter()
        .find(|source| source.id == id)
        .ok_or_else(|| "A janela ou monitor selecionado não está mais disponível.".into())
}
unsafe extern "system" fn window_callback(hwnd: HWND, data: LPARAM) -> BOOL {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool()
            || GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0
        {
            return BOOL(1);
        }
        let mut title = [0_u16; 512];
        let length = GetWindowTextW(hwnd, &mut title);
        let mut rect = RECT::default();
        if length > 0
            && GetWindowRect(hwnd, &mut rect).is_ok()
            && rect.right > rect.left
            && rect.bottom > rect.top
        {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            // EnumWindows invokes this synchronously; data refers to the caller's live Vec.
            let sources = &mut *(data.0 as *mut Vec<VideoSource>);
            sources.push(VideoSource {
                id: format!("window:{}:{pid}", hwnd.0 as usize),
                title: String::from_utf16_lossy(&title[..length as usize]),
                kind: "window".into(),
            });
        }
    }
    BOOL(1)
}
unsafe extern "system" fn monitor_callback(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    unsafe {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(monitor, &mut info.monitorInfo).as_bool() {
            let length = info
                .szDevice
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(info.szDevice.len());
            let sources = &mut *(data.0 as *mut Vec<VideoSource>);
            sources.push(VideoSource {
                id: format!("monitor:{}", monitor.0 as usize),
                title: format!(
                    "Monitor {} — {}×{}",
                    String::from_utf16_lossy(&info.szDevice[..length]),
                    info.monitorInfo.rcMonitor.right - info.monitorInfo.rcMonitor.left,
                    info.monitorInfo.rcMonitor.bottom - info.monitorInfo.rcMonitor.top
                ),
                kind: "monitor".into(),
            });
        }
    }
    BOOL(1)
}
