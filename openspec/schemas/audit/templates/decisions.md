<!-- Decisions taken while implementing this change that the plan did not
     make. Append-only during a run. The agent writes entries and never
     edits `status`; the reviewer writes `status` and never rewrites an
     entry. Delete this comment block when the first entry is added. -->

## D1 — <!-- the decision in one line, imperative -->

- status:   pending <!-- pending | accepted | overturned | blocked -->
- task:     <!-- the tasks.md item this came from, e.g. 2.4 -->
- commit:   <!-- sha of the commit holding it; the revert point. Left
             empty until that commit exists, then filled by the next one. -->
- trigger:  <!-- what the task list did not anticipate -->
- decided:  <!-- what was done -->
- rejected: <!-- the alternative, and why not -->
- blast:    <!-- what reverting this would cost, beyond the revert itself -->
