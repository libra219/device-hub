use gpui_kit::{App, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::{
    core::w,
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{Shell::*, WindowsAndMessaging::*},
    },
};

const CALLBACK: u32 = WM_APP + 0x171;
const READY: *const u16 = w!("DeviceHub.TrayReady");
const SHOW: usize = 1;
const QUIT: usize = 2;

pub fn install(window: &mut Window, cx: &mut App) -> bool {
    let Ok(RawWindowHandle::Win32(handle)) =
        HasWindowHandle::window_handle(window).map(|h| h.as_raw())
    else {
        return false;
    };
    let hwnd = handle.hwnd.get() as HWND;
    unsafe {
        let recreated = RegisterWindowMessageW(w!("TaskbarCreated"));
        if recreated == 0 || SetWindowSubclass(hwnd, Some(callback), 1, recreated as usize) == 0 {
            return false;
        }
        // Explorer runs at normal integrity even when Device HUB runs elevated.
        for message in [CALLBACK, recreated] {
            ChangeWindowMessageFilterEx(hwnd, message, MSGFLT_ALLOW, std::ptr::null_mut());
        }
        add_icon(hwnd);
        cx.on_app_quit(move |_| {
            remove_icon(hwnd);
            std::future::ready(())
        })
        .detach();
        true
    }
}

unsafe fn add_icon(hwnd: HWND) {
    unsafe {
        let mut icon = icon_data(hwnd);
        icon.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        icon.uCallbackMessage = CALLBACK;
        icon.hIcon = LoadIconW(GetModuleHandleW(std::ptr::null()), 1 as *const u16);
        if icon.hIcon.is_null() {
            icon.hIcon = LoadIconW(std::ptr::null_mut(), IDI_APPLICATION);
        }
        let tip = "Device HUB".encode_utf16().collect::<Vec<_>>();
        icon.szTip[..tip.len()].copy_from_slice(&tip);
        if Shell_NotifyIconW(NIM_ADD, &icon) != 0 && SetPropW(hwnd, READY, 1 as _) != 0 {
            return;
        }
        // Keep the settings accessible when Explorer cannot host the icon.
        RemovePropW(hwnd, READY);
        ShowWindow(hwnd, SW_SHOW);
    }
}

fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    }
}

unsafe fn remove_icon(hwnd: HWND) {
    unsafe {
        RemovePropW(hwnd, READY);
        Shell_NotifyIconW(NIM_DELETE, &icon_data(hwnd));
    }
}

unsafe fn show(hwnd: HWND) {
    unsafe {
        ShowWindow(
            hwnd,
            if IsIconic(hwnd) != 0 {
                SW_RESTORE
            } else {
                SW_SHOW
            },
        );
        SetForegroundWindow(hwnd);
    }
}

unsafe fn menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        AppendMenuW(menu, MF_STRING, SHOW, w!("アプリを表示"));
        AppendMenuW(menu, MF_STRING, QUIT, w!("アプリを終了"));
        let mut point = POINT::default();
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            hwnd,
            std::ptr::null(),
        );
        DestroyMenu(menu);
        PostMessageW(hwnd, WM_NULL, 0, 0);
        match command as usize {
            SHOW => show(hwnd),
            QUIT => {
                remove_icon(hwnd);
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
            _ => {}
        }
    }
}

unsafe extern "system" fn callback(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    recreated: usize,
) -> LRESULT {
    unsafe {
        if msg == recreated as u32 {
            add_icon(hwnd);
            return 0;
        }
        if msg == CALLBACK && wp == 1 {
            match lp as u32 {
                WM_LBUTTONUP => show(hwnd),
                WM_RBUTTONUP => menu(hwnd),
                _ => {}
            }
            return 0;
        }
        if msg == WM_CLOSE && !GetPropW(hwnd, READY).is_null() {
            ShowWindow(hwnd, SW_HIDE);
            return 0;
        }
        if msg == WM_NCDESTROY {
            remove_icon(hwnd);
            RemoveWindowSubclass(hwnd, Some(callback), id);
        }
        DefSubclassProc(hwnd, msg, wp, lp)
    }
}
