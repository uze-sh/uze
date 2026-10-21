//! `uze usage`: what uze uses on this machine, the processes the
//! workspace runs and the disk uze holds.

use crate::*;

/// What uze uses on this machine. The server samples
/// its processes while the disk is walked, so the second the sample takes
/// is spent inside the walk.
pub(crate) fn report_usage(app: &UzeApplication, format: OutputFormat) -> Result<()> {
    let measured = with_spinner("measuring what uze uses", || {
        Ok(std::thread::scope(|scope| {
            let processes = scope.spawn(uze_terminal::usage);
            let disk = app.workspace().disk();
            (processes.join(), disk)
        }))
    })?;
    let (processes, disk) = measured;
    let processes = processes
        .unwrap_or_else(|_| {
            Err(uze_terminal::RuntimeError::Protocol(
                "the usage sample panicked".into(),
            ))
        })
        .map_err(terminal_error)?;
    let report = UsageOutput::new(uze_platform::usage::machine(), processes, &disk);
    emit(format, &report, render_usage);
    Ok(())
}

/// What `uze usage` reports, as its JSON says it.
#[derive(serde::Serialize)]
pub(crate) struct UsageOutput {
    machine: MachineOutput,
    workspace: WorkspaceOutput,
    processes: Option<ProcessesOutput>,
    disk: DiskOutput,
}

#[derive(serde::Serialize)]
struct MachineOutput {
    system: String,
    architecture: &'static str,
    cores: usize,
    memory: Option<u64>,
}

#[derive(serde::Serialize)]
struct WorkspaceOutput {
    state: WorkspaceState,
    /// Seconds since the Unix epoch.
    started: Option<u64>,
}

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum WorkspaceState {
    Running,
    NotRunning,
    OtherBuild,
}

#[derive(serde::Serialize)]
struct ProcessesOutput {
    agents: LoadOutput,
    shells: LoadOutput,
    server: LoadOutput,
    clients: LoadOutput,
    total: LoadOutput,
}

#[derive(Clone, Copy, Default, serde::Serialize)]
struct LoadOutput {
    /// How many tabs, or clients, it covers; none for the server and the
    /// total, where a count would add unlike things.
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<u32>,
    /// CPU time spent per second of the window: `2.0` is two cores busy.
    cpu_seconds_per_second: f64,
    /// That as a share of the whole machine, 0 to 1.
    cpu_share: f64,
    memory: u64,
    memory_estimated: bool,
    processes: u32,
}

impl LoadOutput {
    fn of(load: &uze_terminal::Load, window: std::time::Duration, cores: usize) -> Self {
        let per_second = load.cpu.as_secs_f64() / window.as_secs_f64().max(f64::EPSILON);
        Self {
            count: Some(load.count),
            cpu_seconds_per_second: per_second,
            cpu_share: per_second / cores.max(1) as f64,
            memory: load.memory,
            memory_estimated: load.estimated,
            processes: load.processes,
        }
    }

    fn plus(self, other: Self) -> Self {
        Self {
            count: None,
            cpu_seconds_per_second: self.cpu_seconds_per_second + other.cpu_seconds_per_second,
            cpu_share: self.cpu_share + other.cpu_share,
            memory: self.memory + other.memory,
            memory_estimated: self.memory_estimated || other.memory_estimated,
            processes: self.processes + other.processes,
        }
    }
}

#[derive(serde::Serialize)]
struct DiskOutput {
    checkouts: CheckoutsOutput,
    packages: u64,
    records: u64,
    generated: u64,
    cache: u64,
    settings: u64,
    total: u64,
}

#[derive(serde::Serialize)]
struct CheckoutsOutput {
    count: usize,
    bytes: u64,
}

