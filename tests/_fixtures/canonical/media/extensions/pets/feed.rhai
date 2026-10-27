// The `feed` action: one more meal, and when, kept in the store; the
// kennel is asked again to show the pet content.
fn answer(input, context) {
    let kept = store();
    #{
        data: (),
        store: #{ fed: (kept.fed ?? 0) + 1, last_fed: clock() },
        refresh: ["kennel"],
    }
}
