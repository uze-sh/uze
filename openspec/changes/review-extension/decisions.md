## D1 — Hand the host a change to apply under the lock, not a record to write

- status:   pending
- task:     1.6
- commit:   a9c249d5
- trigger:  the design named `write_review_record`; a whole record written back would overwrite an answer the agent wrote between the surface's read and its write
- decided:  `Host::update_review_record(checkout, &mut dyn FnMut(&mut ReviewRecord))`: the host reads, applies the surface's changes and writes under the review lock, and the surface replays its unwritten changes over every record it reads
- rejected: a whole-record write with a version check and retry, which moves the merge into the extension and still loses an answer on a conflict nobody retries
- blast:    the extension's write path and the host's two implementations

## D2 — Re-read the record on the list's cadence rather than by its modification time

- status:   pending
- task:     8.3
- commit:   be4d1359
- trigger:  the host offers no stat of a file under `$UZE_HOME`, and the review already reads its list every 1.5 s
- decided:  the list job reads the record with the list; the door's job reads it every 2 s for the checkout in front
- rejected: a new `Host` capability for the record's modification time, for a file of a few kilobytes read alongside a `git status`
- blast:    none beyond the read itself

## D3 — Insert at the caret, since the composer has no selection of its own

- status:   pending
- task:     6.4
- commit:   f3cd8e15
- trigger:  the spec asked the controls to fence or wrap "the selected text", but a terminal composer's text is selected by the host for copying, never handed to the extension as a range
- decided:  `{ } code block` inserts the marked diff lines as a fenced block at the caret; `` ` code `` inserts a pair of backticks with the caret between them; the spec's requirement and scenario say so
- rejected: a selection inside the composer (shift with the arrows), which adds a text-editing model to a surface whose comments are a few lines
- blast:    the spec's "A comment carries code" requirement

## D4 — Say viewed and the comment actions in words

- status:   pending
- task:     6.5, 7.1
- commit:   (the commit after 8210ec7d)
- trigger:  `no_chrome_glyph_is_written_where_it_is_drawn` refuses ✓ ● ○ × │ in an extension: the host draws chrome
- decided:  the list's detail says `viewed` or `changed since viewed`, the chip says `viewed`, `mark viewed` or `changed since viewed`, a comment's actions are `edit`, `delete`, `resolve`, and a comment is set in by air rather than a drawn rule; only the comment mark (◆, `view::COMMENT_MARK`) is drawn by the host
- rejected: a `RowMark`-like kind for the navigator's trailing marker, which changes every `NavigatorRow::Item` in the tree for one surface
- blast:    the review's look; a later change may give the navigator a viewed kind the host draws

## D5 — Name the commands in the send line, not in the launch instructions

- status:   pending
- task:     9.4
- commit:   (the commit after 8210ec7d)
- trigger:  a bullet on reading and answering a review pushed the longest launch instructions past their 2,000-byte share (`the_longest_policy_fits_its_share_uncut`)
- decided:  the one line a send types names `uze agent review show` and `reply <send>/<n>`, which is where an agent meets them; the `uze:worktree` skill explains them
- rejected: raising `INSTRUCTIONS_SHARE`, which takes room from the briefing and memory every launch carries, for a verb only a reviewed agent uses
- blast:    none

## D6 — The review discards through its own job; the code surface no longer discards

- status:   pending
- task:     2.3, 10.1
- commit:   069b9728
- trigger:  the design kept discard with the changed row and named two callers of `restore_to_head`; with the changes gone from `code`, its discard had no row to stand on
- decided:  `restore_to_head` is called from the review's job alone, which the architecture suite now holds; `FileRequest::Restore` is gone
- rejected: keeping the code surface's restore request for a gesture nothing offers
- blast:    the `extensions` rule's sentence on write capabilities

## D7 — Offer the filter beside the viewed chip, over the diff

- status:   pending
- task:     3.4
- commit:   f3cd8e15
- trigger:  the design put `all · uncommitted` on the surface's mode row, but a view's top row is the host's, and its `subjects` stand over the navigator
- decided:  `all`, `uncommitted` and the viewed chip are the content's modes, on the diff's heading row
- rejected: the navigator's subjects, which read as what the list is of rather than how the diff is taken
- blast:    the review's look

## D8 — Leave the agent's reply out of the journey

- status:   pending
- task:     11.5
- commit:   6efe958c
- trigger:  a journey's shell holds no agent's launch key, which `uze agent review reply` requires, and the stand-in harness does not run commands
- decided:  the journey proves the draft kept, the line typed once allowed and the comment sent; `show` and `reply`, with their refusals, are proven by `uze-workspace`'s review tests against a real task store
- rejected: printing a launch key into the world for the journey to use, which weakens the claim the key exists to make
- blast:    none

## D9 — The code tree no longer marks changed files

- status:   pending
- task:     2.3
- commit:   069b9728
- trigger:  the tree's status markers were read from the changes half, which left
- decided:  the tree marks nothing; the review lists what changed
- rejected: a `git status` read kept in the code surface for its markers, which is the review's read made twice
- blast:    a small loss of information in the tree; the map still marks what is uncommitted
