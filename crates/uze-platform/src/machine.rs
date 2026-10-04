//! What about this machine keeps UZE from working as it should, said the
//! way a person acts on it. Asked by `uze doctor`; nothing here decides
//! anything for UZE itself.

/// One thing about this machine a person should know, and what to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Concern {
    /// What it is about, as `doctor` names it: `windows`, `path`.
    pub subject: &'static str,
    /// What is the matter, and what to do about it.
    pub detail: String,
}

/// Everything this machine has that UZE does not work well with: on
/// Windows a build older than UZE supports, Smart App Control enforcing,
/// and the directory `uze` runs from missing from the `Path` a new shell
/// gets. Nothing on Unix, whose equivalents the installer settles.
pub fn concerns() -> Vec<Concern> {
    imp::concerns()
}

#[cfg(unix)]
mod imp {
    pub(super) fn concerns() -> Vec<super::Concern> {
        Vec::new()
    }
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;

    use super::Concern;
    use crate::win::{registry_dword, registry_string};

    /// Windows 10 22H2, the oldest build with the console UZE draws on and
    /// still serviced.
    const OLDEST_BUILD: u32 = 19045;

    pub(super) fn concerns() -> Vec<Concern> {
        [build(), smart_app_control(), path()]
            .into_iter()
            .flatten()
            .collect()
    }

    fn build() -> Option<Concern> {
        let build: u32 = registry_string(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "CurrentBuildNumber",
            0,
        )
        .ok()
        .flatten()?
        .to_str()?
        .parse()
        .ok()?;
        (build < OLDEST_BUILD).then(|| Concern {
            subject: "windows",
            detail: format!("build {build}; uze needs Windows 10 22H2 ({OLDEST_BUILD}) or later"),
        })
    }

    /// `VerifiedAndReputablePolicyState`: 0 off, 1 enforcing, 2 evaluating.
    fn smart_app_control() -> Option<Concern> {
        let state = registry_dword(
            HKEY_LOCAL_MACHINE,
            r"SYSTEM\CurrentControlSet\Control\CI\Policy",
            "VerifiedAndReputablePolicyState",
        )?;
        (state == 1).then(|| Concern {
            subject: "smart app control",
            detail: "on; it can refuse a uze it has not seen before, an upgrade \
                     included (see the installation guide)"
                .to_owned(),
        })
    }

    fn path() -> Option<Concern> {
        let directory = std::env::current_exe().ok()?.parent()?.to_path_buf();
        let path = crate::environment::path_of_a_new_shell()?;
        (!crate::environment::holds(&path, &directory)).then(|| Concern {
            subject: "path",
            detail: format!(
                "{} is not on the Path a new shell gets; run the installer again",
                directory.display()
            ),
        })
    }
}
