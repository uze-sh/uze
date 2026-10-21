//! The `[projects]` section: the name the operator gives a project, in
//! place of its directory's.
//!
//! Machine-scoped, like the theme: the name someone calls a project by is
//! their own, so it lives in their `config.toml` and never in the project,
//! where everyone who clones it would inherit it. A project is keyed by its
//! root, so every checkout of it reads the one label.

use std::path::{Path, PathBuf};

use crate::{Result, config, home::UzeHome};

const SECTION: &str = "projects";
const LABEL: &str = "label";

/// Every label `config.toml` holds, by the root it was written under.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectLabels {
    labels: Vec<(PathBuf, String)>,
}

impl ProjectLabels {
    /// The label of the project at `root`. A key written by hand spelled
    /// otherwise, with a trailing separator or by the platform's own rules
    /// of case and separators, still matches.
    pub fn of(&self, root: &Path) -> Option<&str> {
        self.labels
            .iter()
            .find(|(key, _)| names_the_root(key, root))
            .map(|(_, label)| label.as_str())
    }

    /// The name a surface calls the project at `root` by: its label, else
    /// its directory's own name.
    pub fn name_of(&self, root: &Path) -> String {
        self.of(root)
            .map_or_else(|| directory_name(root), str::to_owned)
    }

    /// Every key naming `root`, as the file spells it.
    fn keys_of(&self, root: &Path) -> Vec<String> {
        self.labels
            .iter()
            .filter(|(key, _)| names_the_root(key, root))
            .map(|(key, _)| key.to_string_lossy().into_owned())
            .collect()
    }
}

fn names_the_root(key: &Path, root: &Path) -> bool {
    let tidy = |path: &Path| path.components().collect::<PathBuf>();
    uze_platform::path::same_path(&tidy(key), &tidy(root))
}

pub fn labels(home: &UzeHome) -> Result<ProjectLabels> {
    let labels = config::strings_by_table(home, SECTION, LABEL)?
        .into_iter()
        .map(|(key, label)| (PathBuf::from(key), label))
        .collect();
    Ok(ProjectLabels { labels })
}

/// Names the project at `root` `label`, and answers the label in force
/// afterwards. Only a name other than the directory's is kept: an empty
/// one, or the directory's own, takes the label away, and taking away a
/// label that is not there writes nothing. The label is written under the
/// first key the file already spells the root with, and every other key
/// naming it is taken away, so one root never holds two labels.
pub fn set_label(home: &UzeHome, root: &Path, label: &str) -> Result<Option<String>> {
    let label = label.trim();
    let mut keys = labels(home)?.keys_of(root).into_iter();
    let kept = (!label.is_empty() && label != directory_name(root)).then(|| {
        keys.next()
            .unwrap_or_else(|| root.to_string_lossy().into_owned())
    });
    for key in keys {
        config::remove_in_table(home, SECTION, &key, LABEL)?;
    }
    let Some(key) = kept else {
        return Ok(None);
    };
    config::set_in_table(home, SECTION, &key, LABEL, label)?;
    Ok(Some(label.to_owned()))
}

/// A project's name when it has no label: the directory's own.
pub fn directory_name(root: &Path) -> String {
    root.file_name().map_or_else(
        || root.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn home(label: &str) -> UzeHome {
        UzeHome::at(uze_testkit::temp::scratch(label))
    }

    #[test]
    fn a_project_with_no_label_is_named_by_its_directory() {
        let home = home("labels-none");
        let labels = labels(&home).expect("readable");
        assert_eq!(labels.name_of(Path::new("/work/uze")), "uze");
    }

    #[test]
    fn a_label_survives_being_written_and_read_back() {
        let home = home("labels-written");
        let root = Path::new("/work/uze");

        let in_force = set_label(&home, root, "  uze core ").expect("written");

        assert_eq!(in_force.as_deref(), Some("uze core"));
        assert_eq!(labels(&home).unwrap().name_of(root), "uze core");
    }

    #[test]
    fn a_label_written_by_hand_is_read() {
        let home = home("labels-by-hand");
        fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
        fs::write(
            home.config_path(),
            "[projects.'/work/uze']\nlabel = \"uze core\"\n",
        )
        .unwrap();

        assert_eq!(
            labels(&home).unwrap().of(Path::new("/work/uze")),
            Some("uze core")
        );
    }

    #[test]
    fn an_empty_label_gives_the_directory_name_back() {
        let home = home("labels-cleared");
        let root = Path::new("/work/uze");
        set_label(&home, root, "uze core").unwrap();

        assert_eq!(set_label(&home, root, "   ").expect("cleared"), None);

        assert_eq!(labels(&home).unwrap().name_of(root), "uze");
        assert_eq!(fs::read_to_string(home.config_path()).unwrap(), "");
    }

    #[test]
    fn the_directory_name_is_not_kept_as_a_label() {
        let home = home("labels-directory-name");

        assert_eq!(
            set_label(&home, Path::new("/work/uze"), "uze").expect("read"),
            None
        );

        assert!(!home.config_path().exists());
    }

    #[test]
    fn a_rename_replaces_the_key_written_by_hand_rather_than_adding_one() {
        let home = home("labels-same-key");
        fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
        fs::write(
            home.config_path(),
            "[projects.'/work/uze']\nlabel = \"old\" # mine\n",
        )
        .unwrap();

        set_label(&home, Path::new("/work/uze"), "new").unwrap();

        assert_eq!(
            fs::read_to_string(home.config_path()).unwrap(),
            "[projects.'/work/uze']\nlabel = \"new\" # mine\n"
        );
    }

    #[test]
    fn a_key_written_with_a_trailing_separator_is_read() {
        let home = home("labels-trailing");
        fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
        fs::write(
            home.config_path(),
            "[projects.'/work/uze/']\nlabel = \"uze core\"\n",
        )
        .unwrap();

        assert_eq!(
            labels(&home).unwrap().of(Path::new("/work/uze")),
            Some("uze core")
        );

        set_label(&home, Path::new("/work/uze"), "renamed").unwrap();

        assert_eq!(
            fs::read_to_string(home.config_path()).unwrap(),
            "[projects.'/work/uze/']\nlabel = \"renamed\"\n",
            "the key written by hand is the one renamed"
        );
    }

    #[test]
    fn clearing_takes_every_spelling_of_the_root_away() {
        let home = home("labels-two-spellings");
        fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
        fs::write(
            home.config_path(),
            "[projects.'/work/uze']\nlabel = \"a\"\n[projects.'/work/uze/']\nlabel = \"b\"\n",
        )
        .unwrap();

        set_label(&home, Path::new("/work/uze"), "").unwrap();

        assert_eq!(labels(&home).unwrap().of(Path::new("/work/uze")), None);
        assert_eq!(fs::read_to_string(home.config_path()).unwrap(), "");
    }

    #[test]
    fn nothing_reaches_the_project() {
        let home = home("labels-project-untouched");
        let project = uze_testkit::temp::scratch("labels-project");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("agents.yaml"), "plugins: []\n").unwrap();
        let before = listing(&project);

        set_label(&home, &project, "mine").unwrap();
        set_label(&home, &project, "").unwrap();

        assert_eq!(listing(&project), before);
        assert_eq!(
            fs::read_to_string(project.join("agents.yaml")).unwrap(),
            "plugins: []\n"
        );
    }

    fn listing(directory: &Path) -> Vec<PathBuf> {
        let mut entries: Vec<PathBuf> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        entries.sort();
        entries
    }
}
