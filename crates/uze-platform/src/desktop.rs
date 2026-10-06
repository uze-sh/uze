//! The person's desktop: what opens a web address in their browser, and
//! whether they asked it for light or dark.

/// The programs that open a web address in the person's browser, in the
/// order they are tried, each taking the address as its last argument.
pub const URL_OPENERS: &[&str] = imp::URL_OPENERS;

/// What the person set their desktop to draw in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorScheme {
    Light,
    Dark,
}

/// The desktop's light-or-dark setting, as a browser reads it for
/// `prefers-color-scheme`. `None` where there is no desktop to ask, or it
/// expresses no preference. Under WSL the desktop is Windows'.
///
/// Asking may start a process (`defaults`, `reg.exe`, `gdbus`), tens of
/// milliseconds: not for a path that is budgeted per command.
pub fn color_scheme() -> Option<ColorScheme> {
    imp::color_scheme()
}

/// Every change to the desktop's light-or-dark setting, starting with the
/// setting as it is now. Blocks between changes, so it belongs on a thread
/// of its own; ends where there is no desktop to watch.
///
/// Each platform is watched the cheapest way it allows. Windows signals a
/// change to the registry key and the freedesktop portal emits one, so
/// neither is asked again until something changed. macOS and WSL offer
/// nothing this process can wait on — one needs a Cocoa run loop, the other
/// a Windows process of its own — so they are asked every few seconds.
pub fn color_scheme_changes() -> ColorSchemeChanges {
    ColorSchemeChanges {
        settings: imp::settings(),
        last: None,
    }
}

/// See [`color_scheme_changes`].
pub struct ColorSchemeChanges {
    settings: imp::Settings,
    last: Option<ColorScheme>,
}

impl Iterator for ColorSchemeChanges {
    type Item = ColorScheme;

    fn next(&mut self) -> Option<ColorScheme> {
        distinct(&mut self.settings, &mut self.last)
    }
}

/// The next setting unlike the last one handed out. A watch wakes on any
/// write to where the setting lives and a poll on a clock; neither is a
/// change by itself.
fn distinct(
    settings: &mut impl Iterator<Item = ColorScheme>,
    last: &mut Option<ColorScheme>,
) -> Option<ColorScheme> {
    let next = settings.find(|setting| Some(*setting) != *last)?;
    *last = Some(next);
    Some(next)
}

/// Asking a desktop that cannot say when it changed, on a clock.
#[cfg(unix)]
mod polling {
    use std::time::Duration;

    use super::ColorScheme;

    /// About as soon as a person who flipped the setting looks back, and
    /// rarely enough that a process every few seconds costs nothing.
    const EVERY: Duration = Duration::from_secs(5);

    pub(super) struct Polling {
        asked_yet: bool,
    }

    impl Polling {
        pub(super) fn now() -> Self {
            Self { asked_yet: false }
        }
    }

    impl Iterator for Polling {
        type Item = ColorScheme;

