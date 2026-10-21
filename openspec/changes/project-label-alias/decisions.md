## D1 — After a label is written, read every label again rather than patch the model

- status:   pending
- task:     3.2
- commit:
- trigger:  the design says the model changes only on a written answer, but not how the written label reaches it
- decided:  a successful write asks for every label again (`ask_project_labels`); each read carries the number it was asked under and only the latest is kept, the way recent's checkout reads are
- rejected: patching `ProjectLabels` in place from the write's answer: a second mutation path beside the read, and a hand-written key in another spelling would have to be matched again in the client
- blast:    none beyond the revert; the read is one small file

## D2 — An approval raised by a placement or a delivery is named by its own path

- status:   pending
- task:     2.2
- commit:
- trigger:  approvals reach the model from three places, and only the read where a project opens runs on a thread that can resolve a root
- decided:  that read sends the root beside the question (`AwaitingRead`); a question a placement or a delivery raises falls back to its own path, which is the primary checkout and is its project's root unless an anchor sits above the repository
- rejected: resolving the root on the UI thread at absorb, which `model.projects` already does for agents' directories: it walks the filesystem where the design says the reads do
- blast:    a labelled project nested under an outer `agents.yaml` reads its directory's name in those two toasts until the next read where it opens

## D3 — An unlabelled project in recent keeps the name of the path the work recorded

- status:   pending
- task:     2.1
- commit:
- trigger:  recent's in-place work records the pane's directory, which may sit below the project's root
- decided:  the label is looked up under the resolved root; with none, the name is the recorded path's own directory, as recent said before this change
- rejected: naming the resolved root's directory when there is no label: a visible change to recent the spec does not ask for
- blast:    none

## D4 — `uze_core::config` gains no single-key getter for a nested table

- status:   pending
- task:     1.1
- commit:
- trigger:  the task listed get, set and remove; nothing reads one label alone
- decided:  `strings_by_table` lists every label, `set_in_table` and `remove_in_table` write; the task's wording was narrowed to match
- rejected: a `get_in_table` with no caller
- blast:    none
