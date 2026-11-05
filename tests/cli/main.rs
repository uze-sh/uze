//! CLI layer tests (L1/L2): the real `uze` binary against isolated
//! environments — grammar/precedence (ADR-019) and machine-scoped
//! commands. Project-scoped CLI semantics live in `workspace/` and
//! `acceptance/`.

mod agent_report;
mod budget;
// Each platform's confinement is proved against its own kernel, with the
// programs that platform ships: macOS's is proved through the lifecycle
// suite, where its programs run confined.
#[cfg(target_os = "linux")]
mod confine;
// Windows confines in an AppContainer, proved with cmd.exe and PowerShell.
#[cfg(windows)]
mod confine_windows;
mod extensions;
mod grammar;
mod machine;
mod portable_shell;
// Unix only: what is ended is a process group, watched through a FIFO; Windows
// ends a Job Object, proven in uze-platform.
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod workspace_stop;
