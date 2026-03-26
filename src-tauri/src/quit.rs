// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Quit confirmation dialog — platform-specific implementations.

use tauri::AppHandle;

use crate::AppStateExt;
use skill_data::util::MutexExt;

/// Show a quit confirmation dialog on a background thread.
/// If the user confirms and an update is staged, relaunch to apply it;
/// otherwise exit normally.
pub(crate) fn confirm_and_quit(app: AppHandle) {
    let (lang, update_ready) = {
        let s = app.app_state();
        let g = s.lock_or_recover();
        (g.ui.language.clone(), g.update_ready_to_install)
    };

    #[cfg(target_os = "macos")]
    {
        std::thread::spawn(move || {
            if quit_confirmed(&lang, &app) {
                if update_ready {
                    eprintln!("[updater] update staged — relaunching to apply");
                    app.request_restart();
                } else {
                    app.exit(0);
                }
            }
        });
    }

    #[cfg(not(target_os = "macos"))]
    {
        if quit_confirmed(&lang, &app) {
            if update_ready {
                eprintln!("[updater] update staged — relaunching to apply");
                app.request_restart();
            } else {
                app.exit(0);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn quit_confirmed(lang: &str, app: &AppHandle) -> bool {
    use tauri::Manager;
    let (title, description) = quit_dialog_strings(lang);
    let mut dialog = rfd::MessageDialog::new()
        .set_title(title)
        .set_description(description)
        .set_level(rfd::MessageLevel::Info)
        .set_buttons(rfd::MessageButtons::YesNo);
    // Set the parent window so the dialog appears focused / modal
    if let Some(win) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&win);
    }
    dialog.show() == rfd::MessageDialogResult::Yes
}

#[cfg(target_os = "macos")]
fn quit_confirmed(lang: &str, _app: &AppHandle) -> bool {
    use dispatch2::DispatchQueue;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn};
    use objc2_foundation::NSString;

    let (title, description) = quit_dialog_strings(lang);
    let mut confirmed = false;
    DispatchQueue::main().exec_sync(|| {
        // SAFETY: This closure runs on the main thread via DispatchQueue::main().exec_sync.
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(title));
        alert.setInformativeText(&NSString::from_str(description));
        alert.addButtonWithTitle(&NSString::from_str("Yes"));
        alert.addButtonWithTitle(&NSString::from_str("No"));
        confirmed = alert.runModal() == NSAlertFirstButtonReturn;
    });
    confirmed
}

fn quit_dialog_strings(lang: &str) -> (&'static str, &'static str) {
    match lang {
        "de" => (
            "NeuroSkill™ beenden",
            "Möchten Sie NeuroSkill™ wirklich beenden?",
        ),
        "fr" => (
            "Quitter NeuroSkill™",
            "Voulez-vous vraiment quitter NeuroSkill™ ?",
        ),
        "es" => (
            "Salir de NeuroSkill™",
            "¿Seguro que quieres salir de NeuroSkill™?",
        ),
        "he" => (
            "לצאת מ-NeuroSkill™",
            "האם אתה בטוח שברצונך לצאת מ-NeuroSkill™?",
        ),
        "uk" => (
            "Вийти з NeuroSkill™",
            "Ви впевнені, що хочете вийти з NeuroSkill™?",
        ),
        _ => (
            "Quit NeuroSkill™",
            "Are you sure you want to quit NeuroSkill™?",
        ),
    }
}