impl UsageOutput {
    fn new(
        machine: uze_platform::usage::Machine,
        processes: uze_terminal::Usage,
        disk: &uze_application::DiskView,
    ) -> Self {
        use uze_application::HomeTier;
        let tier = |wanted: HomeTier| {
            disk.home
                .iter()
                .find(|(tier, _)| *tier == wanted)
                .map_or(0, |(_, bytes)| *bytes)
        };
        let (state, report) = match processes {
            uze_terminal::Usage::Running(report) => (WorkspaceState::Running, Some(report)),
            uze_terminal::Usage::NotRunning => (WorkspaceState::NotRunning, None),
            uze_terminal::Usage::OtherBuild => (WorkspaceState::OtherBuild, None),
        };
        let processes = report.as_ref().map(|report| {
            let of = |load| LoadOutput::of(load, report.window, machine.cores);
            let (agents, shells, server, clients) = (
                of(&report.agents),
                of(&report.shells),
                of(&report.server),
                of(&report.clients),
            );
            ProcessesOutput {
                agents,
                shells,
                server: LoadOutput {
                    count: None,
                    ..server
                },
                clients,
                total: agents.plus(shells).plus(server).plus(clients),
            }
        });
        Self {
            machine: MachineOutput {
                system: machine.system,
                architecture: machine.architecture,
                cores: machine.cores,
                memory: machine.memory,
            },
            workspace: WorkspaceOutput {
                state,
                started: report.map(|report| report.started),
            },
            processes,
            disk: DiskOutput {
                checkouts: CheckoutsOutput {
                    count: disk.checkouts,
                    bytes: disk.checkout_bytes,
                },
                packages: tier(HomeTier::Packages),
                records: tier(HomeTier::Records),
                generated: tier(HomeTier::Generated),
                cache: tier(HomeTier::Cache),
                settings: tier(HomeTier::Settings),
                total: disk.total(),
            },
        }
    }
}

pub(crate) fn render_usage(report: &UsageOutput) -> String {
    let machine = &report.machine;
    let mut head = vec![
        vec![
            progress::label("machine"),
            format!("{} · {}", machine.system, machine.architecture),
        ],
        vec![progress::label("cpu"), count(machine.cores, "core")],
    ];
    if let Some(memory) = machine.memory {
        head.push(vec![progress::label("memory"), bytes_to_say(memory)]);
    }
    head.push(vec![
        progress::label("workspace"),
        match report.workspace.state {
            WorkspaceState::Running => report
                .workspace
                .started
                .map_or_else(|| "running".to_owned(), uptime_to_say),
            WorkspaceState::NotRunning => progress::label("not running"),
            WorkspaceState::OtherBuild => {
                format!("{} started by another build", progress::warning_icon())
            }
        },
    ]);
    let mut text = progress::report_title("uze usage", None);
    text.push('\n');
    text.push_str(&progress::aligned_rows(head));
    text.push_str("\n\n");
    let disk = &report.disk;
    let size = |name: String, bytes: u64| vec![name, bytes_to_say(bytes)];
    let disk_rows = vec![
        vec![String::new(), progress::label("disk")],
        size(
            format!("checkouts ({})", disk.checkouts.count),
            disk.checkouts.bytes,
        ),
        size("packages".to_owned(), disk.packages),
        size("records".to_owned(), disk.records),
        size("generated".to_owned(), disk.generated),
        size("cache".to_owned(), disk.cache),
        size("settings".to_owned(), disk.settings),
        size(progress::title("total"), disk.total),
    ];
    if let Some(processes) = &report.processes {
        let row = |name: String, load: &LoadOutput| {
            vec![
                name,
                format!("{:.1}%", load.cpu_share * 100.0),
                format!(
                    "{}{}",
                    if load.memory_estimated { "~" } else { "" },
                    bytes_to_say(load.memory)
                ),
                load.processes.to_string(),
            ]
        };
        let counted =
            |name: &str, load: &LoadOutput| format!("{name} ({})", load.count.unwrap_or_default());
        // One table: the disk's sizes stand in the column the CPU does, so
        // both blocks share its edges rather than each finding its own.
        let tables = progress::figure_groups(vec![
            vec![
                vec![
                    String::new(),
                    progress::label("cpu"),
                    progress::label("memory"),
                    progress::label("processes"),
                ],
                row(counted("agents", &processes.agents), &processes.agents),
                row(counted("shells", &processes.shells), &processes.shells),
                row("terminal server".to_owned(), &processes.server),
                row(counted("clients", &processes.clients), &processes.clients),
                row(progress::title("total"), &processes.total),
            ],
            disk_rows,
        ]);
        text.push_str(&tables.join("\n\n"));
    } else {
        if report.workspace.state == WorkspaceState::OtherBuild {
            text.push_str(&format!(
                "  {}\n\n",
                progress::next_step("uze workspace stop")
            ));
        }
        text.push_str(&progress::figure_groups(vec![disk_rows]).concat());
    }
    text.push('\n');
    text
}

