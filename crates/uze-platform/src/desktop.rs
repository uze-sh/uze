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

#[cfg(target_os = "macos")]
mod imp {
    use super::ColorScheme;

    /// `open` is the one opener there.
    pub(super) const URL_OPENERS: &[&str] = &["open"];

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

    use super::ColorScheme;

    /// `explorer.exe` is how WSL reaches the Windows browser.
    pub(super) const URL_OPENERS: &[&str] = &["xdg-open", "sensible-browser", "explorer.exe"];

    const PERSONALIZE: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

    /// Under WSL the person's desktop is Windows', read through its own
    /// `reg.exe`; elsewhere the freedesktop portal, which every desktop
    /// environment answers for its own setting, then GNOME's key for one
    /// running without the portal. With no display there is no desktop.
    pub(super) fn color_scheme() -> Option<ColorScheme> {
        if std::env::var_os("WSL_DISTRO_NAME").is_some() {
            return windows_setting();
        }
        if std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("DISPLAY").is_none() {
            return None;
        }
        portal_setting().or_else(gnome_setting)
    }

    fn windows_setting() -> Option<ColorScheme> {
        // Interop puts Windows' directories on `PATH`, unless it was told
        // not to; the drive is where WSL mounts it by default.
        ["reg.exe", "/mnt/c/Windows/System32/reg.exe"]
            .into_iter()
            .find_map(|program| {
                let output = quiet(Command::new(program).args([
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
    use windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;

    use super::ColorScheme;
    use crate::win::registry_dword;

    /// `explorer.exe` hands an address to the default browser, with none of
    /// the `cmd /C start` parsing that splits one at its `&`.
    pub(super) const URL_OPENERS: &[&str] = &["explorer.exe"];

    /// The setting Settings → Personalization → Colors writes for apps.
    pub(super) fn color_scheme() -> Option<ColorScheme> {
        match registry_dword(
            HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "AppsUseLightTheme",
        )? {
            0 => Some(ColorScheme::Dark),
            1 => Some(ColorScheme::Light),
            _ => None,
        }
    }
}