        fn next(&mut self) -> Option<ColorScheme> {
            loop {
                if self.asked_yet {
                    std::thread::sleep(EVERY);
                }
                self.asked_yet = true;
                if let Some(setting) = super::color_scheme() {
                    return Some(setting);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_change_is_handed_out() {
        use ColorScheme::{Dark, Light};
        let mut seen = [Dark, Dark, Light, Light, Light, Dark].into_iter();
        let mut last = None;
        let changes: Vec<ColorScheme> =
            std::iter::from_fn(|| distinct(&mut seen, &mut last)).collect();
        assert_eq!(changes, [Dark, Light, Dark]);
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::ColorScheme;

    /// `open` is the one opener there.
    pub(super) const URL_OPENERS: &[&str] = &["open"];

    pub(super) type Settings = super::polling::Polling;

    pub(super) fn settings() -> Settings {
        Settings::now()
    }

    /// `AppleInterfaceStyle` is `Dark` while the desktop is dark, the
    /// automatic setting included, and absent while it is light.
    pub(super) fn color_scheme() -> Option<ColorScheme> {
        let output = crate::tools::system("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        Some(interface_style(
            output.status.success(),
            &String::from_utf8_lossy(&output.stdout),
        ))
    }

    fn interface_style(set: bool, value: &str) -> ColorScheme {
        if set && value.trim().eq_ignore_ascii_case("dark") {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn only_a_dark_interface_style_is_dark() {
            assert_eq!(interface_style(true, "Dark\n"), ColorScheme::Dark);
            assert_eq!(interface_style(false, ""), ColorScheme::Light);
            assert_eq!(interface_style(true, "Light\n"), ColorScheme::Light);
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use std::process::{Command, Stdio};

    use super::{ColorScheme, polling::Polling};

    /// `explorer.exe` is how WSL reaches the Windows browser.
    pub(super) const URL_OPENERS: &[&str] = &["xdg-open", "sensible-browser", "explorer.exe"];

    const PERSONALIZE: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

    /// Under WSL the person's desktop is Windows', read through its own
    /// `reg.exe`; elsewhere the freedesktop portal, which every desktop
    /// environment answers for its own setting, then GNOME's key for one
    /// running without the portal. With no display there is no desktop.
    pub(super) fn color_scheme() -> Option<ColorScheme> {
        if under_wsl() {
            return windows_setting();
        }
        if !has_display() {
            return None;
        }
        portal_setting().or_else(gnome_setting)
    }

    fn under_wsl() -> bool {
        std::env::var_os("WSL_DISTRO_NAME").is_some()
    }

    fn has_display() -> bool {
        std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some()
    }

    pub(super) enum Settings {
        Nothing,
        Polling(Polling),
        Portal { monitor: Monitor, asked_yet: bool },
    }

    /// The portal's signal where a desktop runs one; WSL's Windows and a
    /// desktop with no `gdbus` on a clock; nothing without a display.
    pub(super) fn settings() -> Settings {
        if under_wsl() {
            return Settings::Polling(Polling::now());
        }
        if !has_display() {
            return Settings::Nothing;
        }
        match Monitor::start() {
            Some(monitor) => Settings::Portal {
                monitor,
                asked_yet: false,
            },
            None => Settings::Polling(Polling::now()),
        }
    }

    impl Iterator for Settings {
        type Item = ColorScheme;

        fn next(&mut self) -> Option<ColorScheme> {
            match self {
                Self::Nothing => None,
                Self::Polling(polling) => polling.next(),
                Self::Portal { monitor, asked_yet } => {
                    // The monitor is started before the first read, so a
                    // change between the two is heard rather than missed.
                    if !*asked_yet {
                        *asked_yet = true;
                        if let Some(setting) = color_scheme() {
                            return Some(setting);
                        }
                    }
                    if let Some(setting) = monitor.next_change() {
                        return Some(setting);
                    }
                    // The monitor went away: the desktop is still there to
                    // be asked, on a clock.
                    *self = Self::Polling(Polling::now());
                    self.next()
                }
            }
        }
    }

    /// `gdbus monitor` on the portal: one process waiting on the session
    /// bus, rather than one started every few seconds.
    pub(super) struct Monitor {
        child: std::process::Child,
        lines: std::io::Lines<std::io::BufReader<std::process::ChildStdout>>,
    }

    impl Monitor {
        fn start() -> Option<Self> {
            use std::{io::BufRead as _, os::unix::process::CommandExt as _};
            let mut command = crate::tools::system("gdbus");
            command
                .args([
                    "monitor",
                    "--session",
                    "--dest",
                    "org.freedesktop.portal.Desktop",
                    "--object-path",
                    "/org/freedesktop/portal/desktop",
                ])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .stdout(Stdio::piped());
            // Safety: `prctl` is async-signal-safe, which is all a
            // `pre_exec` hook may call between fork and exec. The monitor
            // ends with the thread that started it, which is the watching
            // thread, which lives as long as the process: it never outlives
            // UZE waiting on a bus nobody reads.
            unsafe {
                command.pre_exec(|| {
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                    Ok(())
                });
            }
            let mut child = command.spawn().ok()?;
            let stdout = child.stdout.take()?;
            Some(Self {
                child,
                lines: std::io::BufReader::new(stdout).lines(),
            })
        }

        fn next_change(&mut self) -> Option<ColorScheme> {
            self.lines
                .by_ref()
                .map_while(Result::ok)
                .find_map(|line| setting_changed(&line))
        }
    }

    impl Drop for Monitor {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// `/org/freedesktop/portal/desktop: org.freedesktop.portal.Settings.SettingChanged
    /// ('org.freedesktop.appearance', 'color-scheme', <uint32 1>)`.
    fn setting_changed(line: &str) -> Option<ColorScheme> {
        (line.contains(".SettingChanged ")
            && line.contains("'org.freedesktop.appearance', 'color-scheme'"))
        .then(|| portal_color_scheme(line))
        .flatten()
    }

    fn windows_setting() -> Option<ColorScheme> {
        // Interop puts Windows' directories on `PATH`, unless it was told
        // not to; the drive is where WSL mounts it by default.
        ["reg.exe", "/mnt/c/Windows/System32/reg.exe"]
            .into_iter()
            .find_map(|program| {
                let output = quiet(crate::tools::system(program).args([
                    "query",
                    PERSONALIZE,
                    "/v",
                    "AppsUseLightTheme",
                ]))?;
                apps_use_light_theme(&output)
            })
    }

    fn portal_setting() -> Option<ColorScheme> {
        let output = quiet(crate::tools::system("gdbus").args([
            "call",
            "--session",
            "--timeout",
            "1",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Settings.Read",
            "org.freedesktop.appearance",
            "color-scheme",
        ]))?;
        portal_color_scheme(&output)
    }

    fn gnome_setting() -> Option<ColorScheme> {
        let output = quiet(crate::tools::system("gsettings").args([
            "get",
            "org.gnome.desktop.interface",
            "color-scheme",
        ]))?;
        gnome_color_scheme(&output)
    }

    /// What a command printed, when it ran and succeeded.
    fn quiet(command: &mut Command) -> Option<String> {
        let output = command
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// `AppsUseLightTheme    REG_DWORD    0x1`.
    fn apps_use_light_theme(output: &str) -> Option<ColorScheme> {
        let line = output
            .lines()
            .find(|line| line.contains("AppsUseLightTheme"))?;
        match line.split_whitespace().last()? {
            "0x0" => Some(ColorScheme::Dark),
            "0x1" => Some(ColorScheme::Light),
            _ => None,
        }
    }

    /// `(<<uint32 1>>,)`: 1 prefers dark, 2 prefers light, 0 says nothing.
    fn portal_color_scheme(output: &str) -> Option<ColorScheme> {
        let value = output.split("uint32").nth(1)?;
        let digits: String = value
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        match digits.as_str() {
            "1" => Some(ColorScheme::Dark),
            "2" => Some(ColorScheme::Light),
            _ => None,
        }
    }

    /// `'prefer-dark'`, `'prefer-light'` or `'default'`, which says nothing.
    fn gnome_color_scheme(output: &str) -> Option<ColorScheme> {
        match output.trim().trim_matches('\'') {
            "prefer-dark" => Some(ColorScheme::Dark),
            "prefer-light" => Some(ColorScheme::Light),
            _ => None,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn windows_says_light_or_dark_in_one_dword() {
            let printed = |value: &str| {
                format!(
                    "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\r\n    \
                     AppsUseLightTheme    REG_DWORD    {value}\r\n\r\n"
                )
            };
            assert_eq!(
                apps_use_light_theme(&printed("0x0")),
                Some(ColorScheme::Dark)
            );
            assert_eq!(
                apps_use_light_theme(&printed("0x1")),
                Some(ColorScheme::Light)
            );
            assert_eq!(apps_use_light_theme(&printed("0x2")), None);
            assert_eq!(apps_use_light_theme(""), None);
        }

        #[test]
        fn the_portal_answers_in_either_wrapping() {
            assert_eq!(
                portal_color_scheme("(<<uint32 1>>,)\n"),
                Some(ColorScheme::Dark)
            );
            assert_eq!(
                portal_color_scheme("(<uint32 2>,)\n"),
                Some(ColorScheme::Light)
            );
            assert_eq!(portal_color_scheme("(<<uint32 0>>,)\n"), None);
            assert_eq!(portal_color_scheme("()"), None);
        }

        #[test]
        fn only_the_portals_color_scheme_signal_is_a_change() {
            let signal = |namespace: &str, key: &str, value: &str| {
                format!(
                    "/org/freedesktop/portal/desktop: org.freedesktop.portal.Settings.SettingChanged \
                     ('{namespace}', '{key}', <uint32 {value}>)"
                )
            };
            assert_eq!(
                setting_changed(&signal("org.freedesktop.appearance", "color-scheme", "2")),
                Some(ColorScheme::Light)
            );
            assert_eq!(
                setting_changed(&signal("org.freedesktop.appearance", "color-scheme", "1")),
                Some(ColorScheme::Dark)
            );
            assert_eq!(
                setting_changed(&signal("org.freedesktop.appearance", "accent-color", "1")),
                None
            );
            assert_eq!(
                setting_changed(&signal("org.gnome.desktop.interface", "color-scheme", "1")),
                None
            );
            assert_eq!(
                setting_changed("The name :1.42 is now owned by :1.42"),
                None
            );
        }

        #[test]
        fn gnome_default_is_no_preference() {
            assert_eq!(
                gnome_color_scheme("'prefer-dark'\n"),
                Some(ColorScheme::Dark)
            );
            assert_eq!(
                gnome_color_scheme("'prefer-light'\n"),
                Some(ColorScheme::Light)
            );
            assert_eq!(gnome_color_scheme("'default'\n"), None);
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsStr;

    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, REG_NOTIFY_CHANGE_LAST_SET, RegCloseKey,
        RegNotifyChangeKeyValue, RegOpenKeyExW,
    };

    use super::ColorScheme;
    use crate::win::{registry_dword, wide};

    /// `explorer.exe` hands an address to the default browser, with none of
    /// the `cmd /C start` parsing that splits one at its `&`.
    pub(super) const URL_OPENERS: &[&str] = &["explorer.exe"];

    const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

    /// The setting Settings → Personalization → Colors writes for apps.
    pub(super) fn color_scheme() -> Option<ColorScheme> {
        match registry_dword(HKEY_CURRENT_USER, PERSONALIZE, "AppsUseLightTheme")? {
            0 => Some(ColorScheme::Dark),
            1 => Some(ColorScheme::Light),
            _ => None,
        }
    }

    /// The key that setting lives in, opened to be told when it is written.
    pub(super) struct Settings {
        key: Option<HKEY>,
        asked_yet: bool,
    }

    // SAFETY: a registry key handle is usable from any thread.
    unsafe impl Send for Settings {}

    pub(super) fn settings() -> Settings {
        let path = wide(OsStr::new(PERSONALIZE));
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: `path` is NUL-terminated; `key` is written on success.
        let opened =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_NOTIFY, &mut key) };
        Settings {
            key: (opened == 0).then_some(key),
            asked_yet: false,
        }
    }

    impl Iterator for Settings {
        type Item = ColorScheme;

        fn next(&mut self) -> Option<ColorScheme> {
            let key = self.key?;
            loop {
                if self.asked_yet {
                    // Blocks until a value under the key is written; the
                    // watch is one-shot, so it is asked for again each time.
                    // SAFETY: `key` is open for the life of `self`.
                    let status = unsafe {
                        RegNotifyChangeKeyValue(
                            key,
                            0,
                            REG_NOTIFY_CHANGE_LAST_SET,
                            std::ptr::null_mut(),
                            0,
                        )
                    };
                    if status != 0 {
                        return None;
                    }
                }
                self.asked_yet = true;
                if let Some(setting) = color_scheme() {
                    return Some(setting);
                }
            }
        }
    }

    impl Drop for Settings {
        fn drop(&mut self) {
            if let Some(key) = self.key {
                // SAFETY: opened by `settings`, closed once.
                unsafe { RegCloseKey(key) };
            }
        }
    }
}