/// `up 3h 12m · since 09:14`; past a day, the clock time alone would
/// name the wrong day, so only how long.
fn uptime_to_say(started: u64) -> String {
    let now = uze_platform::clock::since_epoch().as_secs();
    let up = now.saturating_sub(started);
    let (days, hours, minutes) = (up / 86_400, up % 86_400 / 3600, up % 3600 / 60);
    let long = match (days, hours) {
        (0, 0) => format!("{minutes}m"),
        (0, _) => format!("{hours}h {minutes}m"),
        _ => format!("{days}d {hours}h"),
    };
    let since = (days == 0)
        .then(|| uze_platform::clock::seconds_into_local_day(started))
        .flatten()
        .map(|seconds| format!(" · since {:02}:{:02}", seconds / 3600, seconds % 3600 / 60))
        .unwrap_or_default();
    format!("up {long}{since}")
}

/// `812 KB`, `1.4 GB`: the unit that keeps the number short, as the
/// workspace's own surfaces say a size.
fn bytes_to_say(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 || value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod usage_tests {
    use super::*;
    use std::time::Duration;

    fn load(count: u32, cpu_ms: u64, memory: u64, processes: u32) -> uze_terminal::Load {
        uze_terminal::Load {
            count,
            cpu: Duration::from_millis(cpu_ms),
            memory,
            estimated: false,
            processes,
        }
    }

    /// The machine's share, not one core's: two busy cores of four over
    /// one second read 50%, and the total adds every row.
    #[test]
    fn usage_reads_cpu_as_the_machines_share_and_totals_every_row() {
        let report = uze_terminal::UsageReport {
            started: uze_platform::clock::since_epoch().as_secs() - (3 * 3600 + 12 * 60),
            window: Duration::from_secs(1),
            agents: load(3, 2_000, 4 << 30, 46),
            shells: load(2, 0, 12 << 20, 4),
            server: load(1, 0, 31 << 20, 1),
            clients: load(2, 0, 17 << 20, 2),
        };
        let machine = uze_platform::usage::Machine {
            system: "linux".into(),
            architecture: "x86_64",
            cores: 4,
            memory: Some(16 << 30),
        };
        let output = UsageOutput::new(
            machine,
            uze_terminal::Usage::Running(report),
            &uze_application::DiskView::default(),
        );

        let text = progress::unpainted(&render_usage(&output));

        assert!(text.contains("4 cores"), "{text}");
        assert!(text.contains("16.0 GB"), "{text}");
        assert!(text.contains("up 3h 12m"), "{text}");
        let agents = text
            .lines()
            .find(|line| line.contains("agents (3)"))
            .unwrap();
        assert!(
            agents.contains("50.0%") && agents.contains("4.0 GB"),
            "{agents}"
        );
        let total = text
            .lines()
            .find(|line| line.trim_start().starts_with("total") && line.contains('%'))
            .unwrap();
        assert!(total.contains("50.0%") && total.contains("53"), "{total}");
    }
}
