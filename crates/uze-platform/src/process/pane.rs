//! A pane's processes as one unit: what the terminal runtime starts in a
//! pane, how it starts it so nothing escapes the unit, which of its
//! processes a person is talking to, and ending it.

use std::{
    ffi::OsString,
    io,
    sync::atomic::{AtomicBool, Ordering},
};

use super::imp;

/// A pane's processes as one unit: the program a pane runs and everything
/// it starts, ended together when the pane closes.
pub struct Group {
    unit: imp::Group,
    mark: Mark,
    ended: AtomicBool,
}

/// A program to start so that it belongs to its [`Group`] from its first
/// instant.
pub struct Grouped {
    /// What to start in its place.
    pub argv: Vec<OsString>,
    /// The group, made before the program starts; `None` where it is taken
    /// once the program runs ([`Group::adopt`]).
    pub group: Option<Group>,
    /// What the program must be started with in its environment.
    pub mark: Mark,
}

/// An environment variable only one unit's processes carry. Every process
/// inherits its environment, so a process that left the unit (a session of
/// its own, then orphaned) still carries it, and ending the unit finds it
/// by it where the platform would otherwise lose it.
#[derive(Clone, Debug)]
pub struct Mark {
    variable: String,
    value: String,
}

impl Mark {
    pub fn variable(&self) -> &str {
        &self.variable
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

/// How to start `argv` inside a group of its own. On Unix as it is: the
/// program leads a process group from its first instant, and
/// [`Group::adopt`] takes it after. Windows puts a process in a Job Object
/// only once it has started, and whatever it started in between would be
/// outside; so `host` (a program that answers with [`host_grouped`]) is
/// started in its place, given the group's name and `argv`, and joins the
/// group before it starts `argv`. With no `host`, `argv` is started as it
/// is and adopted after, there too.
///
/// The unit's [`Mark`] is `variable`, set to a value no other unit has.
pub fn grouped(
    argv: Vec<OsString>,
    host: Option<&[OsString]>,
    variable: &str,
) -> io::Result<Grouped> {
    let (argv, group) = imp::grouped(argv, host)?;
    let mark = Mark {
        variable: variable.to_owned(),
        value: crate::secret::token()?,
    };
    Ok(Grouped {
        argv,
        group: group.map(|unit| Group::of(unit, mark.clone())),
        mark,
    })
}

/// The other half of [`grouped`]: joins the group named `name`, then runs
/// `argv` on this process's console and standard streams and ends with its
/// exit code, as if it had been started in this one's place.
pub fn host_grouped(name: &str, argv: &[OsString]) -> io::Error {
    imp::host_grouped(name, argv)
}

/// One live process of a [`Group`], as [`foreground`] reads it.
#[derive(Clone, Debug)]
pub struct Member {
    pub pid: u32,
    /// The process that started it, where the platform says.
    pub parent: Option<u32>,
    /// Its short command name ([`crate::probe::command_name_of`]).
    pub name: String,
}

/// The process a person at the terminal `leader` was started on is talking
/// to. Unix keeps it: the terminal's foreground process group, which a
/// shell gives each command it runs and the command's own children share,
/// so an agent stays in front while it runs `git`. Windows' ConPTY keeps no
/// such thing, so it is found the way the console was passed: from the
/// process `group` started first, down through each that `passes_on` (a
/// shell running a command, a launcher running a program) to the first
/// that keeps it. What that one starts is its own work, never the
/// foreground; the newest process was, and it was an agent's `git` as
/// often as the agent.
pub fn foreground(
    leader: u32,
    group: Option<&Group>,
    passes_on: impl Fn(&Member, &Member) -> bool,
) -> Option<u32> {
    imp::foreground(leader, group.map(|group| &group.unit), passes_on)
}

/// Whether `pid` is one of the processes of the pane whose program is
/// `leader`, however far from that program it went: what tells a request
/// made from inside a pane from one made by the person at the terminal.
///
/// On Unix the pane's program leads a session of its own, which every
/// process it starts stays in unless it leaves on purpose, and one that
/// leaves is still found below `leader` while the process that started it
/// lives. On Windows the pane's Job Object holds every process started in
/// it, and nothing leaves a job that was not made to let it.
pub fn belongs(pid: u32, leader: u32, group: Option<&Group>) -> bool {
    imp::belongs(pid, leader, group.map(|group| &group.unit))
}

impl Group {
    /// Takes `pid` (just spawned with `mark`) and what it will start as
    /// one unit, or `None` where it cannot be: on Unix a program that does
    /// not lead a group of its own, which a group signal would then
    /// overreach.
    pub fn adopt(pid: u32, mark: &Mark) -> Option<Self> {
        imp::Group::adopt(pid).map(|unit| Self::of(unit, mark.clone()))
    }

    fn of(unit: imp::Group, mark: Mark) -> Self {
        Self {
            unit,
            mark,
            ended: AtomicBool::new(false),
        }
    }

    /// Ends every process in the unit, once, and every process that left
    /// it carrying its [`Mark`]: call it before the leader is reaped. On
    /// Unix the unit is named by the leader's pid, which a reaped leader
    /// with nothing left in its group gives back for reuse, and a second
    /// signal could then reach a stranger's group.
    pub fn end(&self) {
        if !self.ended.swap(true, Ordering::SeqCst) {
            self.unit.end();
            imp::end_strays(&self.mark.variable, &self.mark.value);
        }
    }

    /// The processes in the unit, oldest first, where the platform lists
    /// them; empty where it does not.
    pub fn members(&self) -> Vec<u32> {
        self.unit.members()
    }
}
